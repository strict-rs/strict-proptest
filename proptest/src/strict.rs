//-
// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The native strict property runner: run a [`Strategy`] against a
//! `Result`-returning property and get the outcome back as a
//! [`TestResult`] instead of a panic.
//!
//! [`ensure_property`] drives [`TestRunner::run`] directly, so the property
//! closure returns `Result<(), TestFailure>` like any other strict test
//! body and no panicking macro is involved: a falsified property surfaces
//! as [`TestFailure::PropertyFalsified`] carrying the engine's rendering of
//! the shrunk minimal failing input, and a run that cannot complete
//! surfaces as [`TestFailure::PropertyAborted`].
//!
//! Strategies are the ordinary [`crate::strategy`] vocabulary: ranges,
//! [`crate::collection`], and the [`Strategy`] combinators (`prop_map`,
//! `prop_filter`, ...). A precondition belongs in the strategy as a
//! [`Strategy::prop_filter`] — unwanted inputs are then rejected at
//! generation, and an over-strict filter surfaces as
//! [`TestFailure::PropertyAborted`] instead of panicking the way
//! `prop_assume!` would.
//!
//! Failure persistence is disabled: a strict property run never writes a
//! `proptest-regressions/` file; pin a discovered counterexample as a named
//! unit test instead. Because that removes the usual replay anchor, the
//! runner is seeded deterministically by default — every run draws the same
//! inputs. The `STRICT_TEST_SEED` environment variable selects the seed:
//! unset (or unrecognized) pins a fixed deterministic seed, `random` opts
//! into OS entropy for an exploratory run, and a bare integer pins that
//! exact seed to replay a discovered case. This supersedes
//! `PROPTEST_RNG_SEED`; the remaining `PROPTEST_*` knobs (case count,
//! shrink iterations, ...) still apply through [`Config::default`], and
//! [`ensure_property_with_config`] hands the runner a caller-built
//! [`Config`] verbatim.

use std::env;
use std::string::ToString as _;

use crate::strategy::Strategy;
use crate::test_runner::{
    Config, RngSeed, TestCaseError, TestError, TestRunner,
};

pub use strict_test_support::TestFailure;

/// The outcome of a strict test body: `Ok(())` when every expectation
/// holds, or the first [`TestFailure`] encountered.
pub type TestResult = Result<(), TestFailure>;

/// Environment variable selecting the strict property-runner RNG seed.
const SEED_ENV: &str = "STRICT_TEST_SEED";

/// Seed pinned when `STRICT_TEST_SEED` is unset or unrecognized, so strict
/// property runs replay the same inputs. With failure persistence disabled
/// this fixed seed is the reproducibility anchor; the exact value is
/// arbitrary — only its fixedness matters.
const DETERMINISTIC_SEED: u64 = 0x5EED;

/// Map a raw `STRICT_TEST_SEED` value onto an RNG seed: `random` opts into
/// OS entropy, a bare integer pins that exact seed, and unset or any
/// unrecognized value pins the fixed [`DETERMINISTIC_SEED`].
#[allow(
    clippy::single_call_fn,
    reason = "map the raw STRICT_TEST_SEED value onto random, fixed, or the default deterministic seed"
)]
fn resolve_seed(raw: Option<&str>) -> RngSeed {
    match raw {
        Some("random") => RngSeed::Random,
        Some(literal) => literal
            .parse::<RngSeed>()
            .unwrap_or(RngSeed::Fixed(DETERMINISTIC_SEED)),
        None => RngSeed::Fixed(DETERMINISTIC_SEED),
    }
}

/// The runner configuration used by [`ensure_property`].
///
/// Failure persistence is disabled (no `proptest-regressions/` files are
/// written) and the RNG seed is resolved from `STRICT_TEST_SEED`; everything
/// else comes from [`Config::default`], which honors the remaining
/// `PROPTEST_*` environment variables.
#[allow(
    clippy::single_call_fn,
    reason = "the persistence-off, STRICT_TEST_SEED-seeded Config that backs ensure_property"
)]
#[must_use]
pub fn strict_default_config() -> Config {
    Config {
        failure_persistence: None,
        rng_seed: resolve_seed(env::var(SEED_ENV).ok().as_deref()),
        ..Config::default()
    }
}

/// Run `property` against inputs generated from `strategy`, shrinking any
/// falsifying input to a minimal counterexample.
///
/// The closure returns [`TestResult`], so a property body composes the
/// same `ensure*` helpers as any other strict test and reads identically.
/// (The closure is the trailing parameter so multi-line properties stay
/// readable.)
///
/// The run uses [`strict_default_config`]: deterministically seeded by
/// default, so runs are reproducible; set `STRICT_TEST_SEED=random` for an
/// exploratory run, or `STRICT_TEST_SEED=<integer>` to replay a specific
/// seed. No `proptest-regressions/` file is ever written.
///
/// # Errors
///
/// Returns [`TestFailure::PropertyFalsified`] when an input falsifies the
/// property (the report carries the engine's rendering of the shrunk
/// minimal failing input), or [`TestFailure::PropertyAborted`] when the
/// runner cannot complete a run (for example, a strategy filter rejects
/// too many inputs).
///
/// # Examples
///
/// ```
/// use proptest::strict::{ensure_property, TestFailure};
/// use strict_test_support::ensure;
///
/// # fn main() -> Result<(), TestFailure> {
/// ensure_property(
///     &(0_u32..10),
///     "generated samples stay below ten",
///     |sample| ensure(sample < 10, "sample below ten"),
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn ensure_property<S, F>(
    strategy: &S,
    context: &'static str,
    property: F,
) -> TestResult
where
    S: Strategy,
    F: Fn(S::Value) -> TestResult,
{
    ensure_property_with_config(
        strategy,
        context,
        strict_default_config(),
        property,
    )
}

