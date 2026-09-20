//-
// Copyright 2017, 2018, 2019, 2024 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::fmt;
use core::iter;
use core::marker::PhantomData;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::SeqCst;
#[cfg(feature = "fork")]
use std::env;
#[cfg(feature = "fork")]
use std::fs;
#[cfg(feature = "std")]
use std::panic;
#[cfg(feature = "std")]
use std::panic::AssertUnwindSafe;
#[cfg(feature = "fork")]
use std::path::PathBuf;
#[cfg(any(feature = "timeout", all(feature = "std", not(target_arch = "wasm32"))))]
use std::time::Instant;

#[cfg(feature = "fork")]
use rusty_fork;
#[cfg(feature = "fork")]
use rusty_fork::fork_test;
#[cfg(feature = "fork")]
use rusty_fork::rusty_fork_id;
#[cfg(feature = "fork")]
use tempfile;

use crate::std_facade::Arc;
use crate::std_facade::BTreeMap;
use crate::std_facade::Box;
#[cfg(feature = "std")]
use crate::std_facade::String;
use crate::std_facade::Vec;
#[cfg(feature = "fork")]
use crate::std_facade::format;
#[cfg(feature = "fork")]
use crate::std_facade::vec;
use crate::strategy::Strategy;
use crate::strategy::ValueTree;
use crate::test_runner::config::Config;
use crate::test_runner::errors::TestCaseError;
use crate::test_runner::errors::TestCaseOk;
use crate::test_runner::errors::TestCaseResult;
use crate::test_runner::errors::TestCaseResultV2;
use crate::test_runner::errors::TestError;
use crate::test_runner::execution::CaseOrigin;
use crate::test_runner::execution::CaseVerdict;
use crate::test_runner::execution::Execution;
use crate::test_runner::execution::ExecutionResult;
use crate::test_runner::execution::FailingCase;
use crate::test_runner::execution::RunFailure;
use crate::test_runner::failure_persistence::PersistedSeed;
use crate::test_runner::reason::Reason;
#[cfg(feature = "fork")]
use crate::test_runner::replay;
use crate::test_runner::result_cache::EvaluationId;
use crate::test_runner::result_cache::ResultCache;
use crate::test_runner::result_cache::ResultCacheKey;
#[cfg(feature = "fork")]
use crate::test_runner::rng::Seed;
use crate::test_runner::rng::TestRng;

/// A minimized native failure pair or the execution failure reached while shrinking.
type ShrinkResult<V, X> = ExecutionResult<FailingCase<V, X>, V, X>;

/// Env-var naming the shared forkfile; set on each child and read by
/// `init_replay` to detect that it is running as a fork child.
#[cfg(feature = "fork")]
const ENV_FORK_FILE: &str = "_PROPTEST_FORKFILE";

/// Verbose level 0: messages emitted unconditionally.
const ALWAYS: u32 = 0;
/// Verbose level 1 to show failures. In state machine tests this level is used
/// to print transitions.
pub const INFO_LOG: u32 = 1;
/// Verbose level 2: low-level tracing of each case and shrink step.
const TRACE: u32 = 2;

/// Emit a `proptest:`-prefixed verbose message when the runner's
/// configured verbosity is at least `$level`, via the diagnostics seam.
#[cfg(feature = "std")]
macro_rules! verbose_message {
    ($runner:expr, $level:expr, $fmt:tt $($arg:tt)*) => {{
        if crate::test_runner::diagnostics::verbose_at_least(
            $runner.config.verbose,
            $level,
        ) {
            crate::test_runner::diagnostics::emit_verbose(
                format_args!($fmt $($arg)*),
            );
        }
    }}
}

/// No-op form of `verbose_message!`: `no_std` has no output channel, so
/// the arguments are only touched to keep them "used".
#[cfg(not(feature = "std"))]
macro_rules! verbose_message {
    ($runner:expr, $level:expr, $fmt:tt $($arg:tt)*) => {{
        match &$runner {
            _ => {}
        }
        match $level {
            _ => {}
        }
        match format_args!($fmt $($arg)*) {
            _ => {}
        }
    }};
}

/// Per-`Reason` tally of how many inputs were rejected at each site.
type RejectionDetail = BTreeMap<Reason, u32>;

/// State used when running a proptest test.
#[derive(Clone)]
pub struct TestRunner {
  /// The configuration governing this run.
  config:          Config,
  /// Count of genuinely new cases that have passed so far.
  successes:       u32,
  /// Count of inputs rejected locally (within a single case).
  local_rejects:   u32,
  /// Count of inputs rejected globally (across the whole run).
  global_rejects:  u32,
  /// The runner's random number generator.
  rng:             TestRng,
  /// Shared counter capping total `Flatten` regenerations.
  flat_map_regens: Arc<AtomicUsize>,

  /// Per-site tally of local rejections, for reporting.
  local_reject_detail:  RejectionDetail,
  /// Per-site tally of global rejections, for reporting.
  global_reject_detail: RejectionDetail,
}

impl fmt::Debug for TestRunner {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("TestRunner")
      .field("config", &self.config)
      .field("successes", &self.successes)
      .field("local_rejects", &self.local_rejects)
      .field("global_rejects", &self.global_rejects)
      .field("rng", &"<TestRng>")
      .field("flat_map_regens", &self.flat_map_regens)
      .field("local_reject_detail", &self.local_reject_detail)
      .field("global_reject_detail", &self.global_reject_detail)
      .finish()
  }
}

impl fmt::Display for TestRunner {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "\tsuccesses: {}\n\tlocal rejects: {}\n", self.successes, self.local_rejects)?;
    for (whence, count) in &self.local_reject_detail {
      writeln!(f, "\t\t{count} times at {whence}")?;
    }
    writeln!(f, "\tglobal rejects: {}", self.global_rejects)?;
    for (whence, count) in &self.global_reject_detail {
      writeln!(f, "\t\t{count} times at {whence}")?;
    }

    Ok(())
  }
}

/// Equivalent to: `TestRunner::new(Config::default())`.
impl Default for TestRunner {
  fn default() -> Self {
    Self::new(Config::default())
  }
}

#[cfg(test)]
/// Build the default unit-test runner config without failure persistence.
///
/// Unit tests that exercise generation or shrinking usually need deterministic
/// in-memory behavior, not persisted regression-file side effects.
#[must_use]
pub fn runner_test_config() -> Config {
  Config {
    failure_persistence: None,
    ..Config::default()
  }
}

#[cfg(test)]
/// Build a unit-test runner whose default config cannot write regressions.
#[must_use]
pub fn test_runner_without_persistence() -> TestRunner {
  TestRunner::new(runner_test_config())
}

/// The fork child's handle to the replay file it appends step marks to;
/// a no-op shim when not running inside a fork.
#[cfg(feature = "fork")]
#[derive(Debug)]
struct ForkOutput {
  /// The replay file to append to, or `None` when not in a fork.
  file: Option<fs::File>,
}

#[cfg(feature = "fork")]
impl ForkOutput {
  /// Append this case's outcome mark to the replay file, if forking.
  fn append(&mut self, result: &TestCaseResult) -> Result<(), Reason> {
    if let Some(ref mut file) = self.file {
      replay::append(file, result).map_err(|error| Reason::from(format!("Failed to append to replay file: {error}")))?;
    }
    Ok(())
  }

  /// Append an "I'm alive" mark so the parent sees progress.
  fn ping(&mut self) -> Result<(), Reason> {
    if let Some(ref mut file) = self.file {
      replay::ping(file).map_err(|error| Reason::from(format!("Failed to ping replay file: {error}")))?;
    }
    Ok(())
  }

  /// Append the termination mark that ends the replay log.
  fn terminate(&mut self) -> Result<(), Reason> {
    if let Some(ref mut file) = self.file {
      replay::terminate(file).map_err(|error| Reason::from(format!("Failed to terminate replay file: {error}")))?;
    }
    Ok(())
  }

  /// A `ForkOutput` that writes nowhere (not in a fork).
  const fn empty() -> Self {
    Self {
      file: None
    }
  }

  /// Whether this output is backed by a real fork replay file.
  const fn is_in_fork(&self) -> bool {
    self.file.is_some()
  }
}

/// Stand-in for `ForkOutput` when the `fork` feature is disabled: every
/// operation is a no-op and the runner is never inside a fork.
#[cfg(not(feature = "fork"))]
#[derive(Debug)]
struct ForkOutput;

#[cfg(not(feature = "fork"))]
impl ForkOutput {
  /// The only `ForkOutput` there is without forking.
  const fn empty() -> Self {
    Self
  }
  /// Always `false`: a non-fork build is never inside a fork.
  const fn is_in_fork(&self) -> bool {
    let _: &Self = self;
    false
  }
}

/// Legacy evaluation payloads, addressed by the same cache identities as typed runs.
struct LegacyCache {
  /// The caller-selected cache policy.
  cache:   Box<dyn ResultCache>,
  /// Payloads owned by this execution; cache entries only reference them.
  results: Vec<TestCaseResult>,
}

impl LegacyCache {
  /// Compute a key using the configured policy.
  fn key(&self, key: &ResultCacheKey<'_>) -> u64 {
    self.cache.key(key)
  }

