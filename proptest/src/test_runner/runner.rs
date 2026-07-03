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

#[cfg(feature = "fork")]
use rusty_fork;
#[cfg(feature = "fork")]
use std::env;
#[cfg(feature = "fork")]
use std::fs;
#[cfg(feature = "fork")]
use tempfile;

use crate::strategy::*;
use crate::test_runner::config::*;
use crate::test_runner::errors::*;
use crate::test_runner::failure_persistence::PersistedSeed;
use crate::test_runner::reason::*;
#[cfg(feature = "fork")]
use crate::test_runner::replay;
use crate::test_runner::result_cache::*;
use crate::test_runner::rng::TestRng;

#[cfg(feature = "fork")]
const ENV_FORK_FILE: &str = "_PROPTEST_FORKFILE";

const ALWAYS: u32 = 0;
/// Verbose level 1 to show failures. In state machine tests this level is used
/// to print transitions.
pub const INFO_LOG: u32 = 1;
const TRACE: u32 = 2;

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

#[cfg(not(feature = "std"))]
macro_rules! verbose_message {
    ($runner:expr, $level:expr, $fmt:tt $($arg:tt)*) => {{
        let _ = &$runner;
        let _ = $level;
        let _ = format_args!($fmt $($arg)*);
    }};
}

type RejectionDetail = BTreeMap<Reason, u32>;

/// State used when running a proptest test.
#[derive(Clone)]
pub struct TestRunner {
    config: Config,
    successes: u32,
    local_rejects: u32,
    global_rejects: u32,
    rng: TestRng,
    flat_map_regens: Arc<AtomicUsize>,

    local_reject_detail: RejectionDetail,
    global_reject_detail: RejectionDetail,
}

impl fmt::Debug for TestRunner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\tsuccesses: {}\n\
             \tlocal rejects: {}\n",
            self.successes, self.local_rejects
        )?;
        for (whence, count) in &self.local_reject_detail {
            writeln!(f, "\t\t{} times at {}", count, whence)?;
        }
        writeln!(f, "\tglobal rejects: {}", self.global_rejects)?;
        for (whence, count) in &self.global_reject_detail {
            writeln!(f, "\t\t{} times at {}", count, whence)?;
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

#[cfg(feature = "fork")]
#[derive(Debug)]
struct ForkOutput {
    file: Option<fs::File>,
}

#[cfg(feature = "fork")]
impl ForkOutput {
    fn append(&mut self, result: &TestCaseResult) {
        if let Some(ref mut file) = self.file {
            replay::append(file, result)
                .expect("Failed to append to replay file");
        }
    }

    fn ping(&mut self) {
        if let Some(ref mut file) = self.file {
            replay::ping(file).expect("Failed to append to replay file");
        }
    }

    fn terminate(&mut self) {
        if let Some(ref mut file) = self.file {
            replay::terminate(file).expect("Failed to append to replay file");
        }
    }

    fn empty() -> Self {
        ForkOutput { file: None }
    }

    fn is_in_fork(&self) -> bool {
        self.file.is_some()
    }
}

#[cfg(not(feature = "fork"))]
#[derive(Debug)]
struct ForkOutput;

#[cfg(not(feature = "fork"))]
impl ForkOutput {
    fn append(&mut self, _result: &TestCaseResult) {}
    #[cfg(feature = "std")]
    fn ping(&mut self) {}
    fn terminate(&mut self) {}
    fn empty() -> Self {
        ForkOutput
    }
    fn is_in_fork(&self) -> bool {
        false
    }
}

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
        return result.map(|_| TestCaseOk::ReplayFromForkSuccess);
    }

    let cache_key = result_cache.key(&ResultCacheKey::new(&case));
    if let Some(result) = result_cache.get(cache_key) {
        return result.clone().map(|_| TestCaseOk::CacheHitSuccess);
    }

    let result = test_fn(case);
    result_cache.put(cache_key, &result);
    result.map(|_| {
        if is_from_persisted_seed {
            TestCaseOk::PersistedCaseSuccess
        } else {
            TestCaseOk::NewCaseSuccess
        }
    })
}

