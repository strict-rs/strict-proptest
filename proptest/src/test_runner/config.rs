//-
// Copyright 2017, 2018, 2019 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::fmt;
use core::num::ParseIntError;
use core::ptr::fn_addr_eq;
use core::str;
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
use std::ffi::OsString;
#[cfg(feature = "std")]
use std::sync::LazyLock;

use crate::std_facade::Box;
use crate::test_runner::FailurePersistence;
#[cfg(feature = "std")]
use crate::test_runner::FileFailurePersistence;
use crate::test_runner::result_cache::ResultCache;
use crate::test_runner::result_cache::noop_result_cache;
use crate::test_runner::rng::RngAlgorithm;

/// Environment override for the number of successful cases to run.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const CASES: &str = "PROPTEST_CASES";
/// Environment override for the local rejection budget.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const MAX_LOCAL_REJECTS: &str = "PROPTEST_MAX_LOCAL_REJECTS";
/// Environment override for the global rejection budget.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const MAX_GLOBAL_REJECTS: &str = "PROPTEST_MAX_GLOBAL_REJECTS";
/// Environment override for flat-map regeneration budget.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const MAX_FLAT_MAP_REGENS: &str = "PROPTEST_MAX_FLAT_MAP_REGENS";
/// Environment override for shrink-time budget.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const MAX_SHRINK_TIME: &str = "PROPTEST_MAX_SHRINK_TIME";
/// Environment override for shrink-iteration budget.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const MAX_SHRINK_ITERS: &str = "PROPTEST_MAX_SHRINK_ITERS";
/// Environment override for the default collection size range.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const MAX_DEFAULT_SIZE_RANGE: &str = "PROPTEST_MAX_DEFAULT_SIZE_RANGE";
/// Environment override for fork isolation.
#[cfg(all(feature = "std", not(target_arch = "wasm32"), feature = "fork"))]
const FORK: &str = "PROPTEST_FORK";
/// Environment override for per-case timeout.
#[cfg(all(feature = "std", not(target_arch = "wasm32"), feature = "timeout"))]
const TIMEOUT: &str = "PROPTEST_TIMEOUT";
/// Environment override for runner verbosity.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const VERBOSE: &str = "PROPTEST_VERBOSE";
/// Environment override for the RNG algorithm.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const RNG_ALGORITHM: &str = "PROPTEST_RNG_ALGORITHM";
/// Environment override for the RNG seed.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const RNG_SEED: &str = "PROPTEST_RNG_SEED";
/// Environment override disabling failure persistence.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const DISABLE_FAILURE_PERSISTENCE: &str = "PROPTEST_DISABLE_FAILURE_PERSISTENCE";

/// Override the config fields from environment variables, if any are set.
/// Without the `std` feature this function returns config unchanged.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[must_use = "the environment overlay returns the updated Config"]
#[allow(
  clippy::single_call_fn,
  reason = "publicly apply the process PROPTEST_* overlay to a Config before runner construction"
)]
pub fn contextualize_config(mut result: Config) -> Config {
  use std::env;

  for (env_var_name, raw_value) in
    env::vars_os().filter_map(|(name, os_value)| name.into_string().ok().map(|env_name| (env_name, os_value)))
  {
    apply_env_var(env_var_name.as_str(), &raw_value, &mut result);
  }

  result
}

/// Parse a typed config value from an environment variable, reporting a
/// structured runner diagnostic when parsing fails.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
fn parse_or_warn<T: str::FromStr + fmt::Display>(raw_value: &OsString, dst: &mut T, typ: &'static str, var: &'static str) {
  use std::borrow::ToOwned as _;
  use std::string::ToString as _;

  use crate::test_runner::diagnostics::RunnerDiagnostic;
  use crate::test_runner::diagnostics::{
    self,
  };

  if let Some(source_text) = raw_value.to_str() {
    if let Ok(parsed) = source_text.parse() {
      *dst = parsed;
    } else {
      diagnostics::emit(&RunnerDiagnostic::EnvVarUnparsable {
        var,
        value: source_text.to_owned(),
        typ,
        default: dst.to_string(),
      });
    }
  } else {
    diagnostics::emit(&RunnerDiagnostic::EnvVarNotUnicode {
      var,
      default: dst.to_string(),
    });
  }
}