  /// Borrow an existing evaluation without copying it into the cache.
  fn get(&self, key: u64) -> Option<&TestCaseResult> {
    self.cache.get(key).and_then(|evaluation| self.results.get(evaluation.0))
  }

  /// Record a legacy result and insert its identity into the cache.
  fn put(&mut self, key: u64, result: &TestCaseResult) {
    let evaluation = EvaluationId(self.results.len());
    self.results.push(result.clone());
    self.cache.put(key, evaluation);
  }
}

/// Adapts the legacy callback and marker replay to the shared native algorithm.
struct LegacyExecution<F, R> {
  /// The user callback.
  test_fn: F,
  /// Already-completed evaluations replayed from a child.
  replay:  R,
  /// Payload storage and the configured cache.
  cache:   LegacyCache,
  /// Child output, or the in-process no-op writer.
  output:  ForkOutput,
}

impl<V, F, R> Execution<V> for LegacyExecution<F, R>
where
  V: fmt::Debug,
  F: Fn(V) -> TestCaseResult,
  R: Iterator<Item = TestCaseResult>,
{
  type Failure = Reason;
  type Error = Reason;

  fn evaluate(&mut self, runner: &TestRunner, _: &mut V, input: V, origin: CaseOrigin) -> Result<CaseVerdict<Reason>, Reason> {
    Ok(
      match call_test(
        runner,
        input,
        &self.test_fn,
        &mut self.replay,
        &mut self.cache,
        &mut self.output,
        origin == CaseOrigin::Persisted,
      ) {
        Ok(passed) => CaseVerdict::Passed(passed),
        Err(TestCaseError::Reject(reason)) => CaseVerdict::Rejected(reason),
        Err(TestCaseError::Fail(reason)) => CaseVerdict::Failed(reason),
      },
    )
  }

  fn backtrack(&mut self) -> Result<(), Reason> {
    #[cfg(feature = "fork")]
    self.output.append(&Ok(()))?;
    Ok(())
  }

  fn shrink_budget(&mut self, exhausted: bool) -> Result<bool, Reason> {
    Ok(exhausted)
  }

  fn is_in_fork(&self) -> bool {
    self.output.is_in_fork()
  }
}

/// Preserve the legacy public error representation at its adapter boundary.
fn legacy_failure<V>(failure: RunFailure<V, Reason, Reason>) -> TestError<V> {
  match failure {
    RunFailure::Aborted(reason)
    | RunFailure::Engine {
      error: reason, ..
    } => TestError::Abort(reason),
    RunFailure::Falsified(reason, counterexample) => TestError::Fail(reason, counterexample),
  }
}

/// Parent-side replay status after re-reading the shared forkfile.
#[cfg(feature = "fork")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParentReplayStatus {
  /// The child wrote progress but has not yet terminated the log.
  InProgress,
  /// The replay log contains a termination mark.
  Terminated,
}

/// Current byte length of the shared fork replay file.
#[cfg(feature = "fork")]
fn forkfile_size(forkfile: &tempfile::NamedTempFile) -> Result<u64, Reason> {
  forkfile
    .as_file()
    .metadata()
    .map(|metadata| metadata.len())
    .map_err(|error| Reason::from(format!("Failed to read fork file metadata: {error}")))
}

/// Create the shared replay file used to coordinate forked child runs.
#[cfg(feature = "fork")]
#[allow(
  clippy::single_call_fn,
  reason = "create and initialize the parent replay file for the forked runner protocol"
)]
fn create_parent_replay_file(seed: Seed) -> Result<(replay::Replay, tempfile::NamedTempFile, PathBuf), Reason> {
  let replay = replay::Replay {
    seed,
    steps: vec![],
  };
  let mut forkfile =
    tempfile::NamedTempFile::new().map_err(|error| Reason::from(format!("Failed to create temporary file for fork: {error}")))?;
  replay
    .init_file(&mut forkfile)
    .map_err(|error| Reason::from(format!("Failed to initialise temporary file for fork: {error}")))?;
  let forkfile_path = forkfile.path().to_path_buf();
  Ok((replay, forkfile, forkfile_path))
}

/// Refresh the parent replay state from the shared forkfile.
#[cfg(feature = "fork")]
#[allow(
  clippy::single_call_fn,
  reason = "refresh the parent replay state from the forkfile and classify its parse status"
)]
fn refresh_parent_replay(forkfile: &mut tempfile::NamedTempFile, replay: &mut replay::Replay) -> Result<ParentReplayStatus, Reason> {
  match replay::Replay::parse_from(forkfile).map_err(|error| Reason::from(format!("Failed to re-read fork file: {error}")))? {
    replay::ReplayFileStatus::InProgress(new_replay) => {
      *replay = new_replay;
      Ok(ParentReplayStatus::InProgress)
    }
    replay::ReplayFileStatus::Terminated(new_replay) => {
      *replay = new_replay;
      Ok(ParentReplayStatus::Terminated)
    }
    replay::ReplayFileStatus::Corrupt => Err("Child process corrupted replay file".into()),
  }
}

/// Whether the parent should append a synthetic child failure to replay.
#[cfg(feature = "fork")]
#[allow(
  clippy::single_call_fn,
  reason = "name the forkfile-length rule for recording abrupt child termination"
)]
fn should_record_abrupt_child_failure(last_fork_file_len: Option<u64>, curr_forkfile_size: u64) -> bool {
  last_fork_file_len.is_none_or(|observed_len| observed_len == curr_forkfile_size)
}

/// Append a synthetic failure when the child exits without terminating replay.
#[cfg(feature = "fork")]
#[allow(
  clippy::single_call_fn,
  reason = "append the synthetic replay failure for an abruptly terminated fork child"
)]
fn record_abrupt_child_failure(
  forkfile: &mut tempfile::NamedTempFile,
  replay: &mut replay::Replay,
  child_error: Option<TestCaseError>,
) -> Result<(), Reason> {
  let error = Err(child_error.unwrap_or_else(|| TestCaseError::fail("Child process was terminated abruptly but with successful status")));
  replay::append(forkfile, &error)
    .map_err(|append_error| Reason::from(format!("Failed to append synthetic fork failure: {append_error}")))?;
  replay.steps.push(error);
  Ok(())
}

/// Run one already-generated `case` through the test closure (`no_std`
/// path).
///
/// Consults the fork-replay iterator and the result cache before
/// invoking `test_fn`, tagging the success with the kind that decides
/// whether it counts toward `cases`. Skips the panic-catching, timeout,
/// and fork-output handling of the `std` path.
#[cfg(not(feature = "std"))]
#[allow(
  clippy::single_call_fn,
  reason = "preserve the platform-specific legacy callback, cache and replay boundary behind its execution adapter"
)]
fn call_test<V, F, R>(
  _runner: &TestRunner,
  case: V,
  test_fn: &F,
  replay_from_fork: &mut R,
  result_cache: &mut LegacyCache,
  _: &mut ForkOutput,
  is_from_persisted_seed: bool,
) -> TestCaseResultV2
where
  V: fmt::Debug,
  F: Fn(V) -> TestCaseResult,
  R: Iterator<Item = TestCaseResult>,
{
  if let Some(result) = replay_from_fork.next() {
    return result.map(|()| TestCaseOk::ReplayFromForkSuccess);
  }

  let cache_key = result_cache.key(&ResultCacheKey::new(&case));
  if let Some(result) = result_cache.get(cache_key) {
    return result.clone().map(|()| TestCaseOk::CacheHitSuccess);
  }

  let result = test_fn(case);
  result_cache.put(cache_key, &result);
  result.map(|()| {
    if is_from_persisted_seed {
      TestCaseOk::PersistedCaseSuccess
    } else {
      TestCaseOk::NewCaseSuccess
    }
  })
}