#[cfg(feature = "std")]
fn call_test<V, F, R>(
    runner: &mut TestRunner,
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
        return result.map(|_| TestCaseOk::ReplayFromForkSuccess);
    }

    // Now that we're about to start a new test (as far as the replay system is
    // concerned), ping the replay file so the parent process can determine
    // that we made it this far.
    fork_output.ping();

    verbose_message!(runner, TRACE, "Next test input: {:?}", case);

    let cache_key = result_cache.key(&ResultCacheKey::new(&case));
    if let Some(result) = result_cache.get(cache_key) {
        verbose_message!(
            runner,
            TRACE,
            "Test input hit cache, skipping execution"
        );
        return result.clone().map(|_| TestCaseOk::CacheHitSuccess);
    }

    #[cfg(feature = "timeout")]
    let time_start = std::time::Instant::now();

    let result = unwrap_or!(
        super::scoped_panic_hook::with_hook(
            |_| { /* Silence out panic backtrace */ },
            || panic::catch_unwind(AssertUnwindSafe(|| test_fn(case)))
        ),
        what => Err(TestCaseError::Fail(
            what.downcast::<&'static str>().map(|s| (*s).into())
                .or_else(|what| what.downcast::<String>().map(|b| (*b).into()))
                .or_else(|what| what.downcast::<Box<str>>().map(|b| (*b).into()))
                .unwrap_or_else(|_| "<unknown panic value>".into()))));

    // If there is a timeout and we exceeded it, fail the test here so we get
    // consistent behaviour. (The parent process cannot precisely time the test
    // cases itself.)
    #[cfg(feature = "timeout")]
    let mut result = result;
    #[cfg(feature = "timeout")]
    if timeout > 0 && result.is_ok() {
        let elapsed = time_start.elapsed();
        let elapsed_millis =
            elapsed.as_secs() as u32 * 1000 + elapsed.subsec_millis();

        if elapsed_millis > timeout {
            result = Err(TestCaseError::fail(format!(
                "Timeout of {} ms exceeded: test took {} ms",
                timeout, elapsed_millis
            )));
        }
    }

    result_cache.put(cache_key, &result);
    fork_output.append(&result);

    match result {
        Ok(()) => verbose_message!(runner, TRACE, "Test case passed"),
        Err(TestCaseError::Reject(ref reason)) => {
            verbose_message!(runner, INFO_LOG, "Test case rejected: {}", reason)
        }
        Err(TestCaseError::Fail(ref reason)) => {
            verbose_message!(runner, INFO_LOG, "Test case failed: {}", reason)
        }
    }

    result.map(|_| {
        if is_from_persisted_seed {
            TestCaseOk::PersistedCaseSuccess
        } else {
            TestCaseOk::NewCaseSuccess
        }
    })
}

type TestRunResult<S> = Result<(), TestError<<S as Strategy>::Value>>;

impl TestRunner {
    /// Create a fresh `TestRunner` with the given configuration.
    ///
    /// The runner will use an RNG with a generated seed and the default
    /// algorithm.
    ///
    /// In `no_std` environments, every `TestRunner` will use the same
    /// hard-coded seed. This seed is not contractually guaranteed and may be
    /// changed between releases without notice.
    pub fn new(config: Config) -> Self {
        let seed = config.rng_seed;
        let algorithm = config.rng_algorithm;
        TestRunner::new_with_rng(config, TestRng::default_rng(seed, algorithm))
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
    pub fn deterministic() -> Self {
        let config = Config::default();
        let algorithm = config.rng_algorithm;
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(algorithm))
    }