/// Apply one known `PROPTEST_*` environment variable to `result`.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[allow(
  clippy::single_call_fn,
  reason = "centralize the recognized PROPTEST_* env-var decision table and its feature-owned branches"
)]
fn apply_env_var(env_var: &str, raw_value: &OsString, result: &mut Config) {
  match env_var {
    #[cfg(feature = "fork")]
    FORK => parse_or_warn(raw_value, &mut result.fork, "bool", FORK),
    #[cfg(feature = "timeout")]
    TIMEOUT => {
      parse_or_warn(raw_value, &mut result.timeout, "timeout", TIMEOUT);
    }
    CASES => parse_or_warn(raw_value, &mut result.cases, "u32", CASES),
    MAX_LOCAL_REJECTS => parse_or_warn(raw_value, &mut result.max_local_rejects, "u32", MAX_LOCAL_REJECTS),
    MAX_GLOBAL_REJECTS => parse_or_warn(raw_value, &mut result.max_global_rejects, "u32", MAX_GLOBAL_REJECTS),
    MAX_FLAT_MAP_REGENS => parse_or_warn(raw_value, &mut result.max_flat_map_regens, "u32", MAX_FLAT_MAP_REGENS),
    MAX_SHRINK_TIME => parse_or_warn(raw_value, &mut result.max_shrink_time, "u32", MAX_SHRINK_TIME),
    MAX_SHRINK_ITERS => parse_or_warn(raw_value, &mut result.max_shrink_iters, "u32", MAX_SHRINK_ITERS),
    MAX_DEFAULT_SIZE_RANGE => parse_or_warn(raw_value, &mut result.max_default_size_range, "usize", MAX_DEFAULT_SIZE_RANGE),
    VERBOSE => {
      parse_or_warn(raw_value, &mut result.verbose, "u32", VERBOSE);
    }
    RNG_ALGORITHM => parse_or_warn(raw_value, &mut result.rng_algorithm, "RngAlgorithm", RNG_ALGORITHM),
    RNG_SEED => {
      parse_or_warn(raw_value, &mut result.rng_seed, "u64", RNG_SEED);
    }
    DISABLE_FAILURE_PERSISTENCE => result.failure_persistence = None,
    unknown if unknown.starts_with("PROPTEST_") => {
      use std::borrow::ToOwned as _;

      use crate::test_runner::diagnostics::RunnerDiagnostic;
      use crate::test_runner::diagnostics::{
        self,
      };

      diagnostics::emit(&RunnerDiagnostic::EnvVarUnknown {
        var: unknown.to_owned()
      });
    }
    _ => {}
  }
}

/// Without the `std` feature this function returns config unchanged.
#[cfg(not(all(feature = "std", not(target_arch = "wasm32"))))]
#[must_use = "the environment overlay returns the updated Config"]
#[allow(
  clippy::single_call_fn,
  reason = "preserve the public Config environment-overlay API as a no-op without std env access"
)]
pub fn contextualize_config(result: Config) -> Config {
  result
}