/// Run one already-generated `case` through the test closure (`std`
/// path).
///
/// Replays a fork step if one is pending, else pings the fork file,
/// checks the result cache, and runs the closure inside a scoped panic
/// hook and `catch_unwind` (converting a panic into `TestCaseError::Fail`
/// and, under `timeout`, failing a case that ran too long). Tags the
/// success with the kind that decides whether it counts toward `cases`.
#[cfg(feature = "std")]
#[allow(
  clippy::single_call_fn,
  reason = "preserve the legacy callback, cache, panic and timeout boundary behind its execution adapter"
)]
fn call_test<V, F, R>(
  runner: &TestRunner,
  case: V,
  test_fn: &F,
  replay_from_fork: &mut R,
  result_cache: &mut LegacyCache,
  fork_output: &mut ForkOutput,
  is_from_persisted_seed: bool,
) -> TestCaseResultV2
where
  V: fmt::Debug,
  F: Fn(V) -> TestCaseResult,
  R: Iterator<Item = TestCaseResult>,
{
  #[cfg(feature = "timeout")]
  let timeout = runner.config.timeout();

  if let Some(result) = replay_from_fork.next() {
    return result.map(|()| TestCaseOk::ReplayFromForkSuccess);
  }

  // Now that we're about to start a new test (as far as the replay system is
  // concerned), ping the replay file so the parent process can determine
  // that we made it this far.
  #[cfg(feature = "fork")]
  fork_output.ping().map_err(TestCaseError::fail)?;
  #[cfg(not(feature = "fork"))]
  let _: &mut ForkOutput = fork_output;

  verbose_message!(runner, TRACE, "Next test input: {:?}", case);

  let cache_key = result_cache.key(&ResultCacheKey::new(&case));
  if let Some(result) = result_cache.get(cache_key) {
    verbose_message!(runner, TRACE, "Test input hit cache, skipping execution");
    return result.clone().map(|()| TestCaseOk::CacheHitSuccess);
  }

  #[cfg(feature = "timeout")]
  let time_start = Instant::now();

  let test_result = unwrap_or!(
        super::scoped_panic_hook::suppress_panic_hook(|| panic::catch_unwind(
            AssertUnwindSafe(|| test_fn(case))
        )),
        what => Err(TestCaseError::Fail(
            what.downcast::<&'static str>().map(|message| (*message).into())
                .or_else(|what| what.downcast::<String>().map(|message| (*message).into()))
                .or_else(|what| what.downcast::<Box<str>>().map(|message| (*message).into()))
                .unwrap_or_else(|_| "<unknown panic value>".into()))));

  // If there is a timeout and we exceeded it, fail the test here so we get
  // consistent behaviour. (The parent process cannot precisely time the test
  // cases itself.)
  #[cfg(feature = "timeout")]
  let mut final_result = test_result;
  #[cfg(feature = "timeout")]
  if timeout > 0 && final_result.is_ok() {
    let elapsed = time_start.elapsed();
    let elapsed_millis = u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);

    if elapsed_millis > timeout {
      final_result = Err(TestCaseError::fail(format!(
        "Timeout of {timeout} ms exceeded: test took {elapsed_millis} ms"
      )));
    }
  }
  #[cfg(not(feature = "timeout"))]
  let final_result = test_result;

  result_cache.put(cache_key, &final_result);
  #[cfg(feature = "fork")]
  fork_output.append(&final_result).map_err(TestCaseError::fail)?;
  match final_result {
    Ok(()) => verbose_message!(runner, TRACE, "Test case passed"),
    Err(TestCaseError::Reject(ref reason)) => {
      verbose_message!(runner, INFO_LOG, "Test case rejected: {}", reason);
    }
    Err(TestCaseError::Fail(ref reason)) => {
      verbose_message!(runner, INFO_LOG, "Test case failed: {}", reason);
    }
  }

  final_result.map(|()| {
    if is_from_persisted_seed {
      TestCaseOk::PersistedCaseSuccess
    } else {
      TestCaseOk::NewCaseSuccess
    }
  })
}

/// The whole-test outcome for a run over strategy `S`: `Ok(())`, or a
/// `TestError` carrying the reason and the minimized failing value.
type TestRunResult<S> = Result<(), TestError<<S as Strategy>::Value>>;

/// Controller text for the shrink-iteration budget in diagnostics.
#[cfg(feature = "std")]
const SHRINK_ITERS_CONTROLLER: &str = "the PROPTEST_MAX_SHRINK_ITERS environment variable or ProptestConfig.max_shrink_iters";
/// Controller text for the shrink-iteration budget in diagnostics.
#[cfg(not(feature = "std"))]
const SHRINK_ITERS_CONTROLLER: &str = "ProptestConfig.max_shrink_iters";

/// Controller text for the shrink-time budget in diagnostics.
#[cfg(feature = "std")]
const SHRINK_TIME_CONTROLLER: &str = "the PROPTEST_MAX_SHRINK_TIME environment variable or ProptestConfig.max_shrink_time";
/// Controller text for the shrink-time budget in diagnostics.
#[cfg(not(feature = "std"))]
const SHRINK_TIME_CONTROLLER: &str = "(not configurable in no_std)";

impl TestRunner {
  /// Create a fresh `TestRunner` with the given configuration.
  ///
  /// The runner will use an RNG with a generated seed and the default
  /// algorithm.
  ///
  /// In `no_std` environments, every `TestRunner` will use the same
  /// hard-coded seed. This seed is not contractually guaranteed and may be
  /// changed between releases without notice.
  #[must_use]
  pub fn new(config: Config) -> Self {
    let seed = config.rng_seed;
    let algorithm = config.rng_algorithm;
    Self::new_with_rng(config, TestRng::default_rng(seed, algorithm))
  }

  /// Create a fresh `TestRunner` with the standard deterministic RNG.
  ///
  /// This is sugar for the following:
  ///
  /// ```rust
  /// # use proptest::test_runner::*;
  /// let config = Config::default();
  /// let algorithm = config.rng_algorithm;
  /// TestRunner::new_with_rng(config, TestRng::deterministic_rng(algorithm));
  /// ```
  ///
  /// Refer to `TestRng::deterministic_rng()` for more information on the
  /// properties of the RNG used here.
  #[must_use]
  pub fn deterministic() -> Self {
    let config = Config::default();
    let algorithm = config.rng_algorithm;
    Self::new_with_rng(config, TestRng::deterministic_rng(algorithm))
  }

  /// Create a fresh `TestRunner` with the given configuration and RNG.
  #[must_use]
  pub fn new_with_rng(config: Config, rng: TestRng) -> Self {
    Self {
      config,
      successes: 0,
      local_rejects: 0,
      global_rejects: 0,
      rng,
      flat_map_regens: Arc::new(AtomicUsize::new(0)),
      local_reject_detail: BTreeMap::new(),
      global_reject_detail: BTreeMap::new(),
    }
  }

  /// Create a fresh `TestRunner` with the same config and global counters as
  /// this one, but with local state reset and an independent `Rng` (but
  /// deterministic).
  pub(crate) fn partial_clone(&mut self) -> Self {
    Self {
      config:               self.config.clone(),
      successes:            0,
      local_rejects:        0,
      global_rejects:       0,
      rng:                  self.new_rng(),
      flat_map_regens:      Arc::clone(&self.flat_map_regens),
      local_reject_detail:  BTreeMap::new(),
      global_reject_detail: BTreeMap::new(),
    }
  }

  /// Returns the RNG for this test run.
  pub const fn rng(&mut self) -> &mut TestRng {
    &mut self.rng
  }

  /// Create a new, independent but deterministic RNG from the RNG in this
  /// runner.
  #[must_use]
  pub fn new_rng(&mut self) -> TestRng {
    self.rng.gen_rng()
  }

  /// Returns the configuration of this runner.
  #[must_use]
  pub const fn config(&self) -> &Config {
    &self.config
  }

  /// Dumps the bytes obtained from the RNG so far (only works if the RNG is
  /// set to `Recorder`).
  #[must_use]
  pub fn bytes_used(&self) -> Option<Vec<u8>> {
    self.rng.bytes_used()
  }

  /// Run test cases against `f`, choosing inputs via `strategy`.
  ///
  /// If any failure cases occur, try to find a minimal failure case and
  /// report that. If invoking `f` panics, the panic is turned into a
  /// `TestCaseError::Fail`.
  ///
  /// If failure persistence is enabled, all persisted failing cases are
  /// tested first. If a later non-persisted case fails, its seed is
  /// persisted before returning failure.
  ///
  /// Returns success or failure indicating why the test as a whole failed.
  ///
  /// ## Errors
  ///
  /// Returns `TestError::Fail` with the minimized input when a case
  /// fails, or `TestError::Abort` when generation fails or too many
  /// inputs are rejected.
  pub fn run<S: Strategy>(&mut self, strategy: &S, test_fn: impl Fn(S::Value) -> TestCaseResult) -> TestRunResult<S> {
    if self.config.fork() {
      self.run_in_fork(strategy, test_fn)
    } else {
      self.run_in_process(strategy, test_fn)
    }
  }

  /// Run a property while retaining its original successful subjects and failures.
  ///
  /// Generation, persistence, case counting, and shrinking use the same native
  /// algorithm as `run`. Values have no cloning, thread-safety, lifetime, or
  /// serialization bounds. Returned subjects live until the report is dropped.
  ///
  /// # Errors
  /// Returns the minimized native counterexample and matching failure, an
  /// engine abort, or a caught panic with all reached evidence. Fork or timeout
  /// configuration requires `run_typed_with_transport` and fails before the
  /// property is invoked on this entry point.
  pub fn run_typed<S, A, E>(&mut self, strategy: &S, property: impl Fn(S::Value) -> Result<A, E>) -> super::PropertyResult<S::Value, A, E>
  where
    S: Strategy,
  {
    let mut execution = super::typed::TypedExecution {
      property,
      channel: super::transport::InProcess(PhantomData),
      cache: self.new_cache(),
      run: super::PropertyRun::default(),
    };
    let result = if self.config.fork() {
      Err(RunFailure::engine(super::ExecutionError::TransportRequired))
    } else {
      self.run_with_execution(strategy, &mut execution)
    };
    execution.complete(result, self.statistics())
  }

  /// Snapshot the native counters without touching evaluation payloads.
  pub(super) const fn statistics(&self) -> super::RunStatistics {
    super::RunStatistics {
      successes:      self.successes,
      local_rejects:  self.local_rejects,
      global_rejects: self.global_rejects,
    }
  }

