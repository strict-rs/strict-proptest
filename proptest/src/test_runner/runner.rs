//-
// Copyright 2017, 2018, 2019, 2024 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "std")]
use crate::std_facade::String;
use crate::std_facade::{Arc, BTreeMap, Box, Vec};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::SeqCst;
use core::{fmt, iter};
#[cfg(feature = "std")]
use std::panic::{self, AssertUnwindSafe};
#[cfg(any(
    feature = "timeout",
    all(feature = "std", not(target_arch = "wasm32"))
))]
use std::time::Instant;

#[cfg(feature = "fork")]
use crate::std_facade::{format, vec};
#[cfg(feature = "fork")]
use rusty_fork;
#[cfg(feature = "fork")]
use rusty_fork::fork_test;
#[cfg(feature = "fork")]
use rusty_fork::rusty_fork_id;
#[cfg(feature = "fork")]
use std::env;
#[cfg(feature = "fork")]
use std::fs;
#[cfg(feature = "fork")]
use std::path::PathBuf;
#[cfg(feature = "fork")]
use tempfile;

use crate::strategy::{Strategy, ValueTree};
use crate::test_runner::config::Config;
use crate::test_runner::errors::{
    TestCaseError, TestCaseOk, TestCaseResult, TestCaseResultV2, TestError,
};
use crate::test_runner::failure_persistence::PersistedSeed;
use crate::test_runner::reason::Reason;
#[cfg(feature = "fork")]
use crate::test_runner::replay;
use crate::test_runner::result_cache::{ResultCache, ResultCacheKey};
use crate::test_runner::rng::{Seed, TestRng};

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
        let _ = &$runner;
        let _ = $level;
        let _ = format_args!($fmt $($arg)*);
    }};
}

/// Per-`Reason` tally of how many inputs were rejected at each site.
type RejectionDetail = BTreeMap<Reason, u32>;

/// State used when running a proptest test.
#[derive(Clone)]
pub struct TestRunner {
    /// The configuration governing this run.
    config: Config,
    /// Count of genuinely new cases that have passed so far.
    successes: u32,
    /// Count of inputs rejected locally (within a single case).
    local_rejects: u32,
    /// Count of inputs rejected globally (across the whole run).
    global_rejects: u32,
    /// The runner's random number generator.
    rng: TestRng,
    /// Shared counter capping total `Flatten` regenerations.
    flat_map_regens: Arc<AtomicUsize>,

    /// Per-site tally of local rejections, for reporting.
    local_reject_detail: RejectionDetail,
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
        write!(
            f,
            "\tsuccesses: {}\n\
             \tlocal rejects: {}\n",
            self.successes, self.local_rejects
        )?;
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
            replay::append(file, result).map_err(|error| {
                Reason::from(format!(
                    "Failed to append to replay file: {error}"
                ))
            })?;
        }
        Ok(())
    }

    /// Append an "I'm alive" mark so the parent sees progress.
    fn ping(&mut self) -> Result<(), Reason> {
        if let Some(ref mut file) = self.file {
            replay::ping(file).map_err(|error| {
                Reason::from(format!("Failed to ping replay file: {error}"))
            })?;
        }
        Ok(())
    }

    /// Append the termination mark that ends the replay log.
    fn terminate(&mut self) -> Result<(), Reason> {
        if let Some(ref mut file) = self.file {
            replay::terminate(file).map_err(|error| {
                Reason::from(format!(
                    "Failed to terminate replay file: {error}"
                ))
            })?;
        }
        Ok(())
    }

    /// A `ForkOutput` that writes nowhere (not in a fork).
    const fn empty() -> Self {
        Self { file: None }
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
    /// No-op: there is no replay file without forking.
    const fn append(&mut self, _result: &TestCaseResult) -> Result<(), Reason> {
        Ok(())
    }

    /// No-op progress ping; present only for signature parity.
    #[cfg(feature = "std")]
    const fn ping(&mut self) -> Result<(), Reason> {
        Ok(())
    }

    /// No-op: there is no replay log to terminate.
    const fn terminate(&mut self) -> Result<(), Reason> {
        Ok(())
    }
    /// The only `ForkOutput` there is without forking.
    fn empty() -> Self {
        ForkOutput
    }
    /// Always `false`: a non-fork build is never inside a fork.
    fn is_in_fork(&self) -> bool {
        false
    }
}

/// Backtrack a budget-exhausted shrink walk to the latest known failing case.
#[allow(
    clippy::single_call_fn,
    reason = "the shrink budget path must restore the latest failing case before returning"
)]
fn restore_latest_failing_case<V: ValueTree>(
    case: &mut V,
    fork_output: &mut ForkOutput,
) -> Result<(), Reason> {
    while case.complicate() {
        fork_output.append(&Ok(()))?;
    }
    Ok(())
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
        .map_err(|error| {
            Reason::from(format!("Failed to read fork file metadata: {error}"))
        })
}

/// Create the shared replay file used to coordinate forked child runs.
#[cfg(feature = "fork")]
#[allow(
    clippy::single_call_fn,
    reason = "create and initialize the parent replay file for the forked runner protocol"
)]
fn create_parent_replay_file(
    seed: Seed,
) -> Result<(replay::Replay, tempfile::NamedTempFile, PathBuf), Reason> {
    let replay = replay::Replay {
        seed,
        steps: vec![],
    };
    let mut forkfile = tempfile::NamedTempFile::new().map_err(|error| {
        Reason::from(format!(
            "Failed to create temporary file for fork: {error}"
        ))
    })?;
    replay.init_file(&mut forkfile).map_err(|error| {
        Reason::from(format!(
            "Failed to initialise temporary file for fork: {error}"
        ))
    })?;
    let forkfile_path = forkfile.path().to_path_buf();
    Ok((replay, forkfile, forkfile_path))
}