/// The compiled-in default `Config`, before any environment overlay.
///
/// This is the persistence-free baseline shared by both `Default` impls:
/// under `std`, `DEFAULT_CONFIG` wraps it to add the default
/// `FileFailurePersistence` and the `PROPTEST_*` overlay; without `std`
/// it is returned verbatim (so persistence stays `None`).
#[allow(
  clippy::single_call_fn,
  reason = "the persistence-free compiled-in Config baseline shared by std and no_std defaults"
)]
fn default_default_config() -> Config {
  Config {
    cases: 256,
    max_local_rejects: 65_536,
    max_global_rejects: 1024,
    max_flat_map_regens: 1_000_000,
    failure_persistence: None,
    source_file: None,
    test_name: None,
    #[cfg(feature = "fork")]
    fork: false,
    #[cfg(feature = "timeout")]
    timeout: 0,
    #[cfg(feature = "std")]
    max_shrink_time: 0,
    max_shrink_iters: u32::MAX,
    max_default_size_range: 100,
    result_cache: noop_result_cache,
    #[cfg(feature = "std")]
    verbose: 0,
    rng_algorithm: RngAlgorithm::default(),
    rng_seed: RngSeed::Random,
    _non_exhaustive: (),
  }
}

/// The process-wide default `Config`, built once on first use.
///
/// Starts from `default_default_config()`, switches `failure_persistence`
/// on to the default `FileFailurePersistence`, and applies the
/// `PROPTEST_*` env overlay exactly once; `Config::default` (under `std`)
/// hands out clones of this.
#[cfg(feature = "std")]
static DEFAULT_CONFIG: LazyLock<Config> = LazyLock::new(|| {
  let mut default_config = default_default_config();
  default_config.failure_persistence = Some(Box::new(FileFailurePersistence::default()));
  contextualize_config(default_config)
});

/// The seed for the RNG, can either be random or specified as a u64.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RngSeed {
  /// Default case, use a random value
  Random,
  /// Use a specific value to generate a seed
  Fixed(u64),
}

impl str::FromStr for RngSeed {
  type Err = ParseIntError;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    s.parse::<u64>().map(RngSeed::Fixed)
  }
}

impl fmt::Display for RngSeed {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match *self {
      Self::Random => write!(f, "random"),
      Self::Fixed(n) => write!(f, "{n}"),
    }
  }
}

/// Configuration for how a proptest test should be run.
#[derive(Clone, Debug)]
pub struct Config {
  /// The number of successful test cases that must execute for the test as a
  /// whole to pass.
  ///
  /// This does not include implicitly-replayed persisted failing cases.
  ///
  /// The default is 256, which can be overridden by setting the
  /// `PROPTEST_CASES` environment variable. (The variable is only considered
  /// when the `std` feature is enabled, which it is by default.)
  pub cases: u32,

  /// The maximum number of individual inputs that may be rejected before the
  /// test as a whole aborts.
  ///
  /// The default is 65536, which can be overridden by setting the
  /// `PROPTEST_MAX_LOCAL_REJECTS` environment variable. (The variable is only
  /// considered when the `std` feature is enabled, which it is by default.)
  pub max_local_rejects: u32,

  /// The maximum number of combined inputs that may be rejected before the
  /// test as a whole aborts.
  ///
  /// The default is 1024, which can be overridden by setting the
  /// `PROPTEST_MAX_GLOBAL_REJECTS` environment variable. (The variable is
  /// only considered when the `std` feature is enabled, which it is by
  /// default.)
  pub max_global_rejects: u32,

  /// The maximum number of times all `Flatten` combinators will attempt to
  /// regenerate values. This puts a limit on the worst-case exponential
  /// explosion that can happen with nested `Flatten`s.
  ///
  /// The default is `1_000_000`, which can be overridden by setting the
  /// `PROPTEST_MAX_FLAT_MAP_REGENS` environment variable. (The variable is
  /// only considered when the `std` feature is enabled, which it is by
  /// default.)
  pub max_flat_map_regens: u32,

  /// Indicates whether and how to persist failed test results.
  ///
  /// When compiling with "std" feature (i.e. the standard library is available), the default
  /// is `Some(Box::new(FileFailurePersistence::SourceParallel("proptest-regressions")))`.
  ///
  /// Without the standard library, the default is `None`, and no persistence occurs.
  ///
  /// See the docs of [`FileFailurePersistence`](enum.FileFailurePersistence.html)
  /// and [`MapFailurePersistence`](struct.MapFailurePersistence.html) for more information.
  ///
  /// You can disable failure persistence with the `PROPTEST_DISABLE_FAILURE_PERSISTENCE`
  /// environment variable but its not currently possible to set the persistence file
  /// with an environment variable. (The variable is
  /// only considered when the `std` feature is enabled, which it is by
  /// default.)
  pub failure_persistence: Option<Box<dyn FailurePersistence>>,