  /// Run with an explicit codec for forked execution and typed replay.
  ///
  /// The configuration is honored verbatim. Without fork or timeout this runs
  /// locally and does not encode or decode the returned values.
  ///
  /// # Errors
  /// Returns the complete reached evidence on falsification, engine abort,
  /// interruption, transport failure, or temporary-file finalization failure.
  pub fn run_typed_with_transport<S, A, E, C>(
    &mut self,
    strategy: &S,
    property: impl Fn(S::Value) -> Result<A, E>,
    transport: C,
  ) -> super::PropertyResult<S::Value, A, E, C::Error>
  where
    S: Strategy,
    C: super::PropertyTransport<S::Value, A, E>,
  {
    #[cfg(feature = "fork")]
    if self.config.fork() {
      return super::typed_fork::run(self, strategy, property, transport);
    }
    let _transport = transport;
    self.run_with_channel(strategy, property, super::transport::InProcess::<C::Error>(PhantomData))
  }

  /// Use one typed payload owner with the shared native case walk.
  pub(super) fn run_with_channel<S, A, E, C>(
    &mut self,
    strategy: &S,
    property: impl Fn(S::Value) -> Result<A, E>,
    channel: C,
  ) -> super::PropertyResult<S::Value, A, E, C::Error>
  where
    S: Strategy,
    C: super::transport::EvaluationChannel<S::Value, A, E>,
  {
    let mut execution = super::typed::TypedExecution {
      property,
      channel,
      cache: self.new_cache(),
      run: super::PropertyRun::default(),
    };
    let result = self.run_with_execution(strategy, &mut execution);
    execution.complete(result, self.statistics())
  }

  /// Unreachable stand-in: without the `fork` feature `Config::fork()`
  /// is always false, so `run` never routes here.
  #[cfg(not(feature = "fork"))]
  fn run_in_fork<S: Strategy>(&mut self, _: &S, _: impl Fn(S::Value) -> TestCaseResult) -> TestRunResult<S> {
    let _: &mut Self = self;
    Err(TestError::Abort(Reason::from("fork support is disabled")))
  }

  /// Run the test in a subprocess, coordinating through a shared
  /// forkfile.
  ///
  /// Spawns children until the replay log terminates, synthesizing a
  /// failure for a crash, nonzero/spurious exit, or timeout, then
  /// replays the recorded steps in-process to recover the shrunken
  /// value and update persistence.
  #[cfg(feature = "fork")]
  fn run_in_fork<S: Strategy>(&mut self, strategy: &S, test_fn: impl Fn(S::Value) -> TestCaseResult) -> TestRunResult<S> {
    let mut deferred_test_fn = Some(test_fn);

    let Some(configured_test_name) = self.config.test_name else {
      return Err(TestError::Abort("Must supply test_name when forking enabled".into()));
    };
    let test_name = fork_test::fix_module_path(configured_test_name);
    let (mut replay, mut forkfile, forkfile_path) = create_parent_replay_file(self.rng.new_rng_seed()).map_err(TestError::Abort)?;
    let mut child_count = 0_u32;
    let timeout = self.config.timeout();

    loop {
      let pre_spawn_forkfile_size = forkfile_size(&forkfile).map_err(TestError::Abort)?;
      let (child_error, last_fork_file_len) = rusty_fork::fork(
        test_name,
        rusty_fork_id!(),
        |cmd| {
          let _configured = cmd.env(ENV_FORK_FILE, &forkfile_path);
        },
        |child, _| await_child(child, &forkfile, timeout),
        || self.run_deferred_child(strategy, &mut deferred_test_fn),
      )
      .map_err(|error| TestError::Abort(format!("Fork failed: {error:?}").into()))?;

      match refresh_parent_replay(&mut forkfile, &mut replay).map_err(TestError::Abort)? {
        ParentReplayStatus::InProgress => {}
        ParentReplayStatus::Terminated => {
          break;
        }
      }

      let curr_forkfile_size = forkfile_size(&forkfile).map_err(TestError::Abort)?;

      // If the child failed to append *anything* to the forkfile, it
      // crashed or timed out before starting even one test case, so
      // bail.
      if curr_forkfile_size == pre_spawn_forkfile_size {
        return Err(TestError::Abort(
          "Child process crashed or timed out before the first test started running; giving up.".into(),
        ));
      }

      // The child only terminates early if it outright crashes or we
      // kill it due to timeout, so add a synthetic failure to the
      // output. But only do this if the length of the fork file is the
      // same as when we last saw it, or if the child was not killed due
      // to timeout. (This is because the child could have appended
      // something to the file after we gave up waiting for it but before
      // we were able to kill it).
      if should_record_abrupt_child_failure(last_fork_file_len, curr_forkfile_size) {
        record_abrupt_child_failure(&mut forkfile, &mut replay, child_error).map_err(TestError::Abort)?;
      }

      // Bail if we've gone through too many processes in case the
      // shrinking process itself is crashing.
      child_count = child_count.saturating_add(1);
      if child_count >= 10000 {
        return Err(TestError::Abort("Giving up after 10000 child processes crashed".into()));
      }
    }

    // Run through the steps in-process (without ever running the actual
    // tests) to produce the shrunken value and update the persistence
    // file.
    self.rng.set_seed(replay.seed);
    self.run_in_process_with_replay(
      strategy,
      |_| Err(TestCaseError::fail("Ran past the end of the replay")),
      replay.steps.into_iter(),
      ForkOutput::empty(),
    )
  }

  /// Run the whole test in this process, first loading any fork replay
  /// steps, then delegating to `run_in_process_with_replay`.
  fn run_in_process<S: Strategy>(&mut self, strategy: &S, test_fn: impl Fn(S::Value) -> TestCaseResult) -> TestRunResult<S> {
    #[cfg(feature = "fork")]
    let (replay_steps, fork_output) = init_replay(&mut self.rng).map_err(TestError::Abort)?;
    #[cfg(not(feature = "fork"))]
    let (replay_steps, fork_output) = (iter::empty::<TestCaseResult>(), ForkOutput::empty());
    self.run_in_process_with_replay(strategy, test_fn, replay_steps.into_iter(), fork_output)
  }

  /// Adapt legacy replay and payload storage to the common generation loop.
  fn run_in_process_with_replay<S: Strategy>(
    &mut self,
    strategy: &S,
    test_fn: impl Fn(S::Value) -> TestCaseResult,
    replay_from_fork: impl Iterator<Item = TestCaseResult>,
    fork_output: ForkOutput,
  ) -> TestRunResult<S> {
    let mut execution = LegacyExecution {
      test_fn,
      replay: replay_from_fork,
      cache: LegacyCache {
        cache:   self.new_cache(),
        results: Vec::new(),
      },
      output: fork_output,
    };
    let result = self.run_with_execution(strategy, &mut execution).map_err(legacy_failure);
    #[cfg(feature = "fork")]
    execution.output.terminate().map_err(TestError::Abort)?;
    result
  }

  /// Replay persisted seeds, then generate cases using one execution adapter.
  ///
  /// The adapter owns evaluation payloads. This loop owns generation, counting,
  /// RNG restoration, and regression persistence for both public runner APIs.
  fn run_with_execution<S, X>(&mut self, strategy: &S, execution: &mut X) -> ExecutionResult<(), S::Value, X>
  where
    S: Strategy,
    X: Execution<S::Value>,
  {
    let old_rng = self.rng.clone();
    let persisted_failure_seeds: Vec<PersistedSeed> = self
      .config
      .failure_persistence
      .as_ref()
      .map(|persistence| persistence.load_persisted_failures2(self.config.source_file))
      .unwrap_or_default();
    for PersistedSeed(seed) in persisted_failure_seeds.into_iter().rev() {
      self.rng.set_seed(seed);
      let result = self.gen_and_run_case(strategy, execution, CaseOrigin::Persisted);
      if let Err(failure) = result {
        self.rng = old_rng;
        return Err(failure);
      }
    }
    self.rng = old_rng;

    while self.successes < self.config.cases {
      let seed = self.rng.gen_get_seed();
      let result = self.gen_and_run_case(strategy, execution, CaseOrigin::Generated);
      if let Err(RunFailure::Falsified(_, ref counterexample)) = result
        && let Some(ref mut persistence) = self.config.failure_persistence
        && !execution.is_in_fork()
      {
        persistence.save_persisted_failure2(self.config.source_file, PersistedSeed(seed), counterexample);
      }
      result?;
    }
    Ok(())
  }

  /// Generate and evaluate a case, counting only fresh, successful evaluations.
  fn gen_and_run_case<S, X>(&mut self, strategy: &S, execution: &mut X, origin: CaseOrigin) -> ExecutionResult<(), S::Value, X>
  where
    S: Strategy,
    X: Execution<S::Value>,
  {
    let case = strategy.new_tree(self).map_err(RunFailure::Aborted)?;
    let passed = self.run_case(case, execution, origin)?;
    match passed {
      TestCaseOk::NewCaseSuccess | TestCaseOk::ReplayFromForkSuccess => {
        self.successes = self.successes.saturating_add(1);
      }
      TestCaseOk::PersistedCaseSuccess | TestCaseOk::CacheHitSuccess | TestCaseOk::Reject => (),
    }
    Ok(())
  }