    /// Create a fresh `TestRunner` with the given configuration and RNG.
    pub fn new_with_rng(config: Config, rng: TestRng) -> Self {
        TestRunner {
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
        TestRunner {
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
    pub fn rng(&mut self) -> &mut TestRng {
        &mut self.rng
    }

    /// Create a new, independent but deterministic RNG from the RNG in this
    /// runner.
    pub fn new_rng(&mut self) -> TestRng {
        self.rng.gen_rng()
    }

    /// Returns the configuration of this runner.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Dumps the bytes obtained from the RNG so far (only works if the RNG is
    /// set to `Recorder`).
    ///
    /// ## Panics
    ///
    /// Panics if the RNG does not capture generated data.
    pub fn bytes_used(&self) -> Vec<u8> {
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

    #[cfg(not(feature = "fork"))]
    fn run_in_fork<S: Strategy>(
        &mut self,
        _: &S,
        _: impl Fn(S::Value) -> TestCaseResult,
    ) -> TestRunResult<S> {
        unreachable!()
    }

    #[cfg(feature = "fork")]
    fn run_in_fork<S: Strategy>(
        &mut self,
        strategy: &S,
        test_fn: impl Fn(S::Value) -> TestCaseResult,
    ) -> TestRunResult<S> {
        let mut test_fn = Some(test_fn);

        let test_name = rusty_fork::fork_test::fix_module_path(
            self.config
                .test_name
                .expect("Must supply test_name when forking enabled"),
        );
        let seed = self.rng.new_rng_seed();
        let mut replay = replay::Replay {
            seed,
            steps: vec![],
        };
        let mut child_count = 0;
        let timeout = self.config.timeout();

        fn forkfile_size(forkfile: &tempfile::NamedTempFile) -> u64 {
            forkfile
                .as_file()
                .metadata()
                .map(|md| md.len())
                .unwrap_or(0)
        }

        // One shared forkfile is created up front and reused by every child
        // spawn, so replay progress accumulates across child processes. A
        // creation or initialisation failure aborts the run instead of
        // panicking.
        let mut forkfile = tempfile::NamedTempFile::new().map_err(|error| {
            TestError::Abort(
                format!("Failed to create temporary file for fork: {}", error)
                    .into(),
            )
        })?;
        replay.init_file(&mut forkfile).map_err(|error| {
            TestError::Abort(
                format!(
                    "Failed to initialise temporary file for fork: {}",
                    error
                )
                .into(),
            )
        })?;
        // The path never changes across spawns; owning a copy keeps the
        // command-setup closure free of any borrow of the forkfile itself.
        let forkfile_path = forkfile.path().to_path_buf();

        loop {
            let pre_spawn_forkfile_size = forkfile_size(&forkfile);
            let (child_error, last_fork_file_len) = rusty_fork::fork(
                test_name,
                rusty_fork_id!(),
                |cmd| {
                    cmd.env(ENV_FORK_FILE, &forkfile_path);
                },
                |child, _| await_child(child, &mut forkfile, timeout),
                || match self.run_in_process(strategy, test_fn.take().unwrap())
                {
                    Ok(_) => (),
                    Err(e) => panic!(
                        "Test failed normally in child process.\n{}\n{}",
                        e, self
                    ),
                },
            )
            .expect("Fork failed");

            let parsed = replay::Replay::parse_from(&mut forkfile)
                .expect("Failed to re-read fork file");
            match parsed {
                replay::ReplayFileStatus::InProgress(new_replay) => {
                    replay = new_replay
                }
                replay::ReplayFileStatus::Terminated(new_replay) => {
                    replay = new_replay;
                    break;
                }
                replay::ReplayFileStatus::Corrupt => {
                    panic!("Child process corrupted replay file")
                }
            }

            let curr_forkfile_size = forkfile_size(&forkfile);

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
            if last_fork_file_len.is_none_or(|last_fork_file_len| {
                last_fork_file_len == curr_forkfile_size
            }) {
                let error = Err(child_error.unwrap_or(TestCaseError::fail(
                    "Child process was terminated abruptly \
                     but with successful status",
                )));
                replay::append(&mut forkfile, &error)
                    .expect("Failed to append to replay file");
                replay.steps.push(error);
            }

            // Bail if we've gone through too many processes in case the
            // shrinking process itself is crashing.
            child_count += 1;
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
            |_| panic!("Ran past the end of the replay"),
            replay.steps.into_iter(),
            ForkOutput::empty(),
        )
    }

    fn run_in_process<S: Strategy>(
        &mut self,
        strategy: &S,
        test_fn: impl Fn(S::Value) -> TestCaseResult,
    ) -> TestRunResult<S> {
        let (replay_steps, fork_output) = init_replay(&mut self.rng);
        self.run_in_process_with_replay(
            strategy,
            test_fn,
            replay_steps.into_iter(),
            fork_output,
        )
    }

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

            if let Err(e) = result {
                fork_output.terminate();
                return Err(e);
            }
        }

        fork_output.terminate();
        Ok(())
    }

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
                self.successes += 1
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
            Err(TestCaseError::Fail(why)) => {
                let why = self
                    .shrink(
                        &mut case,
                        test_fn,
                        replay_from_fork,
                        result_cache,
                        fork_output,
                        is_from_persisted_seed,
                    )
                    .unwrap_or(why);
                Err(TestError::Fail(why, case.current()))
            }
            Err(TestCaseError::Reject(whence)) => {
                self.reject_global(whence)?;
                Ok(TestCaseOk::Reject)
            }
        }
    }