  /// File location of the current test, relevant for persistence
  /// and debugging.
  ///
  /// Note the use of `&str` rather than `Path` to be compatible with
  /// `#![no_std]` use cases where `Path` is unavailable.
  ///
  /// See the docs of [`FileFailurePersistence`](enum.FileFailurePersistence.html)
  /// for more information on how it may be used for persistence.
  pub source_file: Option<&'static str>,

  /// The fully-qualified name of the test being run, as would be passed to
  /// the test executable to run just that test.
  ///
  /// This must be set if `fork` is `true`. Otherwise, it is unused. It is
  /// automatically set by `proptest!`.
  ///
  /// This must include the crate name at the beginning, as produced by
  /// `module_path!()`.
  pub test_name: Option<&'static str>,

  /// If true, tests are run in a subprocess.
  ///
  /// Forking allows proptest to work with tests which may fail by aborting
  /// the process, causing a segmentation fault, etc, but can be a lot slower
  /// in certain environments or when running a very large number of tests.
  ///
  /// For forking to work correctly, both the `Strategy` and the content of
  /// the test case itself must be deterministic.
  ///
  /// This requires the "fork" feature, enabled by default.
  ///
  /// The default is `false`, which can be overridden by setting the
  /// `PROPTEST_FORK` environment variable. (The variable is
  /// only considered when the `std` feature is enabled, which it is by
  /// default.)
  #[cfg(feature = "fork")]
  #[cfg_attr(docsrs, doc(cfg(feature = "fork")))]
  pub fork: bool,

  /// If non-zero, tests are run in a subprocess and each generated case
  /// fails if it takes longer than this number of milliseconds.
  ///
  /// This implicitly enables forking, even if the `fork` field is `false`.
  ///
  /// The type here is plain `u32` (rather than
  /// `Option<std::time::Duration>`) for the sake of ergonomics.
  ///
  /// This requires the "timeout" feature, enabled by default.
  ///
  /// Setting a timeout to less than the time it takes the process to start
  /// up and initialise the first test case will cause the whole test to be
  /// aborted.
  ///
  /// The default is `0` (i.e., no timeout), which can be overridden by
  /// setting the `PROPTEST_TIMEOUT` environment variable. (The variable is
  /// only considered when the `std` feature is enabled, which it is by
  /// default.)
  #[cfg(feature = "timeout")]
  #[cfg_attr(docsrs, doc(cfg(feature = "timeout")))]
  pub timeout: u32,

  /// If non-zero, give up the shrinking process after this many milliseconds
  /// have elapsed since the start of the shrinking process.
  ///
  /// This will not cause currently running test cases to be interrupted.
  ///
  /// This configuration is only available when the `std` feature is enabled
  /// (which it is by default).
  ///
  /// The default is `0` (i.e., no limit), which can be overridden by setting
  /// the `PROPTEST_MAX_SHRINK_TIME` environment variable. (The variable is
  /// only considered when the `std` feature is enabled, which it is by
  /// default.)
  #[cfg(feature = "std")]
  #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
  pub max_shrink_time: u32,

  /// Give up on shrinking if more than this number of iterations of the test
  /// code are run.
  ///
  /// Setting this to `u32::MAX` causes the actual limit to be four
  /// times the number of test cases.
  ///
  /// Setting this value to `0` disables shrinking altogether.
  ///
  /// Note that the type of this field will change in a future version of
  /// proptest to better accommodate its special values.
  ///
  /// The default is `u32::MAX`, which can be overridden by setting the
  /// `PROPTEST_MAX_SHRINK_ITERS` environment variable. (The variable is only
  /// considered when the `std` feature is enabled, which it is by default.)
  pub max_shrink_iters: u32,