  /// Run one specific case and shrink any failure, ignoring fork configuration.
  ///
  /// A callback that returns after the timeout still fails. This entry point
  /// cannot terminate a callback that does not return.
  ///
  /// # Errors
  /// Returns the minimized failure or a rejection-budget abort.
  pub fn run_one<V: ValueTree>(&mut self, case: V, test_fn: impl Fn(V::Value) -> TestCaseResult) -> Result<bool, TestError<V::Value>> {
    let mut execution = LegacyExecution {
      test_fn,
      replay: iter::empty(),
      cache: LegacyCache {
        cache:   self.new_cache(),
        results: Vec::new(),
      },
      output: ForkOutput::empty(),
    };
    self
      .run_case(case, &mut execution, CaseOrigin::Generated)
      .map(|passed| !matches!(passed, TestCaseOk::Reject))
      .map_err(legacy_failure)
  }

  /// Keep each established failure paired with the exact candidate evaluated.
  fn run_case<V, X>(&mut self, mut case: V, execution: &mut X, origin: CaseOrigin) -> ExecutionResult<TestCaseOk, V::Value, X>
  where
    V: ValueTree,
    X: Execution<V::Value>,
  {
    let mut observed = case.current();
    let verdict = execution
      .evaluate(self, &mut observed, case.current(), origin)
      .map_err(RunFailure::engine)?;
    match verdict {
      CaseVerdict::Passed(passed) => Ok(passed),
      CaseVerdict::Rejected(reason) => {
        self.reject_global(reason).map_err(|error: TestError<V::Value>| match error {
          TestError::Abort(abort_reason) | TestError::Fail(abort_reason, _) => RunFailure::Aborted(abort_reason),
        })?;
        Ok(TestCaseOk::Reject)
      }
      CaseVerdict::Failed(failure) => {
        let (minimized_failure, counterexample) = self.shrink(&mut case, execution, (failure, observed))?;
        Err(RunFailure::Falsified(minimized_failure, counterexample))
      }
    }
  }

  /// Walk the value tree while retaining the last failing pair independently.
  ///
  /// Passing/rejected candidates and budget backtracking never replace that
  /// pair. No final property invocation is needed to recover the failure.
  fn shrink<V, X>(&self, case: &mut V, execution: &mut X, mut last_failure: FailingCase<V::Value, X>) -> ShrinkResult<V::Value, X>
  where
    V: ValueTree,
    X: Execution<V::Value>,
  {
    if self.config.max_shrink_iters == 0 {
      verbose_message!(self, INFO_LOG, "Shrinking disabled by configuration");
      return Ok(last_failure);
    }
    match self.walk_shrinks(case, execution, &mut last_failure) {
      Ok(()) => Ok(last_failure),
      Err(error) => Err(RunFailure::Engine {
        error,
        failing: Some(last_failure),
      }),
    }
  }

  /// Update the established pair only after a failing evaluation while walking the tree.
  #[allow(
    clippy::single_call_fn,
    reason = "separate fallible traversal from ownership of the pair returned on every stopping path"
  )]
  fn walk_shrinks<V, X>(&self, case: &mut V, execution: &mut X, last_failure: &mut FailingCase<V::Value, X>) -> Result<(), X::Error>
  where
    V: ValueTree,
    X: Execution<V::Value>,
  {
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    let start_time = Instant::now();
    let mut iterations = 0_u32;
    if !case.simplify() {
      return Ok(());
    }
    loop {
      #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
      let timed_out = self.shrink_time_exceeded(start_time);
      #[cfg(not(all(feature = "std", not(target_arch = "wasm32"))))]
      let timed_out = None;
      if execution.shrink_budget(self.shrink_budget_exhausted(iterations, timed_out))? {
        break;
      }
      iterations = iterations.saturating_add(1);
      let mut observed = case.current();
      let verdict = execution.evaluate(self, &mut observed, case.current(), CaseOrigin::Shrink)?;
      let walked = match verdict {
        CaseVerdict::Passed(_) | CaseVerdict::Rejected(_) => self.complicate_or_note(case),
        CaseVerdict::Failed(failure) => {
          *last_failure = (failure, observed);
          self.simplify_or_note(case)
        }
      };
      if !walked {
        return Ok(());
      }
    }
    while case.complicate() {
      execution.backtrack()?;
    }
    Ok(())
  }

  /// How many milliseconds the shrink phase has been running past the
  /// `max_shrink_time` budget, or `None` while still inside it (or when
  /// the budget is unlimited).
  #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
  fn shrink_time_exceeded(&self, start_time: Instant) -> Option<u64> {
    if self.config.max_shrink_time == 0 {
      return None;
    }
    let elapsed = start_time.elapsed();
    let elapsed_ms = elapsed
      .as_secs()
      .saturating_mul(1000)
      .saturating_add(u64::from(elapsed.subsec_millis()));
    (elapsed_ms > u64::from(self.config.max_shrink_time)).then_some(elapsed_ms)
  }

  /// Whether the shrink walk must stop early because a budget is spent —
  /// the iteration cap (`max_shrink_iters`) or the wall clock
  /// (`max_shrink_time`) — emitting the ALWAYS-level diagnostic that
  /// names the knob to raise.
  fn shrink_budget_exhausted(&self, iterations: u32, timed_out: Option<u64>) -> bool {
    if iterations >= self.config.max_shrink_iters() {
      verbose_message!(
        self,
        ALWAYS,
        "Aborting shrinking after {} iterations (set {} to a large(r) value to shrink more; current configuration: {} iterations)",
        SHRINK_ITERS_CONTROLLER,
        self.config.max_shrink_iters(),
        iterations
      );
      return true;
    }

    let Some(ms) = timed_out else {
      return false;
    };
    #[cfg(feature = "std")]
    let current = self.config.max_shrink_time;
    #[cfg(not(feature = "std"))]
    let current = 0;
    verbose_message!(
      self,
      ALWAYS,
      "Aborting shrinking after taking too long: {} ms (set {} to a large(r) value to shrink more; current configuration: {} ms)",
      ms,
      SHRINK_TIME_CONTROLLER,
      current
    );
    true
  }

  /// Run the deferred user closure in the first fork child, leaving later
  /// children to replay from the shared forkfile.
  #[cfg(feature = "fork")]
  #[allow(
    clippy::single_call_fn,
    reason = "the fork child may consume the user closure exactly once while later child spawns only replay"
  )]
  fn run_deferred_child<S, F>(&mut self, strategy: &S, deferred_test_fn: &mut Option<F>)
  where
    S: Strategy,
    F: Fn(S::Value) -> TestCaseResult,
  {
    let Some(child_test_fn) = deferred_test_fn.take() else {
      return;
    };
    let _result = self.run_in_process(strategy, child_test_fn);
  }

  /// Backtrack the shrink walk toward the most recent failing value,
  /// noting at TRACE level when the tree has no further complications.
  fn complicate_or_note<V: ValueTree>(&self, case: &mut V) -> bool {
    if case.complicate() {
      return true;
    }
    verbose_message!(self, TRACE, "Cannot complicate further");
    false
  }

  /// Step the shrink walk toward a simpler value, noting at TRACE level
  /// when the tree has no further simplifications.
  fn simplify_or_note<V: ValueTree>(&self, case: &mut V) -> bool {
    if case.simplify() {
      return true;
    }
    verbose_message!(self, TRACE, "Cannot simplify further");
    false
  }

  /// Update the state to account for a local rejection from `whence`, and
  /// return `Ok` if the caller should keep going or `Err` to abort.
  ///
  /// ## Errors
  ///
  /// Returns `Err` with an explanatory `Reason` once more than
  /// `max_local_rejects` inputs have been rejected locally.
  pub fn reject_local(&mut self, whence: impl Into<Reason>) -> Result<(), Reason> {
    if self.local_rejects >= self.config.max_local_rejects {
      Err("Too many local rejects".into())
    } else {
      self.local_rejects = self.local_rejects.saturating_add(1);
      Self::insert_or_increment(&mut self.local_reject_detail, whence.into());
      Ok(())
    }
  }

  /// Update the state to account for a global rejection from `whence`, and
  /// return `Ok` if the caller should keep going or `Err` to abort.
  fn reject_global<T>(&mut self, whence: Reason) -> Result<(), TestError<T>> {
    if self.global_rejects >= self.config.max_global_rejects {
      Err(TestError::Abort("Too many global rejects".into()))
    } else {
      self.global_rejects = self.global_rejects.saturating_add(1);
      Self::insert_or_increment(&mut self.global_reject_detail, whence);
      Ok(())
    }
  }

  /// Insert 1 or increment the rejection detail at key for whence.
  fn insert_or_increment(into: &mut RejectionDetail, whence: Reason) {
    let count = into.entry(whence).or_insert(0);
    *count = count.saturating_add(1);
  }

  /// Increment the counter of flat map regenerations and return whether it
  /// is still under the configured limit.
  #[must_use]
  pub fn flat_map_regen(&self) -> bool {
    u32::try_from(self.flat_map_regens.fetch_add(1, SeqCst)).is_ok_and(|regens| regens < self.config.max_flat_map_regens)
  }

  /// Build a fresh result cache from the configured factory.
  fn new_cache(&self) -> Box<dyn ResultCache> {
    (self.config.result_cache)()
  }
}