/// Refresh the parent replay state from the shared forkfile.
#[cfg(feature = "fork")]
#[allow(
    clippy::single_call_fn,
    reason = "refresh the parent replay state from the forkfile and classify its parse status"
)]
fn refresh_parent_replay(
    forkfile: &mut tempfile::NamedTempFile,
    replay: &mut replay::Replay,
) -> Result<ParentReplayStatus, Reason> {
    match replay::Replay::parse_from(forkfile).map_err(|error| {
        Reason::from(format!("Failed to re-read fork file: {error}"))
    })? {
        replay::ReplayFileStatus::InProgress(new_replay) => {
            *replay = new_replay;
            Ok(ParentReplayStatus::InProgress)
        }
        replay::ReplayFileStatus::Terminated(new_replay) => {
            *replay = new_replay;
            Ok(ParentReplayStatus::Terminated)
        }
        replay::ReplayFileStatus::Corrupt => {
            Err("Child process corrupted replay file".into())
        }
    }
}

/// Whether the parent should append a synthetic child failure to replay.
#[cfg(feature = "fork")]
#[allow(
    clippy::single_call_fn,
    reason = "name the forkfile-length rule for recording abrupt child termination"
)]
fn should_record_abrupt_child_failure(
    last_fork_file_len: Option<u64>,
    curr_forkfile_size: u64,
) -> bool {
    last_fork_file_len
        .is_none_or(|observed_len| observed_len == curr_forkfile_size)
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
    let error = Err(child_error.unwrap_or_else(|| {
        TestCaseError::fail(
            "Child process was terminated abruptly but with successful status",
        )
    }));
    replay::append(forkfile, &error).map_err(|append_error| {
        Reason::from(format!(
            "Failed to append synthetic fork failure: {append_error}"
        ))
    })?;
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
fn call_test<V, F, R>(
    _runner: &mut TestRunner,
    case: V,
    test_fn: &F,
    replay_from_fork: &mut R,
    result_cache: &mut dyn ResultCache,
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
    final_result.map(|()| {
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
fn call_test<V, F, R>(
    runner: &TestRunner,
    case: V,
    test_fn: &F,
    replay_from_fork: &mut R,
    result_cache: &mut dyn ResultCache,
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
    fork_output.ping().map_err(TestCaseError::fail)?;

    verbose_message!(runner, TRACE, "Next test input: {:?}", case);

    let cache_key = result_cache.key(&ResultCacheKey::new(&case));
    if let Some(result) = result_cache.get(cache_key) {
        verbose_message!(
            runner,
            TRACE,
            "Test input hit cache, skipping execution"
        );
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
        let elapsed_millis =
            u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);

        if elapsed_millis > timeout {
            final_result = Err(TestCaseError::fail(format!(
                "Timeout of {timeout} ms exceeded: test took \
                 {elapsed_millis} ms"
            )));
        }
    }
    #[cfg(not(feature = "timeout"))]
    let final_result = test_result;

    result_cache.put(cache_key, &final_result);
    fork_output
        .append(&final_result)
        .map_err(TestCaseError::fail)?;

    match final_result {
        Ok(()) => verbose_message!(runner, TRACE, "Test case passed"),
        Err(TestCaseError::Reject(ref reason)) => {
            verbose_message!(
                runner,
                INFO_LOG,
                "Test case rejected: {}",
                reason
            );
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
const SHRINK_ITERS_CONTROLLER: &str = "the PROPTEST_MAX_SHRINK_ITERS environment \
     variable or ProptestConfig.max_shrink_iters";
/// Controller text for the shrink-iteration budget in diagnostics.
#[cfg(not(feature = "std"))]
const SHRINK_ITERS_CONTROLLER: &str = "ProptestConfig.max_shrink_iters";

/// Controller text for the shrink-time budget in diagnostics.
#[cfg(feature = "std")]
const SHRINK_TIME_CONTROLLER: &str = "the PROPTEST_MAX_SHRINK_TIME environment \
     variable or ProptestConfig.max_shrink_time";
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
    /// TestRunner::new_with_rng(
    ///     config,
    ///     TestRng::deterministic_rng(algorithm));
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
            config: self.config.clone(),
            successes: 0,
            local_rejects: 0,
            global_rejects: 0,
            rng: self.new_rng(),
            flat_map_regens: Arc::clone(&self.flat_map_regens),
            local_reject_detail: BTreeMap::new(),
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
    pub fn run<S: Strategy>(
        &mut self,
        strategy: &S,
        test_fn: impl Fn(S::Value) -> TestCaseResult,
    ) -> TestRunResult<S> {
        if self.config.fork() {
            self.run_in_fork(strategy, test_fn)
        } else {
            self.run_in_process(strategy, test_fn)
        }
    }

    /// Unreachable stand-in: without the `fork` feature `Config::fork()`
    /// is always false, so `run` never routes here.
    #[cfg(not(feature = "fork"))]
    fn run_in_fork<S: Strategy>(
        &mut self,
        _: &S,
        _: impl Fn(S::Value) -> TestCaseResult,
    ) -> TestRunResult<S> {
        unreachable!()
    }

    /// Run the test in a subprocess, coordinating through a shared
    /// forkfile.
    ///
    /// Spawns children until the replay log terminates, synthesizing a
    /// failure for a crash, nonzero/spurious exit, or timeout, then
    /// replays the recorded steps in-process to recover the shrunken
    /// value and update persistence.
    #[cfg(feature = "fork")]
    fn run_in_fork<S: Strategy>(
        &mut self,
        strategy: &S,
        test_fn: impl Fn(S::Value) -> TestCaseResult,
    ) -> TestRunResult<S> {
        let mut deferred_test_fn = Some(test_fn);

        let Some(configured_test_name) = self.config.test_name else {
            return Err(TestError::Abort(
                "Must supply test_name when forking enabled".into(),
            ));
        };
        let test_name = fork_test::fix_module_path(configured_test_name);
        let (mut replay, mut forkfile, forkfile_path) =
            create_parent_replay_file(self.rng.new_rng_seed())
                .map_err(TestError::Abort)?;
        let mut child_count = 0_u32;
        let timeout = self.config.timeout();

        loop {
            let pre_spawn_forkfile_size =
                forkfile_size(&forkfile).map_err(TestError::Abort)?;
            let (child_error, last_fork_file_len) = rusty_fork::fork(
                test_name,
                rusty_fork_id!(),
                |cmd| {
                    let _configured = cmd.env(ENV_FORK_FILE, &forkfile_path);
                },
                |child, _| await_child(child, &forkfile, timeout),
                || self.run_deferred_child(strategy, &mut deferred_test_fn),
            )
            .map_err(|error| {
                TestError::Abort(format!("Fork failed: {error:?}").into())
            })?;

            match refresh_parent_replay(&mut forkfile, &mut replay)
                .map_err(TestError::Abort)?
            {
                ParentReplayStatus::InProgress => {}
                ParentReplayStatus::Terminated => {
                    break;
                }
            }

            let curr_forkfile_size =
                forkfile_size(&forkfile).map_err(TestError::Abort)?;

            // If the child failed to append *anything* to the forkfile, it
            // crashed or timed out before starting even one test case, so
            // bail.
            if curr_forkfile_size == pre_spawn_forkfile_size {
                return Err(TestError::Abort(
                    "Child process crashed or timed out before the first test \
                     started running; giving up."
                        .into(),
                ));
            }

            // The child only terminates early if it outright crashes or we
            // kill it due to timeout, so add a synthetic failure to the
            // output. But only do this if the length of the fork file is the
            // same as when we last saw it, or if the child was not killed due
            // to timeout. (This is because the child could have appended
            // something to the file after we gave up waiting for it but before
            // we were able to kill it).
            if should_record_abrupt_child_failure(
                last_fork_file_len,
                curr_forkfile_size,
            ) {
                record_abrupt_child_failure(
                    &mut forkfile,
                    &mut replay,
                    child_error,
                )
                .map_err(TestError::Abort)?;
            }

            // Bail if we've gone through too many processes in case the
            // shrinking process itself is crashing.
            child_count = child_count.saturating_add(1);
            if child_count >= 10000 {
                return Err(TestError::Abort(
                    "Giving up after 10000 child processes crashed".into(),
                ));
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
    fn run_in_process<S: Strategy>(
        &mut self,
        strategy: &S,
        test_fn: impl Fn(S::Value) -> TestCaseResult,
    ) -> TestRunResult<S> {
        let (replay_steps, fork_output) =
            init_replay(&mut self.rng).map_err(TestError::Abort)?;
        self.run_in_process_with_replay(
            strategy,
            test_fn,
            replay_steps.into_iter(),
            fork_output,
        )
    }

    /// The core case loop: replay persisted failures first (RNG saved and
    /// restored around them), then generate and run fresh cases until
    /// `cases` successes, persisting the seed of any failing case unless
    /// this is the fork child.
    fn run_in_process_with_replay<S: Strategy>(
        &mut self,
        strategy: &S,
        test_fn: impl Fn(S::Value) -> TestCaseResult,
        mut replay_from_fork: impl Iterator<Item = TestCaseResult>,
        mut fork_output: ForkOutput,
    ) -> TestRunResult<S> {
        let old_rng = self.rng.clone();

        let persisted_failure_seeds: Vec<PersistedSeed> = self
            .config
            .failure_persistence
            .as_ref()
            .map(|f| f.load_persisted_failures2(self.config.source_file))
            .unwrap_or_default();

        let mut result_cache = self.new_cache();

        for PersistedSeed(persisted_seed) in
            persisted_failure_seeds.into_iter().rev()
        {
            self.rng.set_seed(persisted_seed);
            self.gen_and_run_case(
                strategy,
                &test_fn,
                &mut replay_from_fork,
                &mut *result_cache,
                &mut fork_output,
                true,
            )?;
        }
        self.rng = old_rng;

        while self.successes < self.config.cases {
            // Generate a new seed and make an RNG from that so that we know
            // what seed to persist if this case fails.
            let seed = self.rng.gen_get_seed();
            let result = self.gen_and_run_case(
                strategy,
                &test_fn,
                &mut replay_from_fork,
                &mut *result_cache,
                &mut fork_output,
                false,
            );
            let source_file = self.config.source_file;

            // Don't update the persistence file if we're a child process. The
            // parent relies on it remaining consistent and will take care of
            // updating it itself.
            if let Err(TestError::Fail(_, ref shrunken_value)) = result
                && let Some(ref mut failure_persistence) =
                    self.config.failure_persistence
                && !fork_output.is_in_fork()
            {
                failure_persistence.save_persisted_failure2(
                    source_file,
                    PersistedSeed(seed),
                    shrunken_value,
                );
            }

            if let Err(error) = result {
                fork_output.terminate().map_err(TestError::Abort)?;
                return Err(error);
            }
        }

        fork_output.terminate().map_err(TestError::Abort)?;
        Ok(())
    }

    /// Build one input from `strategy` (an `Err` becomes `TestError::Abort`)
    /// and run it, advancing `successes` only for genuinely new or
    /// fork-replayed passes.
    fn gen_and_run_case<S: Strategy>(
        &mut self,
        strategy: &S,
        f: &impl Fn(S::Value) -> TestCaseResult,
        replay_from_fork: &mut impl Iterator<Item = TestCaseResult>,
        result_cache: &mut dyn ResultCache,
        fork_output: &mut ForkOutput,
        is_from_persisted_seed: bool,
    ) -> TestRunResult<S> {
        let case = unwrap_or!(strategy.new_tree(self), msg =>
                return Err(TestError::Abort(msg)));

        // We only count new cases to our set of successful runs against
        // `PROPTEST_CASES` config.
        let ok_type = self.run_one_with_replay(
            case,
            f,
            replay_from_fork,
            result_cache,
            fork_output,
            is_from_persisted_seed,
        )?;
        match ok_type {
            TestCaseOk::NewCaseSuccess | TestCaseOk::ReplayFromForkSuccess => {
                self.successes = self.successes.saturating_add(1);
            }
            TestCaseOk::PersistedCaseSuccess
            | TestCaseOk::CacheHitSuccess
            | TestCaseOk::Reject => (),
        }

        Ok(())
    }

    /// Run one specific test case against this runner.
    ///
    /// If the test fails, finds the minimal failing test case. If the test
    /// does not fail, returns whether it succeeded or was filtered out.
    ///
    /// This does not honour the `fork` config, and will not be able to
    /// terminate the run if it runs for longer than `timeout`. However, if the
    /// test function returns but took longer than `timeout`, the test case
    /// will fail.
    ///
    /// ## Errors
    ///
    /// Returns `TestError::Fail` with the minimized input if the case
    /// fails, or `TestError::Abort` if too many inputs are rejected.
    pub fn run_one<V: ValueTree>(
        &mut self,
        case: V,
        test_fn: impl Fn(V::Value) -> TestCaseResult,
    ) -> Result<bool, TestError<V::Value>> {
        let mut result_cache = self.new_cache();
        self.run_one_with_replay(
            case,
            test_fn,
            &mut iter::empty::<TestCaseResult>().fuse(),
            &mut *result_cache,
            &mut ForkOutput::empty(),
            false,
        )
        .map(|ok_type| !matches!(ok_type, TestCaseOk::Reject))
    }

    /// Run one pre-built `case` once, then shrink it on failure.
    ///
    /// On `Fail` enters the shrink loop and returns the minimized value;
    /// on `Reject` charges the global reject budget and reports the
    /// non-counting outcome.
    fn run_one_with_replay<V: ValueTree>(
        &mut self,
        mut case: V,
        test_fn: impl Fn(V::Value) -> TestCaseResult,
        replay_from_fork: &mut impl Iterator<Item = TestCaseResult>,
        result_cache: &mut dyn ResultCache,
        fork_output: &mut ForkOutput,
        is_from_persisted_seed: bool,
    ) -> Result<TestCaseOk, TestError<V::Value>> {
        let result = call_test(
            self,
            case.current(),
            &test_fn,
            replay_from_fork,
            result_cache,
            fork_output,
            is_from_persisted_seed,
        );

        match result {
            Ok(success_type) => Ok(success_type),
            Err(TestCaseError::Fail(failure_reason)) => {
                let shrunk_reason = self
                    .shrink(
                        &mut case,
                        test_fn,
                        replay_from_fork,
                        result_cache,
                        fork_output,
                        is_from_persisted_seed,
                    )
                    .map_err(TestError::Abort)?
                    .unwrap_or(failure_reason);
                Err(TestError::Fail(shrunk_reason, case.current()))
            }
            Err(TestCaseError::Reject(whence)) => {
                self.reject_global(whence)?;
                Ok(TestCaseOk::Reject)
            }
        }
    }

    /// Minimize a failing `case` by walking `simplify`/`complicate`.
    ///
    /// Returns the most recent failing `Reason`, or `None` if shrinking
    /// is disabled or the first simplification does not reproduce the
    /// failure. Stops on an exhausted tree or a spent iteration/time
    /// budget, backtracking to the last failing value before returning.
    fn shrink<V: ValueTree>(
        &self,
        case: &mut V,
        test_fn: impl Fn(V::Value) -> TestCaseResult,
        replay_from_fork: &mut impl Iterator<Item = TestCaseResult>,
        result_cache: &mut dyn ResultCache,
        fork_output: &mut ForkOutput,
        is_from_persisted_seed: bool,
    ) -> Result<Option<Reason>, Reason> {
        // exit early if shrink disabled
        if self.config.max_shrink_iters == 0 {
            verbose_message!(
                self,
                INFO_LOG,
                "Shrinking disabled by configuration"
            );
            return Ok(None);
        }

        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        let start_time = Instant::now();
        let mut last_failure = None;
        let mut iterations = 0;

        verbose_message!(self, TRACE, "Starting shrinking");

        if !case.simplify() {
            return Ok(last_failure);
        }

        loop {
            #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
            let timed_out: Option<u64> = self.shrink_time_exceeded(start_time);
            #[cfg(not(all(feature = "std", not(target_arch = "wasm32"))))]
            let timed_out: Option<u64> = None;

            if self.shrink_budget_exhausted(iterations, timed_out) {
                restore_latest_failing_case(case, fork_output)?;
                break;
            }

            iterations = iterations.saturating_add(1);

            let result = call_test(
                self,
                case.current(),
                &test_fn,
                replay_from_fork,
                result_cache,
                fork_output,
                is_from_persisted_seed,
            );

            let walked = match result {
                // Rejections are effectively a pass here,
                // since they indicate that any behaviour of
                // the function under test is acceptable.
                Ok(_) | Err(TestCaseError::Reject(..)) => {
                    self.complicate_or_note(case)
                }
                Err(TestCaseError::Fail(why)) => {
                    last_failure = Some(why);
                    self.simplify_or_note(case)
                }
            };
            if !walked {
                break;
            }
        }

        Ok(last_failure)
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
        (elapsed_ms > u64::from(self.config.max_shrink_time))
            .then_some(elapsed_ms)
    }

    /// Whether the shrink walk must stop early because a budget is spent —
    /// the iteration cap (`max_shrink_iters`) or the wall clock
    /// (`max_shrink_time`) — emitting the ALWAYS-level diagnostic that
    /// names the knob to raise.
    fn shrink_budget_exhausted(
        &self,
        iterations: u32,
        timed_out: Option<u64>,
    ) -> bool {
        if iterations >= self.config.max_shrink_iters() {
            verbose_message!(
                self,
                ALWAYS,
                "Aborting shrinking after {} iterations (set {} \
                 to a large(r) value to shrink more; current \
                 configuration: {} iterations)",
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
            "Aborting shrinking after taking too long: {} ms \
                 (set {} to a large(r) value to shrink more; current \
                 configuration: {} ms)",
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
    fn run_deferred_child<S, F>(
        &mut self,
        strategy: &S,
        deferred_test_fn: &mut Option<F>,
    ) where
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
    pub fn reject_local(
        &mut self,
        whence: impl Into<Reason>,
    ) -> Result<(), Reason> {
        if self.local_rejects >= self.config.max_local_rejects {
            Err("Too many local rejects".into())
        } else {
            self.local_rejects = self.local_rejects.saturating_add(1);
            Self::insert_or_increment(
                &mut self.local_reject_detail,
                whence.into(),
            );
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
        u32::try_from(self.flat_map_regens.fetch_add(1, SeqCst))
            .is_ok_and(|regens| regens < self.config.max_flat_map_regens)
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
fn init_replay(
    rng: &mut TestRng,
) -> Result<(Vec<TestCaseResult>, ForkOutput), Reason> {
    use crate::test_runner::replay::{Replay, ReplayFileStatus, open_file};

    env::var_os(ENV_FORK_FILE).map_or_else(
        || Ok((vec![], ForkOutput::empty())),
        |path| {
            let mut file = open_file(&path).map_err(|error| {
                Reason::from(format!("Failed to open replay file: {error}"))
            })?;
            let loaded = Replay::parse_from(&mut file).map_err(|error| {
                Reason::from(format!("Failed to read replay file: {error}"))
            })?;
            match loaded {
                ReplayFileStatus::InProgress(replay) => {
                    rng.set_seed(replay.seed);
                    Ok((replay.steps, ForkOutput { file: Some(file) }))
                }

                ReplayFileStatus::Terminated(_) => {
                    Err("Replay file for child process is terminated".into())
                }

                ReplayFileStatus::Corrupt => {
                    Err("Replay file for child process is corrupt".into())
                }
            }
        },
    )
}

/// Without the `fork` feature there is never a replay: no steps and an
/// empty `ForkOutput`.
#[cfg(not(feature = "fork"))]
fn init_replay(
    _rng: &mut TestRng,
) -> Result<(iter::Empty<TestCaseResult>, ForkOutput), Reason> {
    Ok((iter::empty(), ForkOutput::empty()))
}

/// Wait for a fork child to exit, mapping a nonzero exit status into a
/// synthetic case failure.
#[cfg(feature = "fork")]
#[allow(
    clippy::single_call_fn,
    reason = "wait for a fork child unconditionally, turning a nonzero exit into a synthetic failure"
)]
fn await_child_without_timeout(
    child: &mut rusty_fork::ChildWrapper,
) -> (Option<TestCaseError>, Option<u64>) {
    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            return (
                Some(TestCaseError::fail(format!(
                    "Failed to wait for child process: {error}"
                ))),
                None,
            );
        }
    };

    if status.success() {
        (None, None)
    } else {
        (
            Some(TestCaseError::fail(format!(
                "Child process exited with {status}"
            ))),
            None,
        )
    }
}

/// Wait for a fork child to exit; with the `timeout` feature off this
/// just defers to `await_child_without_timeout`.
#[cfg(all(feature = "fork", not(feature = "timeout")))]
fn await_child(
    child: &mut rusty_fork::ChildWrapper,
    _: &tempfile::NamedTempFile,
    _timeout: u32,
) -> (Option<TestCaseError>, Option<u64>) {
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
                Some(TestCaseError::fail(format!(
                    "Failed to read fork file metadata: {error}"
                ))),
                None,
            );
        }
    };

    loop {
        let wait_status =
            match child.wait_timeout(Duration::from_millis(timeout.into())) {
                Ok(status) => status,
                Err(error) => {
                    return (
                        Some(TestCaseError::fail(format!(
                            "Failed to wait for child process: {error}"
                        ))),
                        Some(last_forkfile_len),
                    );
                }
            };

        if let Some(status) = wait_status {
            if status.success() {
                return (None, None);
            }
            return (
                Some(TestCaseError::fail(format!(
                    "Child process exited with {status}"
                ))),
                None,
            );
        }

        let current_len = match forkfile.as_file().metadata() {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                return (
                    Some(TestCaseError::fail(format!(
                        "Failed to read fork file metadata: {error}"
                    ))),
                    Some(last_forkfile_len),
                );
            }
        };
        // If we've gone a full timeout period without the file growing,
        // fail the test and kill the child.
        if current_len <= last_forkfile_len {
            return (
                Some(TestCaseError::fail(
                    "Timed out waiting for child process",
                )),
                Some(current_len),
            );
        }
        last_forkfile_len = current_len;
    }
}

#[cfg(test)]
mod test {
    use std::cell::Cell;
    use std::fs;
    #[cfg(feature = "timeout")]
    use std::thread;
    #[cfg(feature = "timeout")]
    use std::time::Duration;

    use super::*;
    use crate::test_runner::result_cache::basic_result_cache;
    use crate::test_runner::{
        FileFailurePersistence, RngAlgorithm, RngSeed, TestRng,
    };
    use strict_test_support::{
        TestFailure, capture_ignored_test, ensure, ensure_contains, ensure_eq,
        ensure_ok, ensure_some,
    };

    const PERSISTED_COUNTING_CHILD: &str = "test_runner::runner::test::\
         persisted_cases_do_not_count_towards_total_cases_child";
    const PERSISTED_RELOAD_CHILD: &str = "test_runner::runner::test::\
         failing_cases_persisted_and_reloaded_child";

    #[test]
    fn gives_up_after_too_many_rejections() -> Result<(), TestFailure> {
        let config = runner_test_config();
        let mut runner = TestRunner::new(config.clone());
        let runs = Cell::new(0);
        let result = runner.run(&(0_u32..), |_| {
            runs.set(runs.get() + 1);
            Err(TestCaseError::reject("reject"))
        });
        ensure(
            matches!(result, Err(TestError::Abort(_))),
            "exhausting the global reject budget aborts the run",
        )?;
        ensure_eq(
            &(config.max_global_rejects + 1),
            &runs.get(),
            "the runner stops after the budget plus the aborting case",
        )
    }

    #[test]
    fn test_pass() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(runner_test_config());
        let result = runner.run(&(1_u32..), |candidate| {
            if candidate > 0 {
                Ok(())
            } else {
                Err(TestCaseError::fail("generated value must be positive"))
            }
        });
        ensure(result == Ok(()), "a passing property returns Ok")
    }

    #[test]
    fn test_fail_via_result() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(runner_test_config());
        let result = runner.run(&(0_u32..10_u32), |candidate| {
            if candidate < 5 {
                Ok(())
            } else {
                Err(TestCaseError::fail("not less than 5"))
            }
        });

        ensure(
            result == Err(TestError::Fail("not less than 5".into(), 5)),
            "a result failure shrinks to the boundary value",
        )
    }

    #[test]
    fn test_fail_via_panic() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(runner_test_config());
        let result = runner.run(&(0_u32..10_u32), |candidate| {
            // Legacy-surface test: the panic inside the closure IS the
            // subject — it proves the runner converts a panicking case into
            // TestError::Fail.
            if candidate >= 5 {
                panic::resume_unwind(Box::new("not less than 5"));
            }
            Ok(())
        });
        ensure(
            result == Err(TestError::Fail("not less than 5".into(), 5)),
            "a panicking case is caught and shrinks to the boundary value",
        )
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
    fn persisted_cases_do_not_count_towards_total_cases()
    -> Result<(), TestFailure> {
        let captured = capture_ignored_test(PERSISTED_COUNTING_CHILD)?;
        ensure(
            captured.status.success(),
            "the captured persistence-counting child passes",
        )?;
        ensure_contains(
            &captured.stderr,
            "Saving this and future failures in persistence-test-counting.txt",
            "the persistence-counting save diagnostic is captured",
        )
    }

    #[test]
    #[ignore = "captured by persisted_cases_do_not_count_towards_total_cases"]
    fn persisted_cases_do_not_count_towards_total_cases_child()
    -> Result<(), TestFailure> {
        const FILE: &str = "persistence-test-counting.txt";
        let _guard = PersistenceFileGuard(FILE);
        drop(fs::remove_file(FILE));

        let config = Config {
            failure_persistence: Some(Box::new(
                FileFailurePersistence::Direct(FILE),
            )),
            cases: 1,
            ..runner_test_config()
        };

        let max = 10_000_000_i32;
        ensure(
            TestRunner::new(config.clone())
                .run(&(0_i32..max), |_v| {
                    Err(TestCaseError::Fail("persist a failure".into()))
                })
                .is_err(),
            "the seeding run must fail so a seed is persisted",
        )?;

        let run_count = Cell::new(0);
        ensure_ok(
            TestRunner::new(config).run(&(0_i32..max), |_v| {
                run_count.set(run_count.get() + 1);
                Ok(())
            }),
            "the replay run succeeds",
        )?;

        // Persisted ran, and a new case ran, and only new case counts
        // against `cases: 1`.
        ensure_eq(
            &run_count.get(),
            &2,
            "the persisted replay does not count toward cases",
        )
    }

    #[derive(Clone, Copy, PartialEq)]
    struct PoorlyBehavedDebug(i32);
    impl fmt::Debug for PoorlyBehavedDebug {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "\r\n{:?}\r\n", self.0)
        }
    }

    #[test]
    fn failing_cases_persisted_and_reloaded() -> Result<(), TestFailure> {
        let captured = capture_ignored_test(PERSISTED_RELOAD_CHILD)?;
        ensure(
            captured.status.success(),
            "the captured persistence-reload child passes",
        )?;
        ensure_contains(
            &captured.stderr,
            "Saving this and future failures in persistence-test-reload.txt",
            "the persistence-reload save diagnostic is captured",
        )
    }

    #[test]
    #[ignore = "captured by failing_cases_persisted_and_reloaded"]
    fn failing_cases_persisted_and_reloaded_child() -> Result<(), TestFailure> {
        const FILE: &str = "persistence-test-reload.txt";
        let _guard = PersistenceFileGuard(FILE);
        drop(fs::remove_file(FILE));

        let max = 10_000_000_i32;
        let input = (0_i32..max).prop_map(PoorlyBehavedDebug);
        let config = Config {
            failure_persistence: Some(Box::new(
                FileFailurePersistence::Direct(FILE),
            )),
            ..runner_test_config()
        };

        // First test with cases that fail above half max, and then below half
        // max, to ensure we can correctly parse both lines of the persistence
        // file.
        let midpoint = max.div_euclid(2);
        let first_sub_failure = ensure_some(
            TestRunner::new(config.clone())
                .run(&input, |candidate| {
                    if candidate.0 < midpoint {
                        Ok(())
                    } else {
                        Err(TestCaseError::Fail("too big".into()))
                    }
                })
                .err(),
            "the first sub-max run must fail",
        )?;
        let first_super_failure = ensure_some(
            TestRunner::new(config.clone())
                .run(&input, |candidate| {
                    if candidate.0 >= midpoint {
                        Ok(())
                    } else {
                        Err(TestCaseError::Fail("too small".into()))
                    }
                })
                .err(),
            "the first super-max run must fail",
        )?;
        let second_sub_failure = ensure_some(
            TestRunner::new(config.clone())
                .run(&input, |candidate| {
                    if candidate.0 < midpoint {
                        Ok(())
                    } else {
                        Err(TestCaseError::Fail("too big".into()))
                    }
                })
                .err(),
            "the second sub-max run must fail",
        )?;
        let second_super_failure = ensure_some(
            TestRunner::new(config)
                .run(&input, |candidate| {
                    if candidate.0 >= midpoint {
                        Ok(())
                    } else {
                        Err(TestCaseError::Fail("too small".into()))
                    }
                })
                .err(),
            "the second super-max run must fail",
        )?;

        ensure(
            first_sub_failure == second_sub_failure,
            "the persisted sub-max failure replays identically",
        )?;
        ensure(
            first_super_failure == second_super_failure,
            "the persisted super-max failure replays identically",
        )
    }

    #[test]
    fn new_rng_makes_separate_rng() -> Result<(), TestFailure> {
        use rand::RngExt as _;
        let mut runner = TestRunner::new(runner_test_config());
        let from_1 = runner.new_rng().random::<[u8; 16]>();
        let from_2 = runner.rng().random::<[u8; 16]>();
        ensure(from_1 != from_2, "a new rng draws a different stream")
    }

    #[test]
    fn record_rng_use() -> Result<(), TestFailure> {
        use rand::RngExt as _;

        // create value with recorder rng
        let default_config = runner_test_config();
        let recorder_rng =
            TestRng::default_rng(RngSeed::Random, RngAlgorithm::Recorder);
        let mut recorder_runner =
            TestRunner::new_with_rng(default_config.clone(), recorder_rng);
        let random_byte_array1 = recorder_runner.rng().random::<[u8; 16]>();
        let bytes_used = ensure_some(
            recorder_runner.bytes_used(),
            "recorder runner exposes captured bytes",
        )?;
        // could use more bytes for some reason
        ensure(
            bytes_used.len() >= 16,
            "the recorder captured at least the drawn bytes",
        )?;

        // re-create value with pass-through rng
        let passthrough_rng =
            TestRng::from_seed(RngAlgorithm::PassThrough, &bytes_used);
        let mut passthrough_runner =
            TestRunner::new_with_rng(default_config, passthrough_rng);
        let random_byte_array2 = passthrough_runner.rng().random::<[u8; 16]>();

        // make sure the same value was created
        ensure(
            random_byte_array1 == random_byte_array2,
            "replaying recorded bytes recreates the same value",
        )
    }

    #[cfg(feature = "fork")]
    #[test]
    fn run_successful_test_in_fork() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            test_name: Some(concat!(
                module_path!(),
                "::run_successful_test_in_fork"
            )),
            ..runner_test_config()
        });

        ensure(
            runner.run(&(0_u32..1000), |_| Ok(())).is_ok(),
            "a passing forked run returns Ok",
        )
    }

    #[cfg(feature = "fork")]
    #[test]
    fn normal_failure_in_fork_results_in_correct_failure()
    -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            test_name: Some(concat!(
                module_path!(),
                "::normal_failure_in_fork_results_in_correct_failure"
            )),
            ..runner_test_config()
        });

        let failure = ensure_some(
            runner
                .run(&(0_u32..1000), |candidate| {
                    if candidate < 500 {
                        Ok(())
                    } else {
                        Err(TestCaseError::fail("value reached 500"))
                    }
                })
                .err(),
            "a failing forked run must return the failure",
        )?;

        match failure {
            TestError::Fail(_, value) => {
                ensure_eq(&500, &value, "the forked failure shrinks to 500")
            }
            TestError::Abort(_) => {
                ensure(false, "the forked failure must be Fail, not Abort")
            }
        }
    }

    // Fork-surface test: the child reports a failure and the parent shrinks
    // it through the fork boundary.
    #[cfg(feature = "fork")]
    #[test]
    fn nonsuccessful_exit_finds_correct_failure() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            test_name: Some(concat!(
                module_path!(),
                "::nonsuccessful_exit_finds_correct_failure"
            )),
            ..runner_test_config()
        });

        let failure = ensure_some(
            runner
                .run(&(0_u32..1000), |candidate| {
                    if candidate >= 500 {
                        return Err(TestCaseError::fail(
                            "child reported a failure",
                        ));
                    }
                    Ok(())
                })
                .err(),
            "a crashing child must surface as a failure",
        )?;

        match failure {
            TestError::Fail(_, value) => {
                ensure_eq(&500, &value, "the crash shrinks to 500")
            }
            TestError::Abort(_) => {
                ensure(false, "the crash must be Fail, not Abort")
            }
        }
    }

    // Fork-surface test: the child reports a failure after earlier cases
    // pass, so the parent still has to shrink across process isolation.
    #[cfg(feature = "fork")]
    #[test]
    fn spurious_exit_finds_correct_failure() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            test_name: Some(concat!(
                module_path!(),
                "::spurious_exit_finds_correct_failure"
            )),
            ..runner_test_config()
        });

        let failure = ensure_some(
            runner
                .run(&(0_u32..1000), |candidate| {
                    if candidate >= 500 {
                        return Err(TestCaseError::fail(
                            "child reported a late failure",
                        ));
                    }
                    Ok(())
                })
                .err(),
            "a spuriously exiting child must surface as a failure",
        )?;

        match failure {
            TestError::Fail(_, value) => {
                ensure_eq(&500, &value, "the spurious exit shrinks to 500")
            }
            TestError::Abort(_) => {
                ensure(false, "the spurious exit must be Fail, not Abort")
            }
        }
    }

    #[cfg(feature = "timeout")]
    #[test]
    fn long_sleep_timeout_finds_correct_failure() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            timeout: 500,
            test_name: Some(concat!(
                module_path!(),
                "::long_sleep_timeout_finds_correct_failure"
            )),
            ..runner_test_config()
        });

        let failure = ensure_some(
            runner
                .run(&(0_u32..1000), |candidate| {
                    if candidate >= 500 {
                        thread::sleep(Duration::from_millis(10_000));
                    }
                    Ok(())
                })
                .err(),
            "a long-sleeping case must time out into a failure",
        )?;

        match failure {
            TestError::Fail(_, value) => {
                ensure_eq(&500, &value, "the timeout shrinks to 500")
            }
            TestError::Abort(_) => {
                ensure(false, "the timeout must be Fail, not Abort")
            }
        }
    }

    #[cfg(feature = "timeout")]
    #[test]
    fn mid_sleep_timeout_finds_correct_failure() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            timeout: 500,
            test_name: Some(concat!(
                module_path!(),
                "::mid_sleep_timeout_finds_correct_failure"
            )),
            ..runner_test_config()
        });

        let failure = ensure_some(
            runner
                .run(&(0_u32..1000), |candidate| {
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
                })
                .err(),
            "a mid-sleep case must time out into a failure",
        )?;

        match failure {
            TestError::Fail(_, value) => {
                ensure_eq(&500, &value, "the mid-sleep timeout shrinks to 500")
            }
            TestError::Abort(_) => {
                ensure(false, "the mid-sleep timeout must be Fail, not Abort")
            }
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn duplicate_tests_not_run_with_basic_result_cache()
    -> Result<(), TestFailure> {
        use std::cell::{Cell, RefCell};
        use std::collections::HashSet;
        use std::rc::Rc;

        fn record_candidate_once(
            seen: &RefCell<HashSet<u32>>,
            candidate: u32,
        ) -> bool {
            seen.try_borrow_mut()
                .is_ok_and(|mut seen_values| seen_values.insert(candidate))
        }

        fn reject_above_five(candidate: u32) -> TestCaseResult {
            match candidate {
                0..=5 => Ok(()),
                _ => Err(TestCaseError::fail("value above 5")),
            }
        }

        for _ in 0..256 {
            let mut runner = TestRunner::new(Config {
                result_cache: basic_result_cache,
                ..runner_test_config()
            });
            let pass = Rc::new(Cell::new(true));
            let seen = Rc::new(RefCell::new(HashSet::new()));
            let result = runner.run(
                &(0_u32..65536_u32).prop_map(|raw| raw.rem_euclid(10)),
                |candidate| {
                    let inserted = record_candidate_once(&seen, candidate);
                    pass.set(pass.get() && inserted);
                    reject_above_five(candidate)
                },
            );

            ensure(pass.get(), "no cached value ran more than once")?;
            match result {
                Err(TestError::Fail(_, val)) => {
                    ensure_eq(&6, &val, "the failure shrinks to 6")?;
                }
                _ => {
                    ensure(
                        false,
                        "the cached run must fail with Fail, not pass or abort",
                    )?;
                }
            }
        }
        Ok(())
    }
}