  /// The default maximum size to `proptest::collection::SizeRange`. The default
  /// strategy for collections (like `Vec`) use collections in the range of
  /// `0..max_default_size_range`.
  ///
  /// The default is `100` which can be overridden by setting the
  /// `PROPTEST_MAX_DEFAULT_SIZE_RANGE` environment variable. (The variable
  /// is only considered when the `std` feature is enabled, which it is by
  /// default.)
  pub max_default_size_range: usize,

  /// A function to create new result caches.
  ///
  /// The default is to do no caching. The easiest way to enable caching is
  /// to set this field to `basic_result_cache` (though that is currently
  /// only available with the `std` feature).
  ///
  /// This is useful for strategies which have a tendency to produce
  /// duplicate values, or for tests where shrinking can take a very long
  /// time due to exploring the same output multiple times.
  ///
  /// When caching is enabled, generated values themselves are not stored, so
  /// this does not pose a risk of memory exhaustion for large test inputs
  /// unless using extraordinarily large test case counts.
  ///
  /// Caching incurs its own overhead, and may very well make your test run
  /// more slowly.
  pub result_cache: fn() -> Box<dyn ResultCache>,

  /// Set to non-zero values to cause proptest to emit human-targeted
  /// messages to stderr as it runs.
  ///
  /// Greater values cause greater amounts of logs to be emitted. The exact
  /// meaning of certain levels other than 0 is subject to change.
  ///
  /// - 0: No extra output.
  /// - 1: Log test failure messages. In state machine tests, this level is used to print
  ///   transitions.
  /// - 2: Trace low-level details.
  ///
  /// This is only available with the `std` feature (enabled by default)
  /// since on nostd proptest has no way to produce output.
  ///
  /// The default is `0`, which can be overridden by setting the
  /// `PROPTEST_VERBOSE` environment variable. (The variable is only considered
  /// when the `std` feature is enabled, which it is by default.)
  #[cfg(feature = "std")]
  #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
  pub verbose: u32,

  /// The RNG algorithm to use when not using a user-provided RNG.
  ///
  /// The default is `RngAlgorithm::default()`, which can be overridden by
  /// setting the `PROPTEST_RNG_ALGORITHM` environment variable to one of the following:
  ///
  /// - `xs` — `RngAlgorithm::XorShift`
  /// - `cc` — `RngAlgorithm::ChaCha`
  ///
  /// (The variable is only considered when the `std` feature is enabled,
  /// which it is by default.)
  pub rng_algorithm: RngAlgorithm,

  /// Seed used for the RNG. Set by using the `PROPTEST_RNG_SEED` environment variable
  /// If the environment variable is undefined, a random seed is generated (this is the default
  /// option).
  pub rng_seed: RngSeed,

  // Needs to be public so FRU syntax can be used.
  #[doc(hidden)]
  pub _non_exhaustive: (),
}

/// Compare two result-cache factory function pointers by address.
///
/// `Config`'s `PartialEq` cannot compare the `result_cache` `fn` field
/// structurally, so it treats two configs as sharing a cache only when
/// both point at the very same factory (`core::ptr::fn_addr_eq`).
#[allow(
  clippy::single_call_fn,
  reason = "compare two result-cache factory function pointers by address for Config's PartialEq"
)]
fn result_cache_eq(left: fn() -> Box<dyn ResultCache>, right: fn() -> Box<dyn ResultCache>) -> bool {
  fn_addr_eq(left, right)
}