/// Run `property` against inputs generated from `strategy` under a
/// caller-built [`Config`], shrinking any falsifying input to a minimal
/// counterexample.
///
/// The `config` is handed to the runner verbatim: the caller owns every
/// choice in it, including failure persistence and the RNG seed. Use
/// [`strict_default_config`] as a starting point to keep the strict
/// defaults (no persistence, deterministic seed) and override only
/// specific fields.
///
/// # Errors
///
/// Returns [`TestFailure::PropertyFalsified`] when an input falsifies the
/// property (the report carries the engine's rendering of the shrunk
/// minimal failing input), or [`TestFailure::PropertyAborted`] when the
/// runner cannot complete a run (for example, a strategy filter rejects
/// too many inputs).
#[allow(
    clippy::single_call_fn,
    reason = "drive TestRunner::run under a caller Config, mapping TestError onto TestFailure"
)]
pub fn ensure_property_with_config<S, F>(
    strategy: &S,
    context: &'static str,
    config: Config,
    property: F,
) -> TestResult
where
    S: Strategy,
    F: Fn(S::Value) -> TestResult,
{
    let mut runner = TestRunner::new(config);
    let outcome = runner.run(strategy, |input| {
        property(input)
            .map_err(|failure| TestCaseError::fail(failure.to_string()))
    });
    match outcome {
        Ok(()) => Ok(()),
        Err(failed @ TestError::Fail(..)) => {
            Err(TestFailure::PropertyFalsified {
                context,
                report: failed.to_string(),
            })
        }
        Err(TestError::Abort(reason)) => Err(TestFailure::PropertyAborted {
            context,
            reason: reason.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::path::Path;
    use std::string::ToString as _;

    use strict_test_support::{
        ensure, ensure_all, ensure_contains, ensure_eq, ensure_some,
    };

    use super::{
        DETERMINISTIC_SEED, TestFailure, TestResult, ensure_property,
        ensure_property_with_config, resolve_seed, strict_default_config,
    };
    use crate::strategy::Strategy;
    use crate::test_runner::{Config, RngSeed};

    #[test]
    fn ensure_property_passes_when_the_property_holds() -> TestResult {
        ensure_property(
            &(0_u32..10),
            "all generated inputs satisfy the range bound",
            |input| ensure(input < 10, "input stays below ten"),
        )
    }

    #[test]
    fn ensure_property_reports_falsified_properties_with_minimal_input()
    -> TestResult {
        // Both the shrinking contract and the message-format contract are
        // read off one rendered failure, so the falsified property runs
        // once and the facets are batched.
        let failure = ensure_some(
            ensure_property(&(1_u32..32), "input stays below one", |input| {
                ensure(input < 1, "input stays below one")
            })
            .err(),
            "a falsified property must fail",
        )?;
        let rendered = failure.to_string();
        ensure_all(&[
            (
                rendered.contains("property falsified"),
                "the failure names the falsified family",
            ),
            (
                rendered.contains("input stays below one"),
                "the failure carries the property context",
            ),
            (
                rendered.contains("minimal failing input: 1"),
                "shrinking converges to the minimal counterexample",
            ),
        ])
    }

    #[test]
    fn ensure_property_reports_aborted_runs() -> TestResult {
        let rejecting = Strategy::prop_filter(
            0_u32..10,
            "rejected by the test fixture",
            |_input| false,
        );
        let failure = ensure_some(
            ensure_property(
                &rejecting,
                "a fully rejecting filter aborts the run",
                |_input| Ok(()),
            )
            .err(),
            "a fully rejecting strategy must abort the run",
        )?;
        ensure_contains(
            &failure.to_string(),
            "property aborted",
            "the failure names the abort family",
        )
    }

    #[test]
    fn ensure_property_with_config_honors_the_caller_config() -> TestResult {
        // The runner must take the caller's Config verbatim: a case count
        // of 7 runs exactly 7 successful cases, and the strict default
        // persistence/seed choices are not re-imposed on it.
        let executed = Cell::new(0_u32);
        let config = Config {
            cases: 7,
            failure_persistence: None,
            rng_seed: RngSeed::Fixed(11),
            ..Config::default()
        };
        ensure_property_with_config(
            &(0_u32..100),
            "an explicit config drives the run",
            config,
            |_input| {
                executed.set(executed.get() + 1);
                Ok(())
            },
        )?;
        ensure_eq(
            &executed.get(),
            &7,
            "the caller's case count is used verbatim",
        )
    }

    #[test]
    fn strict_default_config_preserves_ordinary_proptest_defaults() -> TestResult
    {
        // strict_default_config only pins persistence and the seed; the
        // remaining knobs must still come from Config::default() so the
        // PROPTEST_* environment overrides keep working through it.
        let strict = strict_default_config();
        let ordinary = Config::default();
        ensure_all(&[
            (
                strict.cases == ordinary.cases,
                "the case count comes from Config::default",
            ),
            (
                strict.max_shrink_iters == ordinary.max_shrink_iters,
                "the shrink budget comes from Config::default",
            ),
        ])
    }

    #[test]
    fn resolve_seed_maps_raw_values_onto_rng_seeds() -> TestResult {
        // The pure seam is driven directly so no test mutates the process
        // environment (racy, and set_var is banned by the lint policy).
        ensure_all(&[
            (
                resolve_seed(None) == RngSeed::Fixed(DETERMINISTIC_SEED),
                "unset pins the fixed deterministic seed",
            ),
            (
                resolve_seed(Some("random")) == RngSeed::Random,
                "random opts into OS entropy",
            ),
            (
                resolve_seed(Some("42")) == RngSeed::Fixed(42),
                "a bare integer pins that exact seed",
            ),
            (
                resolve_seed(Some("garbage"))
                    == RngSeed::Fixed(DETERMINISTIC_SEED),
                "an unrecognized value falls back to the fixed seed",
            ),
        ])
    }

    /// A strict-shaped config carrying an explicit seed, so seed behavior
    /// is driven without touching `STRICT_TEST_SEED` itself.
    fn seeded_config(seed: RngSeed) -> Config {
        Config {
            failure_persistence: None,
            rng_seed: seed,
            ..Config::default()
        }
    }

    #[test]
    fn fixed_seed_replays_the_same_samples() -> TestResult {
        // Fold each drawn sample through an order-sensitive accumulator;
        // two runs under the same fixed seed must fold to the same value.
        let strategy = 0_u64..1_000;
        let fold = |seed: RngSeed| -> Result<u64, TestFailure> {
            let acc = Cell::new(0_u64);
            ensure_property_with_config(
                &strategy,
                "fold the sampled inputs",
                seeded_config(seed),
                |sample| {
                    acc.set(acc.get().wrapping_mul(31).wrapping_add(sample));
                    Ok(())
                },
            )?;
            Ok(acc.get())
        };
        let first = fold(RngSeed::Fixed(DETERMINISTIC_SEED))?;
        let second = fold(RngSeed::Fixed(DETERMINISTIC_SEED))?;
        ensure_eq(
            &first,
            &second,
            "a fixed seed replays the same sampled input sequence",
        )
    }

    #[test]
    fn random_seed_still_drives_the_property() -> TestResult {
        // The entropy path must execute and honor both polarities: a true
        // property passes and a false one still falsifies.
        ensure_property_with_config(
            &(0_u32..10),
            "a random-seeded run holds the bound",
            seeded_config(RngSeed::Random),
            |sample| ensure(sample < 10, "sample stays below ten"),
        )?;
        let failure = ensure_some(
            ensure_property_with_config(
                &(1_u32..32),
                "a random-seeded run still falsifies",
                seeded_config(RngSeed::Random),
                |sample| ensure(sample < 1, "sample below one"),
            )
            .err(),
            "a false property must falsify even under a random seed",
        )?;
        ensure_contains(
            &failure.to_string(),
            "property falsified",
            "the random-seeded failure names the falsified family",
        )
    }

    #[test]
    fn strict_runs_never_write_persistence_files() -> TestResult {
        // Config-level: the strict default disables persistence outright,
        // so the runner has nothing to write with.
        ensure(
            strict_default_config().failure_persistence.is_none(),
            "strict_default_config must disable failure persistence",
        )?;
        // Behavior-level: falsify a property under the default strict
        // config, then pin the absence of the exact file the default
        // persistence would have used. Proptest's default is
        // FileFailurePersistence::SourceParallel("proptest-regressions"),
        // which maps a source under src/ to a crate-root sibling tree
        // (failure_persistence/file.rs::resolve): this module's source
        // <crate>/src/strict.rs resolves to
        // <crate>/proptest-regressions/strict.txt.
        let failure = ensure_some(
            ensure_property(
                &(1_u32..32),
                "falsify a property to probe persistence",
                |input| ensure(input < 1, "input stays below one"),
            )
            .err(),
            "the persistence probe property must falsify",
        )?;
        ensure_contains(
            &failure.to_string(),
            "property falsified",
            "the persistence probe reports the falsified family",
        )?;
        let counterfactual = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("proptest-regressions")
            .join("strict.txt");
        ensure(
            !counterfactual.exists(),
            "a strict run must not write proptest-regressions/strict.txt",
        )
    }
}