// Legacy-surface module: `rusty_fork_test!` only accepts unit-returning
// `#[test]` bodies (the fork protocol reads the child's exit status), so
// these tests cannot return `Result<(), TestFailure>` — a panic in the
// child is the failure signal the harness is built around.
#[cfg(test)]
#[cfg(feature = "fork")]
#[cfg(feature = "timeout")]
mod timeout_tests {
    use std::string::ToString as _;
    use std::thread;
    use std::time::Duration;

    use rusty_fork::rusty_fork_test;

    use super::*;
    use crate::num::u64 as num_u64;
    use crate::strategy::Just;
    use strict_test_support::ensure;

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 4_000)]

        #[test]
        fn max_shrink_iters_works() {
            run_shrink_bail(Config {
                max_shrink_iters: 5,
                ..runner_test_config()
            });
        }

        #[test]
        fn max_shrink_time_works() {
            run_shrink_bail(Config {
                max_shrink_time: 1000,
                ..runner_test_config()
            });
        }

        #[test]
        fn max_shrink_iters_works_with_forking() {
            run_shrink_bail(Config {
                fork: true,
                test_name: Some(
                    concat!(module_path!(),
                            "::max_shrink_iters_works_with_forking")),
                max_shrink_time: 1000,
                ..runner_test_config()
            });
        }

        #[test]
        fn detects_child_failure_to_start() {
            let mut runner = TestRunner::new(Config {
                timeout: 100,
                test_name: Some(concat!(
                    module_path!(),
                    "::detects_child_failure_to_start"
                )),
                ..runner_test_config()
            });
            let result = runner.run(
                &Just(()).prop_map(|()| {
                    thread::sleep(Duration::from_millis(200));
                }),
                Ok,
            );

            if let Err(failure) = ensure(
                matches!(result, Err(TestError::Abort(_))),
                "a child that fails to start is reported as an abort",
            ) {
                panic::resume_unwind(Box::new(failure.to_string()));
            }
        }
    }

    fn run_shrink_bail(config: Config) {
        let mut runner = TestRunner::new(config);
        let result = runner.run(&num_u64::ANY, |candidate| {
            thread::sleep(Duration::from_millis(250));
            if u32::try_from(candidate).is_ok() {
                Ok(())
            } else {
                Err(TestCaseError::fail("value exceeds u32::MAX"))
            }
        });

        let verdict = match result {
            Err(TestError::Fail(_, failing_value)) => ensure(
                failing_value > u64::from(u32::MAX),
                "the final value remains a failing case",
            ),
            Err(TestError::Abort(_)) | Ok(()) => {
                ensure(false, "shrinking bails with a failing case")
            }
        };

        if let Err(failure) = verdict {
            panic::resume_unwind(Box::new(failure.to_string()));
        }
    }
}