impl PartialEq for Config {
  fn eq(&self, other: &Self) -> bool {
    #[cfg(feature = "fork")]
    let fork_fields_eq = self.fork == other.fork;
    #[cfg(not(feature = "fork"))]
    let fork_fields_eq = true;

    #[cfg(feature = "timeout")]
    let timeout_fields_eq = self.timeout == other.timeout;
    #[cfg(not(feature = "timeout"))]
    let timeout_fields_eq = true;

    #[cfg(feature = "std")]
    let std_fields_eq = self.max_shrink_time == other.max_shrink_time && self.verbose == other.verbose;
    #[cfg(not(feature = "std"))]
    let std_fields_eq = true;

    let fields_eq = self.cases == other.cases
      && self.max_local_rejects == other.max_local_rejects
      && self.max_global_rejects == other.max_global_rejects
      && self.max_flat_map_regens == other.max_flat_map_regens
      && self.failure_persistence == other.failure_persistence
      && self.source_file == other.source_file
      && self.test_name == other.test_name
      && fork_fields_eq
      && timeout_fields_eq
      && std_fields_eq
      && self.max_shrink_iters == other.max_shrink_iters
      && self.max_default_size_range == other.max_default_size_range
      && result_cache_eq(self.result_cache, other.result_cache);

    fields_eq && self.rng_algorithm == other.rng_algorithm && self.rng_seed == other.rng_seed
  }
}

impl Config {
  /// Constructs a `Config` only differing from the `default()` in the
  /// number of test cases required to pass the test successfully.
  ///
  /// This is simply a more concise alternative to using field-record update
  /// syntax:
  ///
  /// ```
  /// # use proptest::test_runner::Config;
  /// assert_eq!(Config::with_cases(42), Config {
  ///   cases: 42,
  ///   ..Config::default()
  /// });
  /// ```
  #[allow(
    clippy::single_call_fn,
    reason = "a Config that differs from the default only in its configured case count"
  )]
  #[must_use]
  pub fn with_cases(cases: u32) -> Self {
    Self {
      cases,
      ..Self::default()
    }
  }

  /// Constructs a `Config` only differing from the `default()` in the
  /// `source_file` of the present test.
  ///
  /// This is simply a more concise alternative to using field-record update
  /// syntax:
  ///
  /// ```
  /// # use proptest::test_runner::Config;
  /// assert_eq!(Config::with_source_file("computer/question"), Config {
  ///   source_file: Some("computer/question"),
  ///   ..Config::default()
  /// });
  /// ```
  #[must_use]
  pub fn with_source_file(source_file: &'static str) -> Self {
    Self {
      source_file: Some(source_file),
      ..Self::default()
    }
  }

  /// Constructs a `Config` only differing from the provided `Config`
  /// instance, `self`, in the `source_file` of the present test.
  ///
  /// This is simply a more concise alternative to using field-record update
  /// syntax:
  ///
  /// ```
  /// # use proptest::test_runner::Config;
  /// let a = Config::with_source_file("computer/question");
  /// let b = a.clone_with_source_file("answer/42");
  /// assert_eq!(a, Config {
  ///   source_file: Some("computer/question"),
  ///   ..Config::default()
  /// });
  /// assert_eq!(b, Config {
  ///   source_file: Some("answer/42"),
  ///   ..Config::default()
  /// });
  /// ```
  #[must_use]
  pub fn clone_with_source_file(&self, source_file: &'static str) -> Self {
    let mut result = self.clone();
    result.source_file = Some(source_file);
    result
  }

  /// Constructs a `Config` only differing from the `default()` in the
  /// `failure_persistence` member.
  ///
  /// This is simply a more concise alternative to using field-record update
  /// syntax:
  ///
  /// ```
  /// # use proptest::test_runner::{Config, FileFailurePersistence};
  /// assert_eq!(
  ///   Config::with_failure_persistence(FileFailurePersistence::WithSource("regressions")),
  ///   Config {
  ///     failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
  ///     ..Config::default()
  ///   }
  /// );
  /// ```
  pub fn with_failure_persistence<T>(failure_persistence: T) -> Self
  where
    T: FailurePersistence + 'static,
  {
    Self {
      failure_persistence: Some(Box::new(failure_persistence)),
      ..Default::default()
    }
  }

  /// Return whether this configuration implies forking.
  ///
  /// This method exists even if the "fork" feature is disabled, in which
  /// case it simply returns false.
  #[must_use]
  pub const fn fork(&self) -> bool {
    self.raw_fork() || self.timeout() > 0
  }

  /// Backing accessor for `fork()`: the raw `fork` field, present only
  /// when the `fork` feature is enabled.
  #[cfg(feature = "fork")]
  const fn raw_fork(&self) -> bool {
    self.fork
  }

  /// Backing accessor for `fork()`: always `false` when the `fork`
  /// feature is disabled and there is no `fork` field.
  #[cfg(not(feature = "fork"))]
  const fn raw_fork(&self) -> bool {
    false
  }

  /// Returns the configured timeout.
  ///
  /// This method exists even if the "timeout" feature is disabled, in which
  /// case it simply returns 0.
  #[cfg(feature = "timeout")]
  #[must_use]
  pub const fn timeout(&self) -> u32 {
    self.timeout
  }

  /// Returns the configured timeout.
  ///
  /// This method exists even if the "timeout" feature is disabled, in which
  /// case it simply returns 0.
  #[cfg(not(feature = "timeout"))]
  #[must_use]
  pub const fn timeout(&self) -> u32 {
    0
  }

  /// Returns the configured limit on shrinking iterations.
  ///
  /// This takes into account the special "automatic" behaviour.
  #[must_use]
  pub const fn max_shrink_iters(&self) -> u32 {
    if u32::MAX == self.max_shrink_iters {
      self.cases.saturating_mul(4)
    } else {
      self.max_shrink_iters
    }
  }

  /// Hidden macro helper that clones a config expression without relying on
  /// `Clone` being imported at the call site.
  #[doc(hidden)]
  #[must_use]
  pub fn __sugar_to_owned(&self) -> Self {
    self.clone()
  }
}