/// Detect the fork-child role from the `_PROPTEST_FORKFILE` env var.
///
/// A child seeds its RNG from the replay file and returns the recorded
/// steps plus a `ForkOutput` that appends to it; a parent (no env var)
/// returns no steps and an empty `ForkOutput`.
#[cfg(feature = "fork")]
#[allow(
  clippy::single_call_fn,
  reason = "detect the fork-child role from _PROPTEST_FORKFILE and seed the RNG from its replay log"
)]
fn init_replay(rng: &mut TestRng) -> Result<(Vec<TestCaseResult>, ForkOutput), Reason> {
  use crate::test_runner::replay::Replay;
  use crate::test_runner::replay::ReplayFileStatus;
  use crate::test_runner::replay::open_file;

  env::var_os(ENV_FORK_FILE).map_or_else(
    || Ok((vec![], ForkOutput::empty())),
    |path| {
      let mut file = open_file(&path).map_err(|error| Reason::from(format!("Failed to open replay file: {error}")))?;
      let loaded = Replay::parse_from(&mut file).map_err(|error| Reason::from(format!("Failed to read replay file: {error}")))?;
      match loaded {
        ReplayFileStatus::InProgress(replay) => {
          rng.set_seed(replay.seed);
          Ok((replay.steps, ForkOutput {
            file: Some(file)
          }))
        }

        ReplayFileStatus::Terminated(_) => Err("Replay file for child process is terminated".into()),

        ReplayFileStatus::Corrupt => Err("Replay file for child process is corrupt".into()),
      }
    },
  )
}

/// Wait for a fork child to exit, mapping a nonzero exit status into a
/// synthetic case failure.
#[cfg(feature = "fork")]
#[allow(
  clippy::single_call_fn,
  reason = "wait for a fork child unconditionally, turning a nonzero exit into a synthetic failure"
)]
fn await_child_without_timeout(child: &mut rusty_fork::ChildWrapper) -> (Option<TestCaseError>, Option<u64>) {
  let status = match child.wait() {
    Ok(status) => status,
    Err(error) => {
      return (
        Some(TestCaseError::fail(format!("Failed to wait for child process: {error}"))),
        None,
      );
    }
  };

  if status.success() {
    (None, None)
  } else {
    (Some(TestCaseError::fail(format!("Child process exited with {status}"))), None)
  }
}

/// Wait for a fork child to exit; with the `timeout` feature off this
/// just defers to `await_child_without_timeout`.
#[cfg(all(feature = "fork", not(feature = "timeout")))]
fn await_child(child: &mut rusty_fork::ChildWrapper, _: &tempfile::NamedTempFile, _timeout: u32) -> (Option<TestCaseError>, Option<u64>) {
  await_child_without_timeout(child)
}