    fn shrink<V: ValueTree>(
        &mut self,
        case: &mut V,
        test_fn: impl Fn(V::Value) -> TestCaseResult,
        replay_from_fork: &mut impl Iterator<Item = TestCaseResult>,
        result_cache: &mut dyn ResultCache,
        fork_output: &mut ForkOutput,
        is_from_persisted_seed: bool,
    ) -> Option<Reason> {
        // exit early if shrink disabled
        if self.config.max_shrink_iters == 0 {
            verbose_message!(
                self,
                INFO_LOG,
                "Shrinking disabled by configuration"
            );
            return None;
        }

        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        let start_time = std::time::Instant::now();
        let mut last_failure = None;
        let mut iterations = 0;

        verbose_message!(self, TRACE, "Starting shrinking");

        if !case.simplify() {
            return last_failure;
        }

        loop {
            #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
            let timed_out: Option<u64> = self.shrink_time_exceeded(start_time);
            #[cfg(not(all(feature = "std", not(target_arch = "wasm32"))))]
            let timed_out: Option<u64> = None;

            if self.shrink_budget_exhausted(iterations, timed_out) {
                self.backtrack_to_last_failure(case, fork_output);
                break;
            }

            iterations += 1;

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

        last_failure
    }

    /// Spend the rest of the shrink walk backtracking to the most recent
    /// failing case once a shrink budget is exhausted.
    fn backtrack_to_last_failure<V: ValueTree>(
        &self,
        case: &mut V,
        fork_output: &mut ForkOutput,
    ) {
        // Move back to the most recent failing case
        while case.complicate() {
            fork_output.append(&Ok(()));
        }
    }

    /// How many milliseconds the shrink phase has been running past the
    /// `max_shrink_time` budget, or `None` while still inside it (or when
    /// the budget is unlimited).
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    fn shrink_time_exceeded(
        &self,
        start_time: std::time::Instant,
    ) -> Option<u64> {
        if self.config.max_shrink_time == 0 {
            return None;
        }
        let elapsed = start_time.elapsed();
        let elapsed_ms = elapsed
            .as_secs()
            .saturating_mul(1000)
            .saturating_add(u64::from(elapsed.subsec_millis()));
        (elapsed_ms > self.config.max_shrink_time as u64).then_some(elapsed_ms)
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
            #[cfg(feature = "std")]
            const CONTROLLER: &str = "the PROPTEST_MAX_SHRINK_ITERS environment \
                 variable or ProptestConfig.max_shrink_iters";
            #[cfg(not(feature = "std"))]
            const CONTROLLER: &str = "ProptestConfig.max_shrink_iters";
            verbose_message!(
                self,
                ALWAYS,
                "Aborting shrinking after {} iterations (set {} \
                 to a large(r) value to shrink more; current \
                 configuration: {} iterations)",
                CONTROLLER,
                self.config.max_shrink_iters(),
                iterations
            );
            return true;
        }

        let Some(ms) = timed_out else {
            return false;
        };
        #[cfg(feature = "std")]
        const CONTROLLER: &str = "the PROPTEST_MAX_SHRINK_TIME environment \
                 variable or ProptestConfig.max_shrink_time";
        #[cfg(feature = "std")]
        let current = self.config.max_shrink_time;
        #[cfg(not(feature = "std"))]
        const CONTROLLER: &str = "(not configurable in no_std)";
        #[cfg(not(feature = "std"))]
        let current = 0;
        verbose_message!(
            self,
            ALWAYS,
            "Aborting shrinking after taking too long: {} ms \
             (set {} to a large(r) value to shrink more; current \
             configuration: {} ms)",
            ms,
            CONTROLLER,
            current
        );
        true
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
    pub fn reject_local(
        &mut self,
        whence: impl Into<Reason>,
    ) -> Result<(), Reason> {
        if self.local_rejects >= self.config.max_local_rejects {
            Err("Too many local rejects".into())
        } else {
            self.local_rejects += 1;
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
            self.global_rejects += 1;
            Self::insert_or_increment(&mut self.global_reject_detail, whence);
            Ok(())
        }
    }

    /// Insert 1 or increment the rejection detail at key for whence.
    fn insert_or_increment(into: &mut RejectionDetail, whence: Reason) {
        into.entry(whence)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

    /// Increment the counter of flat map regenerations and return whether it
    /// is still under the configured limit.
    pub fn flat_map_regen(&self) -> bool {
        self.flat_map_regens.fetch_add(1, SeqCst)
            < self.config.max_flat_map_regens as usize
    }

    fn new_cache(&self) -> Box<dyn ResultCache> {
        (self.config.result_cache)()
    }
}

#[cfg(feature = "fork")]
fn init_replay(rng: &mut TestRng) -> (Vec<TestCaseResult>, ForkOutput) {
    use crate::test_runner::replay::{Replay, ReplayFileStatus::*, open_file};

    if let Some(path) = env::var_os(ENV_FORK_FILE) {
        let mut file = open_file(&path).expect("Failed to open replay file");
        let loaded =
            Replay::parse_from(&mut file).expect("Failed to read replay file");
        match loaded {
            InProgress(replay) => {
                rng.set_seed(replay.seed);
                (replay.steps, ForkOutput { file: Some(file) })
            }

            Terminated(_) => {
                panic!("Replay file for child process is terminated?")
            }

            Corrupt => panic!("Replay file for child process is corrupt"),
        }
    } else {
        (vec![], ForkOutput::empty())
    }
}

#[cfg(not(feature = "fork"))]
fn init_replay(
    _rng: &mut TestRng,
) -> (iter::Empty<TestCaseResult>, ForkOutput) {
    (iter::empty(), ForkOutput::empty())
}

#[cfg(feature = "fork")]
fn await_child_without_timeout(
    child: &mut rusty_fork::ChildWrapper,
) -> (Option<TestCaseError>, Option<u64>) {
    let status = child.wait().expect("Failed to wait for child process");

    if status.success() {
        (None, None)
    } else {
        (
            Some(TestCaseError::fail(format!(
                "Child process exited with {}",
                status
            ))),
            None,
        )
    }
}

#[cfg(all(feature = "fork", not(feature = "timeout")))]
fn await_child(
    child: &mut rusty_fork::ChildWrapper,
    _: &mut tempfile::NamedTempFile,
    _timeout: u32,
) -> (Option<TestCaseError>, Option<u64>) {
    await_child_without_timeout(child)
}

#[cfg(all(feature = "fork", feature = "timeout"))]
fn await_child(
    child: &mut rusty_fork::ChildWrapper,
    forkfile: &mut tempfile::NamedTempFile,
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
    let mut last_forkfile_len = forkfile
        .as_file()
        .metadata()
        .map(|md| md.len())
        .unwrap_or(0);

    loop {
        if let Some(status) = child
            .wait_timeout(Duration::from_millis(timeout.into()))
            .expect("Failed to wait for child process")
        {
            if status.success() {
                return (None, None);
            } else {
                return (
                    Some(TestCaseError::fail(format!(
                        "Child process exited with {}",
                        status
                    ))),
                    None,
                );
            }
        }

        let current_len = forkfile
            .as_file()
            .metadata()
            .map(|md| md.len())
            .unwrap_or(0);
        // If we've gone a full timeout period without the file growing,
        // fail the test and kill the child.
        if current_len <= last_forkfile_len {
            return (
                Some(TestCaseError::fail(
                    "Timed out waiting for child process",
                )),
                Some(current_len),
            );
        } else {
            last_forkfile_len = current_len;
        }
    }
}

#[cfg(test)]
mod test {
    use std::cell::Cell;
    use std::fs;

    use super::*;
    use crate::strategy::Strategy;
    use crate::test_runner::{FileFailurePersistence, RngAlgorithm, TestRng};
    use strict_test_support::{
        TestFailure, ensure, ensure_eq, ensure_ok, ensure_some,
    };

    #[test]
    fn gives_up_after_too_many_rejections() -> Result<(), TestFailure> {
        let config = Config::default();
        let mut runner = TestRunner::new(config.clone());
        let runs = Cell::new(0);
        let result = runner.run(&(0u32..), |_| {
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
        let mut runner = TestRunner::default();
        let result = runner.run(&(1u32..), |v| {
            if v > 0 {
                Ok(())
            } else {
                Err(TestCaseError::fail("generated value must be positive"))
            }
        });
        ensure(result == Ok(()), "a passing property returns Ok")
    }

    #[test]
    fn test_fail_via_result() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            failure_persistence: None,
            ..Config::default()
        });
        let result = runner.run(&(0u32..10u32), |v| {
            if v < 5 {
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

    // Legacy-surface test: the panic inside the closure IS the subject —
    // it proves the runner converts a panicking case into TestError::Fail.
    #[test]
    fn test_fail_via_panic() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            failure_persistence: None,
            ..Config::default()
        });
        let result = runner.run(&(0u32..10u32), |v| {
            assert!(v < 5, "not less than 5");
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
            let _ = fs::remove_file(self.0);
        }
    }

    #[test]
    fn persisted_cases_do_not_count_towards_total_cases()
    -> Result<(), TestFailure> {
        const FILE: &str = "persistence-test-counting.txt";
        let _guard = PersistenceFileGuard(FILE);
        let _ = fs::remove_file(FILE);

        let config = Config {
            failure_persistence: Some(Box::new(
                FileFailurePersistence::Direct(FILE),
            )),
            cases: 1,
            ..Config::default()
        };

        let max = 10_000_000i32;
        ensure(
            TestRunner::new(config.clone())
                .run(&(0i32..max), |_v| {
                    Err(TestCaseError::Fail("persist a failure".into()))
                })
                .is_err(),
            "the seeding run must fail so a seed is persisted",
        )?;

        let run_count = Cell::new(0);
        ensure_ok(
            TestRunner::new(config.clone()).run(&(0i32..max), |_v| {
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
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "\r\n{:?}\r\n", self.0)
        }
    }

    #[test]
    fn failing_cases_persisted_and_reloaded() -> Result<(), TestFailure> {
        const FILE: &str = "persistence-test-reload.txt";
        let _guard = PersistenceFileGuard(FILE);
        let _ = fs::remove_file(FILE);

        let max = 10_000_000i32;
        let input = (0i32..max).prop_map(PoorlyBehavedDebug);
        let config = Config {
            failure_persistence: Some(Box::new(
                FileFailurePersistence::Direct(FILE),
            )),
            ..Config::default()
        };

        // First test with cases that fail above half max, and then below half
        // max, to ensure we can correctly parse both lines of the persistence
        // file.
        let first_sub_failure = ensure_some(
            TestRunner::new(config.clone())
                .run(&input, |v| {
                    if v.0 < max / 2 {
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
                .run(&input, |v| {
                    if v.0 >= max / 2 {
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
                .run(&input, |v| {
                    if v.0 < max / 2 {
                        Ok(())
                    } else {
                        Err(TestCaseError::Fail("too big".into()))
                    }
                })
                .err(),
            "the second sub-max run must fail",
        )?;
        let second_super_failure = ensure_some(
            TestRunner::new(config.clone())
                .run(&input, |v| {
                    if v.0 >= max / 2 {
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
        use rand::RngExt;
        let mut runner = TestRunner::default();
        let from_1 = runner.new_rng().random::<[u8; 16]>();
        let from_2 = runner.rng().random::<[u8; 16]>();
        ensure(from_1 != from_2, "a new rng draws a different stream")
    }

    #[test]
    fn record_rng_use() -> Result<(), TestFailure> {
        use rand::RngExt;

        // create value with recorder rng
        let default_config = Config::default();
        let recorder_rng =
            TestRng::default_rng(RngSeed::Random, RngAlgorithm::Recorder);
        let mut runner =
            TestRunner::new_with_rng(default_config.clone(), recorder_rng);
        let random_byte_array1 = runner.rng().random::<[u8; 16]>();
        let bytes_used = runner.bytes_used();
        // could use more bytes for some reason
        ensure(
            bytes_used.len() >= 16,
            "the recorder captured at least the drawn bytes",
        )?;

        // re-create value with pass-through rng
        let passthrough_rng =
            TestRng::from_seed(RngAlgorithm::PassThrough, &bytes_used);
        let mut runner =
            TestRunner::new_with_rng(default_config, passthrough_rng);
        let random_byte_array2 = runner.rng().random::<[u8; 16]>();

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
            ..Config::default()
        });

        ensure(
            runner.run(&(0u32..1000), |_| Ok(())).is_ok(),
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
            ..Config::default()
        });

        let failure = ensure_some(
            runner
                .run(&(0u32..1000), |v| {
                    if v < 500 {
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

    // Legacy-surface test: the child calling process::exit(1) IS the
    // subject — it proves a crashing child is synthesized into a failure.
    #[cfg(feature = "fork")]
    #[test]
    fn nonsuccessful_exit_finds_correct_failure() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            test_name: Some(concat!(
                module_path!(),
                "::nonsuccessful_exit_finds_correct_failure"
            )),
            ..Config::default()
        });

        let failure = ensure_some(
            runner
                .run(&(0u32..1000), |v| {
                    if v >= 500 {
                        ::std::process::exit(1);
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

    // Legacy-surface test: the child calling process::exit(0) IS the
    // subject — it proves a spuriously-succeeding child is caught.
    #[cfg(feature = "fork")]
    #[test]
    fn spurious_exit_finds_correct_failure() -> Result<(), TestFailure> {
        let mut runner = TestRunner::new(Config {
            fork: true,
            test_name: Some(concat!(
                module_path!(),
                "::spurious_exit_finds_correct_failure"
            )),
            ..Config::default()
        });

        let failure = ensure_some(
            runner
                .run(&(0u32..1000), |v| {
                    if v >= 500 {
                        ::std::process::exit(0);
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
            ..Config::default()
        });

        let failure = ensure_some(
            runner
                .run(&(0u32..1000), |v| {
                    if v >= 500 {
                        ::std::thread::sleep(
                            ::std::time::Duration::from_millis(10_000),
                        );
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
            ..Config::default()
        });

        let failure = ensure_some(
            runner
                .run(&(0u32..1000), |v| {
                    if v >= 500 {
                        // Sleep a little longer than the timeout. This means that
                        // sometimes the test case itself will return before the parent
                        // process has noticed the child is timing out, so it's up to
                        // the child to mark it as a failure.
                        ::std::thread::sleep(
                            ::std::time::Duration::from_millis(600),
                        );
                    } else {
                        // Sleep a bit so that the parent and child timing don't stay
                        // in sync.
                        ::std::thread::sleep(
                            ::std::time::Duration::from_millis(100),
                        )
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

        for _ in 0..256 {
            let mut runner = TestRunner::new(Config {
                failure_persistence: None,
                result_cache:
                    crate::test_runner::result_cache::basic_result_cache,
                ..Config::default()
            });
            let pass = Rc::new(Cell::new(true));
            let seen = Rc::new(RefCell::new(HashSet::new()));
            let result =
                runner.run(&(0u32..65536u32).prop_map(|v| v % 10), |val| {
                    if !seen.borrow_mut().insert(val) {
                        pass.set(false);
                    }

                    if val <= 5 {
                        Ok(())
                    } else {
                        Err(TestCaseError::fail("value above 5"))
                    }
                });

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
#[cfg(all(feature = "fork", feature = "timeout", test))]
mod timeout_tests {
    use std::thread;
    use std::time::Duration;

    use super::*;

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 4_000)]

        #[test]
        fn max_shrink_iters_works() {
            test_shrink_bail(Config {
                max_shrink_iters: 5,
                .. Config::default()
            });
        }

        #[test]
        fn max_shrink_time_works() {
            test_shrink_bail(Config {
                max_shrink_time: 1000,
                .. Config::default()
            });
        }

        #[test]
        fn max_shrink_iters_works_with_forking() {
            test_shrink_bail(Config {
                fork: true,
                test_name: Some(
                    concat!(module_path!(),
                            "::max_shrink_iters_works_with_forking")),
                max_shrink_time: 1000,
                .. Config::default()
            });
        }

        #[test]
        fn detects_child_failure_to_start() {
            let mut runner = TestRunner::new(Config {
                timeout: 100,
                test_name: Some(
                    concat!(module_path!(),
                            "::detects_child_failure_to_start")),
                .. Config::default()
            });
            let result = runner.run(&Just(()).prop_map(|()| {
                thread::sleep(Duration::from_millis(200))
            }), Ok);

            if let Err(TestError::Abort(_)) = result {
                // OK
            } else {
                panic!("Unexpected result: {:?}", result);
            }
        }
    }

    fn test_shrink_bail(config: Config) {
        let mut runner = TestRunner::new(config);
        let result = runner.run(&crate::num::u64::ANY, |v| {
            thread::sleep(Duration::from_millis(250));
            if v <= u32::MAX as u64 {
                Ok(())
            } else {
                Err(TestCaseError::fail("value exceeds u32::MAX"))
            }
        });

        if let Err(TestError::Fail(_, value)) = result {
            // Ensure the final value was in fact a failing case.
            assert!(value > u32::MAX as u64);
        } else {
            panic!("Unexpected result: {:?}", result);
        }
    }
}