#[cfg(feature = "std")]
impl Default for Config {
  fn default() -> Self {
    DEFAULT_CONFIG.clone()
  }
}

#[cfg(not(feature = "std"))]
impl Default for Config {
  fn default() -> Self {
    default_default_config()
  }
}

#[cfg(test)]
mod tests {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_all;

  use super::*;
  use crate::test_runner::errors::TestCaseResult;
  use crate::test_runner::result_cache::ResultCacheKey;

  #[test]
  fn config_partial_eq_default_equals_self_and_clone() -> Result<(), TestFailure> {
    let default = Config::default();

    ensure(default.eq(&default), "Config PartialEq is reflexive")?;
    ensure(default == default.clone(), "default equals its clone")
  }

  #[test]
  fn config_partial_eq_result_cache_factory_uses_explicit_helper() -> Result<(), TestFailure> {
    struct TestResultCache;

    impl ResultCache for TestResultCache {
      fn key(&self, _: &ResultCacheKey<'_>) -> u64 {
        1
      }

      fn put(&mut self, _: u64, _: &TestCaseResult) {}

      fn get(&self, _: u64) -> Option<&TestCaseResult> {
        None
      }
    }

    fn test_result_cache() -> Box<dyn ResultCache> {
      Box::new(TestResultCache)
    }

    let default = Config::default();
    let same_factory = Config {
      result_cache: default.result_cache,
      ..default.clone()
    };
    let different_factory = Config {
      result_cache: test_result_cache,
      ..default.clone()
    };

    ensure_all(&[
      (
        result_cache_eq(default.result_cache, same_factory.result_cache),
        "the same factory pointer compares equal",
      ),
      (default == same_factory, "configs sharing a factory are equal"),
      (
        !result_cache_eq(default.result_cache, different_factory.result_cache),
        "a different factory pointer compares unequal",
      ),
      (default != different_factory, "configs with different factories are unequal"),
    ])
  }
}