/// Wait for a fork child to exit, killing it if the forkfile stops
/// growing for a full timeout window.
///
/// A zero `timeout` defers to `await_child_without_timeout`. Otherwise
/// the child may outlive one timeout as long as the forkfile keeps
/// growing between checks; returning the last observed length lets the
/// caller tell a real timeout from a late append.
#[cfg(all(feature = "fork", feature = "timeout"))]
#[allow(
  clippy::single_call_fn,
  reason = "wait for a fork child, killing it once the forkfile stalls for a full timeout window"
)]
fn await_child(
  child: &mut rusty_fork::ChildWrapper,
  forkfile: &tempfile::NamedTempFile,
  timeout: u32,
) -> (Option<TestCaseError>, Option<u64>) {
  use std::time::Duration;

  if 0 == timeout {
    return await_child_without_timeout(child);
  }

  // The child can run for longer than the timeout since it may run
  // multiple tests. Each time the timeout expires, we check whether the
  // file has grown larger. If it has, we allow the child to keep running
  // until the next timeout.
  let mut last_forkfile_len = match forkfile.as_file().metadata() {
    Ok(metadata) => metadata.len(),
    Err(error) => {
      return (
        Some(TestCaseError::fail(format!("Failed to read fork file metadata: {error}"))),
        None,
      );
    }
  };

  loop {
    let wait_status = match child.wait_timeout(Duration::from_millis(timeout.into())) {
      Ok(status) => status,
      Err(error) => {
        return (
          Some(TestCaseError::fail(format!("Failed to wait for child process: {error}"))),
          Some(last_forkfile_len),
        );
      }
    };

    if let Some(status) = wait_status {
      if status.success() {
        return (None, None);
      }
      return (Some(TestCaseError::fail(format!("Child process exited with {status}"))), None);
    }

    let current_len = match forkfile.as_file().metadata() {
      Ok(metadata) => metadata.len(),
      Err(error) => {
        return (
          Some(TestCaseError::fail(format!("Failed to read fork file metadata: {error}"))),
          Some(last_forkfile_len),
        );
      }
    };
    // If we've gone a full timeout period without the file growing,
    // fail the test and kill the child.
    if current_len <= last_forkfile_len {
      return (Some(TestCaseError::fail("Timed out waiting for child process")), Some(current_len));
    }
    last_forkfile_len = current_len;
  }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod test {
  use std::cell::Cell;
  use std::fs;
  use std::io::ErrorKind;
  #[cfg(feature = "timeout")]
  use std::thread;
  #[cfg(feature = "timeout")]
  use std::time::Duration;

  use strict_test_support::CapturedBinary;
  use strict_test_support::ComparisonFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::TestFailure;
  use strict_test_support::capture_ignored_test;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_ne;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::test_runner::FileFailurePersistence;
  use crate::test_runner::RngAlgorithm;
  use crate::test_runner::RngSeed;
  use crate::test_runner::TestRng;
  use crate::test_runner::result_cache::basic_result_cache;

  const PERSISTED_COUNTING_CHILD: &str = "test_runner::runner::test::persisted_cases_do_not_count_towards_total_cases_child";
  const PERSISTED_RELOAD_CHILD: &str = "test_runner::runner::test::failing_cases_persisted_and_reloaded_child";

  /// Complete native assertion subjects stay concrete and allocated on failure.
  type Check<S> = Result<(), Box<PredicateFailure<S>>>;
  /// A real ignored-test subprocess with its preparation and execution evidence.
  type Capture = Result<CapturedBinary, TestFailure>;
  /// Randomness observed before and after creating an independent generator.
  type RngBytes = [u8; 16];

  /// Complete legacy runner outcome for a scalar input.
  type ScalarRun = Result<(), TestError<u32>>;

  /// The configured fork runner and its complete scalar outcome.
  #[cfg(feature = "fork")]
  type ForkRun = (TestRunner, ScalarRun);

  #[test]
  fn gives_up_after_too_many_rejections() -> Check<(TestRunner, u32, ScalarRun)> {
    let mut runner = TestRunner::new(runner_test_config());
    let runs = Cell::new(0_u32);
    let result = runner.run(&(0_u32..), |_| {
      runs.set(runs.get().saturating_add(1));
      Err(TestCaseError::reject("reject"))
    });
    ensure_that(
      (runner, runs.get(), result),
      "global rejection stops after the budget plus the aborting case",
      |observed| matches!(observed.2, Err(TestError::Abort(_))) && observed.1 == observed.0.config.max_global_rejects.saturating_add(1),
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn test_pass() -> Result<(), ComparisonFailure<ScalarRun, ScalarRun>> {
    let mut runner = TestRunner::new(runner_test_config());
    let result = runner.run(&(1_u32..), |candidate| {
      if candidate > 0 {
        Ok(())
      } else {
        Err(TestCaseError::fail("generated value must be positive"))
      }
    });
    ensure_eq(result, Ok(()), "a passing property returns Ok").map(drop)
  }

  #[test]
  fn test_fail_via_result() -> Result<(), ComparisonFailure<ScalarRun, ScalarRun>> {
    let mut runner = TestRunner::new(runner_test_config());
    let result = runner.run(&(0_u32..10), |candidate| {
      if candidate < 5 {
        Ok(())
      } else {
        Err(TestCaseError::fail("not less than 5"))
      }
    });
    ensure_eq(
      result,
      Err(TestError::Fail("not less than 5".into(), 5)),
      "a result failure shrinks to the boundary value",
    )
    .map(drop)
  }

  #[test]
  fn test_fail_via_panic() -> Result<(), ComparisonFailure<ScalarRun, ScalarRun>> {
    let mut runner = TestRunner::new(runner_test_config());
    let result = runner.run(&(0_u32..10), |candidate| {
      // Unwinding is the behavior under test at the legacy callback boundary.
      if candidate >= 5 {
        panic::resume_unwind(Box::new("not less than 5"));
      }
      Ok(())
    });
    ensure_eq(
      result,
      Err(TestError::Fail("not less than 5".into(), 5)),
      "an unwinding case is caught and minimized",
    )
    .map(drop)
  }

  /// Deletes its persistence file on drop, so a test leaves no residue on
  /// the green path and on `Err` returns alike. Each persistence test owns
  /// a distinct file: the tests run on parallel threads, and sharing one
  /// path lets one test's persisted seeds inflate the other's replay count.
  struct PersistenceFileGuard(&'static str);

  impl Drop for PersistenceFileGuard {
    fn drop(&mut self) {
      drop(fs::remove_file(self.0));
    }
  }

  #[test]
  fn persisted_cases_do_not_count_towards_total_cases() -> Check<Capture> {
    check_persistence_child(
      PERSISTED_COUNTING_CHILD,
      b"proptest: Saving this and future failures in persistence-test-counting.txt",
    )
    .map(drop)
  }

  /// Retain the child execution while checking its persistence diagnostic.
  fn check_persistence_child(child: &str, diagnostic: &[u8]) -> Result<Capture, Box<PredicateFailure<Capture>>> {
    ensure_that(
      capture_ignored_test(child),
      "the persistence child passes and emits its save diagnostic",
      |capture| {
        capture.as_ref().is_ok_and(|captured| {
          captured.output.status.success()
            && captured
              .output
              .stderr
              .split(|&byte| byte == b'\n')
              .any(|line| line.starts_with(diagnostic))
        })
      },
    )
    .map_err(Box::new)
  }

  #[test]
  #[ignore = "captured by persisted_cases_do_not_count_towards_total_cases"]
  fn persisted_cases_do_not_count_towards_total_cases_child() -> Result<(), impl fmt::Debug> {
    const FILE: &str = "persistence-test-counting.txt";
    let _guard = PersistenceFileGuard(FILE);
    let removed = fs::remove_file(FILE);
    let config = Config {
      failure_persistence: Some(Box::new(FileFailurePersistence::Direct(FILE))),
      cases: 1,
      ..runner_test_config()
    };
    let seeded = TestRunner::new(config.clone()).run(&(0_i32..10_000_000), |_| Err(TestCaseError::Fail("persist a failure".into())));
    let run_count = Cell::new(0_u32);
    let replayed = TestRunner::new(config).run(&(0_i32..10_000_000), |_| {
      run_count.set(run_count.get().saturating_add(1));
      Ok(())
    });
    ensure_that(
      (removed, seeded, replayed, run_count.get()),
      "persisted cases run before, and do not consume, the fresh case budget",
      |observed| {
        (observed.0.is_ok() || observed.0.as_ref().is_err_and(|error| error.kind() == ErrorKind::NotFound))
          && observed.1.is_err()
          && observed.2.is_ok()
          && observed.3 == 2
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[derive(Clone, Copy, PartialEq)]
  struct PoorlyBehavedDebug(i32);
  impl fmt::Debug for PoorlyBehavedDebug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "\r\n{:?}\r\n", self.0)
    }
  }

  #[test]
  fn failing_cases_persisted_and_reloaded() -> Check<Capture> {
    check_persistence_child(
      PERSISTED_RELOAD_CHILD,
      b"proptest: Saving this and future failures in persistence-test-reload.txt",
    )
    .map(drop)
  }

  #[test]
  #[ignore = "captured by failing_cases_persisted_and_reloaded"]
  fn failing_cases_persisted_and_reloaded_child() -> Result<(), impl fmt::Debug> {
    const FILE: &str = "persistence-test-reload.txt";
    let _guard = PersistenceFileGuard(FILE);
    let removed = fs::remove_file(FILE);
    let max = 10_000_000_i32;
    let input = (0_i32..max).prop_map(PoorlyBehavedDebug);
    let config = Config {
      failure_persistence: Some(Box::new(FileFailurePersistence::Direct(FILE))),
      ..runner_test_config()
    };
    let midpoint = max.div_euclid(2);
    let below_midpoint = |candidate: PoorlyBehavedDebug| {
      if candidate.0 < midpoint {
        Ok(())
      } else {
        Err(TestCaseError::Fail("too big".into()))
      }
    };
    let above_midpoint = |candidate: PoorlyBehavedDebug| {
      if candidate.0 >= midpoint {
        Ok(())
      } else {
        Err(TestCaseError::Fail("too small".into()))
      }
    };
    let runs = [
      TestRunner::new(config.clone()).run(&input, below_midpoint),
      TestRunner::new(config.clone()).run(&input, above_midpoint),
      TestRunner::new(config.clone()).run(&input, below_midpoint),
      TestRunner::new(config).run(&input, above_midpoint),
    ];
    ensure_that(
      (removed, runs),
      "both persisted failures replay identically despite debug newlines",
      |observed| {
        let [ref first_sub, ref first_super, ref second_sub, ref second_super] = observed.1;
        (observed.0.is_ok() || observed.0.as_ref().is_err_and(|error| error.kind() == ErrorKind::NotFound))
          && observed.1.iter().all(Result::is_err)
          && first_sub == second_sub
          && first_super == second_super
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn new_rng_makes_separate_rng() -> Result<(), ComparisonFailure<RngBytes, RngBytes>> {
    use rand::RngExt as _;
    let mut runner = TestRunner::new(runner_test_config());
    let from_1 = runner.new_rng().random::<[u8; 16]>();
    let from_2 = runner.rng().random::<[u8; 16]>();
    ensure_ne(from_1, from_2, "a new rng draws a different stream").map(drop)
  }

  #[test]
  fn record_rng_use() -> Result<(), impl fmt::Debug> {
    use rand::RngExt as _;
    let config = runner_test_config();
    let recorder_rng = TestRng::default_rng(RngSeed::Random, RngAlgorithm::Recorder);
    let mut recorder = TestRunner::new_with_rng(config.clone(), recorder_rng);
    let original = recorder.rng().random::<[u8; 16]>();
    let bytes = recorder.bytes_used();
    let replayed = bytes.as_ref().map(|recorded| {
      let mut runner = TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::PassThrough, recorded));
      let value = runner.rng().random::<[u8; 16]>();
      (runner, value)
    });
    ensure_that(
      (recorder, original, bytes, replayed),
      "recorded bytes recreate the same generated value",
      |observed| {
        observed.2.as_ref().is_some_and(|recorded| recorded.len() >= 16)
          && observed.3.as_ref().is_some_and(|reached| reached.1 == observed.1)
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[cfg(feature = "fork")]
  #[test]
  fn run_successful_test_in_fork() -> Check<ScalarRun> {
    let mut runner = TestRunner::new(Config {
      fork: true,
      test_name: Some(concat!(module_path!(), "::run_successful_test_in_fork")),
      ..runner_test_config()
    });

    ensure_that(
      runner.run(&(0_u32..1000), |_| Ok(())),
      "a passing forked run returns Ok",
      Result::is_ok,
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[cfg(feature = "fork")]
  #[test]
  fn normal_failure_in_fork_results_in_correct_failure() -> Check<ForkRun> {
    check_fork_failure(
      fork_config(concat!(module_path!(), "::normal_failure_in_fork_results_in_correct_failure")),
      |candidate| fails_at_boundary(candidate, "value reached 500"),
    )
    .map(drop)
  }

  /// Enable process isolation for the exact test entry point.
  #[cfg(feature = "fork")]
  fn fork_config(test_name: &'static str) -> Config {
    Config {
      fork: true,
      test_name: Some(test_name),
      ..runner_test_config()
    }
  }

  /// A returned callback failure separates the passing and failing halves of the range.
  #[cfg(feature = "fork")]
  fn fails_at_boundary(candidate: u32, reason: &'static str) -> TestCaseResult {
    if candidate < 500 {
      Ok(())
    } else {
      Err(TestCaseError::fail(reason))
    }
  }

  /// Keep the runner and native outcome while checking cross-process shrinking.
  #[cfg(feature = "fork")]
  fn check_fork_failure(config: Config, property: impl Fn(u32) -> TestCaseResult) -> Result<ForkRun, Box<PredicateFailure<ForkRun>>> {
    let mut runner = TestRunner::new(config);
    let result = runner.run(&(0_u32..1000), property);
    ensure_that(
      (runner, result),
      "the forked failure minimizes to 500 without aborting",
      |observed| matches!(observed.1, Err(TestError::Fail(_, 500))),
    )
    .map_err(Box::new)
  }

  // Fork-surface test: the child reports a failure and the parent shrinks
  // it through the fork boundary.
  #[cfg(feature = "fork")]
  #[test]
  fn nonsuccessful_exit_finds_correct_failure() -> Check<ForkRun> {
    check_fork_failure(
      fork_config(concat!(module_path!(), "::nonsuccessful_exit_finds_correct_failure")),
      |candidate| fails_at_boundary(candidate, "child reported a failure"),
    )
    .map(drop)
  }

  // Fork-surface test: the child reports a failure after earlier cases
  // pass, so the parent still has to shrink across process isolation.
  #[cfg(feature = "fork")]
  #[test]
  fn spurious_exit_finds_correct_failure() -> Check<ForkRun> {
    check_fork_failure(
      fork_config(concat!(module_path!(), "::spurious_exit_finds_correct_failure")),
      |candidate| fails_at_boundary(candidate, "child reported a late failure"),
    )
    .map(drop)
  }

  #[cfg(feature = "timeout")]
  #[test]
  fn long_sleep_timeout_finds_correct_failure() -> Check<ForkRun> {
    check_fork_failure(
      Config {
        timeout: 500,
        ..fork_config(concat!(module_path!(), "::long_sleep_timeout_finds_correct_failure"))
      },
      |candidate| {
        if candidate >= 500 {
          thread::sleep(Duration::from_secs(10));
        }
        Ok(())
      },
    )
    .map(drop)
  }

  #[cfg(feature = "timeout")]
  #[test]
  fn mid_sleep_timeout_finds_correct_failure() -> Check<ForkRun> {
    check_fork_failure(
      Config {
        timeout: 500,
        ..fork_config(concat!(module_path!(), "::mid_sleep_timeout_finds_correct_failure"))
      },
      |candidate| {
        if candidate >= 500 {
          // Sleep a little longer than the timeout. This means that
          // sometimes the test case itself will return before the parent
          // process has noticed the child is timing out, so it's up to
          // the child to mark it as a failure.
          thread::sleep(Duration::from_millis(600));
        } else {
          // Sleep a bit so that the parent and child timing don't stay
          // in sync.
          thread::sleep(Duration::from_millis(100));
        }
        Ok(())
      },
    )
    .map(drop)
  }

  #[test]
  fn duplicate_tests_not_run_with_basic_result_cache() -> Result<(), impl fmt::Debug> {
    use std::collections::HashSet;
    let fails_above_five = |candidate| {
      if candidate <= 5 {
        Ok(())
      } else {
        Err(TestCaseError::fail("value above 5"))
      }
    };
    let runs: Vec<_> = (0..256)
      .map(|_| {
        let mut runner = TestRunner::new(Config {
          result_cache: basic_result_cache,
          ..runner_test_config()
        });
        let calls = Cell::new(Vec::new());
        let result = runner.run(&(0_u32..65536).prop_map(|raw| raw.rem_euclid(10)), |candidate| {
          let mut observed = calls.take();
          observed.push(candidate);
          calls.set(observed);
          fails_above_five(candidate)
        });
        (runner, calls.into_inner(), result)
      })
      .collect();
    ensure_that(
      runs,
      "cached inputs execute once and the failure still minimizes to six",
      |observed| {
        observed.iter().all(|reached| {
          !reached.1.is_empty()
            && reached.1.iter().copied().collect::<HashSet<_>>().len() == reached.1.len()
            && matches!(reached.2, Err(TestError::Fail(_, 6)))
        })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}

/// Legacy timeout regressions retain native assertions under the original outer watchdog.
#[cfg(test)]
#[cfg(all(feature = "fork", feature = "timeout"))]
mod timeout_tests {
  use std::boxed::Box;
  use std::env;
  use std::fmt;
  use std::io;
  use std::process::Child;
  use std::process::Command;
  use std::process::ExitStatus;
  use std::thread;
  use std::time::Duration;
  use std::time::Instant;

  use rusty_fork::rusty_fork_test_name;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::Config;
  use super::TestCaseError;
  use super::TestError;
  use super::TestRunner;
  use super::runner_test_config;
  use crate::num::u64 as num_u64;
  use crate::strategy::Just;
  use crate::strategy::Strategy as _;

  /// Test identity inherited by the watchdog child and any nested runner forks.
  const WATCHDOG_CHILD: &str = "_PROPTEST_TEST_WATCHDOG";
  /// The process, its timed wait, and any required kill and reap results.
  type WatchdogObservation = (
    Child,
    io::Result<Option<ExitStatus>>,
    Option<(io::Result<()>, io::Result<ExitStatus>)>,
  );
  /// The runner and its complete legacy shrinking outcome.
  type ShrinkRun = (TestRunner, Result<(), TestError<u64>>);

  /// Each process returns the evidence it owns at the ordinary test boundary.
  enum WatchdogEvidence<A> {
    /// The child completed its assertion and retained its native evidence.
    Child(A),
    /// The parent supervised the complete child process lifetime.
    Parent(WatchdogObservation),
  }

  impl<A: fmt::Debug> fmt::Debug for WatchdogEvidence<A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      match *self {
        Self::Child(ref evidence) => formatter.debug_tuple("Child").field(evidence).finish(),
        Self::Parent(ref observation) => formatter.debug_tuple("Parent").field(observation).finish(),
      }
    }
  }

  /// Setup, native child assertions, and process supervision remain distinct.
  #[derive(Debug, thiserror::Error)]
  enum WatchdogFailure<E> {
    /// Resolve or launch the exact current test executable.
    #[error("watchdog setup failed: {0}")]
    Setup(io::Error),
    /// Return the original child assertion through the libtest boundary.
    #[error("child assertion failed: {0:?}")]
    Child(E),
    /// Preserve the process and every reached wait and cleanup observation.
    #[error("watchdog observed an unsuccessful child: {0:?}")]
    Parent(Box<PredicateFailure<WatchdogObservation>>),
  }

  /// Keep the four-second watchdog while the child returns through libtest.
  fn within_watchdog<A, E: fmt::Debug>(name: &str, body: impl FnOnce() -> Result<A, E>) -> Result<WatchdogEvidence<A>, WatchdogFailure<E>> {
    if env::var_os(WATCHDOG_CHILD).is_some_and(|selected| selected == name) {
      return body().map(WatchdogEvidence::Child).map_err(WatchdogFailure::Child);
    }
    let executable = env::current_exe().map_err(WatchdogFailure::Setup)?;
    let mut child = Command::new(executable)
      .args(["--exact", name, "--nocapture"])
      .env(WATCHDOG_CHILD, name)
      .spawn()
      .map_err(WatchdogFailure::Setup)?;
    let started = Instant::now();
    let waited = loop {
      match child.try_wait() {
        Ok(None) if started.elapsed() < Duration::from_secs(4) => thread::sleep(Duration::from_millis(10)),
        completion => break completion,
      }
    };
    // The watchdog owns termination and reaping when the timed wait did not finish.
    let cleanup = if matches!(waited, Ok(Some(_))) {
      None
    } else {
      Some((child.kill(), child.wait()))
    };
    ensure_that(
      (child, waited, cleanup),
      "the child assertion succeeds within the four-second watchdog",
      |observed| {
        observed
          .1
          .as_ref()
          .is_ok_and(|status| status.is_some_and(|exited| exited.success()))
          && observed.2.is_none()
      },
    )
    .map(WatchdogEvidence::Parent)
    .map_err(|failure| WatchdogFailure::Parent(Box::new(failure)))
  }

  #[test]
  fn max_shrink_iters_works() -> Result<(), impl fmt::Debug> {
    within_watchdog(rusty_fork_test_name!(max_shrink_iters_works), || {
      run_shrink_bail(Config {
        max_shrink_iters: 5,
        ..runner_test_config()
      })
    })
    .map(drop)
  }

  #[test]
  fn max_shrink_time_works() -> Result<(), impl fmt::Debug> {
    within_watchdog(rusty_fork_test_name!(max_shrink_time_works), || {
      run_shrink_bail(Config {
        max_shrink_time: 1000,
        ..runner_test_config()
      })
    })
    .map(drop)
  }

  #[test]
  fn max_shrink_iters_works_with_forking() -> Result<(), impl fmt::Debug> {
    within_watchdog(rusty_fork_test_name!(max_shrink_iters_works_with_forking), || {
      run_shrink_bail(Config {
        fork: true,
        test_name: Some(concat!(module_path!(), "::max_shrink_iters_works_with_forking")),
        max_shrink_time: 1000,
        ..runner_test_config()
      })
    })
    .map(drop)
  }

  #[test]
  fn detects_child_failure_to_start() -> Result<(), impl fmt::Debug> {
    within_watchdog(rusty_fork_test_name!(detects_child_failure_to_start), || {
      let mut runner = TestRunner::new(Config {
        timeout: 100,
        test_name: Some(concat!(module_path!(), "::detects_child_failure_to_start")),
        ..runner_test_config()
      });
      let result = runner.run(&Just(()).prop_map(|()| thread::sleep(Duration::from_millis(200))), Ok);
      ensure_that(
        (runner, result),
        "a child that fails to start is reported as an abort",
        |observed| matches!(observed.1, Err(TestError::Abort(_))),
      )
      .map_err(Box::new)
    })
    .map(drop)
  }

  /// Exhaust the configured shrink budget without losing the established failing case.
  fn run_shrink_bail(config: Config) -> Result<ShrinkRun, Box<PredicateFailure<ShrinkRun>>> {
    let mut runner = TestRunner::new(config);
    let result = runner.run(&num_u64::ANY, |candidate| {
      thread::sleep(Duration::from_millis(250));
      if u32::try_from(candidate).is_ok() {
        Ok(())
      } else {
        Err(TestCaseError::fail("value exceeds u32::MAX"))
      }
    });
    ensure_that(
      (runner, result),
      "the final value remains a failing case when shrinking stops",
      |observed| matches!(observed.1, Err(TestError::Fail(_, failing_value)) if failing_value > u64::from(u32::MAX)),
    )
    .map_err(Box::new)
  }
}
