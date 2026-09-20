//-
// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Typed, deterministically seeded property execution through the native runner.
//!
//! Properties return their original success subjects and concrete failures.
//! The native report retains every evaluation, and a falsification pairs the
//! minimized strategy value with the assertion failure from that evaluation.
//! Preconditions belong in strategies such as [`Strategy::prop_filter`].
//!
//! [`ensure_property`] disables regression persistence and uses a fixed seed.
//! `STRICT_TEST_SEED=random` selects entropy, an integer selects that seed,
//! and absent or invalid values use `0x5EED`. Other `PROPTEST_*` settings
//! still come from [`Config::default`]. [`ensure_property_with_config`]
//! preserves the caller's configuration.
//!
//! Fork or timeout execution requires [`ensure_property_with_transport`].
//! Its codec transports subjects, failures, counterexamples, and shared case
//! state. In-process execution requires no serialization bounds.

use std::env;

use crate::strategy::Strategy;
use crate::test_runner::Config;
use crate::test_runner::PropertyResult;
use crate::test_runner::PropertyTransport;
use crate::test_runner::RngSeed;
use crate::test_runner::TestRunner;

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
    Some(literal) => literal.parse::<RngSeed>().unwrap_or(RngSeed::Fixed(DETERMINISTIC_SEED)),
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

/// Run a typed property under deterministic strict defaults and shrink failures.
///
/// Every successful subject and concrete failure remains in the native report.
/// Values need not be cloneable, thread safe, or serializable.
///
/// # Errors
///
/// Returns a native falsification, generation abort, or engine failure with
/// strict context and all reached evidence. Fork or timeout requires an
/// explicit transport and is rejected before invoking the property.
///
/// # Examples
///
/// ```
/// use proptest::strict::ensure_property;
/// use proptest::test_runner::PropertyFailure;
/// use proptest::test_runner::PropertyResult;
/// use strict_test_support::PredicateFailure;
/// use strict_test_support::ensure_that;
///
/// fn range_property() -> PropertyResult<u32, u32, PredicateFailure<u32>> {
///   ensure_property(&(0_u32..10), "samples stay below ten", |sample| {
///     ensure_that(sample, "sample below ten", |sample| *sample < 10)
///   })
/// }
///
/// # fn main() -> Result<(), Box<PropertyFailure<u32, u32, PredicateFailure<u32>>>> {
/// range_property().map(drop)
/// # }
/// ```
pub fn ensure_property<S, F, A, E>(strategy: &S, context: &'static str, property: F) -> PropertyResult<S::Value, A, E>
where
  S: Strategy,
  F: Fn(S::Value) -> Result<A, E>,
{
  ensure_property_with_config(strategy, context, strict_default_config(), property)
}

/// Run a typed property using the caller's configuration verbatim.
///
/// Use [`strict_default_config`] when only selected settings should differ
/// from deterministic, persistence-free defaults.
///
/// # Errors
///
/// Returns a native falsification or engine failure with all reached evidence.
/// Fork and timeout settings require the explicit transport entry.
#[allow(
  clippy::single_call_fn,
  reason = "public entry point preserves caller-selected configuration independently of strict defaults"
)]
pub fn ensure_property_with_config<S, F, A, E>(
  strategy: &S,
  context: &'static str,
  config: Config,
  property: F,
) -> PropertyResult<S::Value, A, E>
where
  S: Strategy,
  F: Fn(S::Value) -> Result<A, E>,
{
  attach_context(TestRunner::new(config).run_typed(strategy, property), context)
}

/// Run with a declared typed fork transport and caller-selected configuration.
///
/// The runner owns process supervision, shrinking, persistence, and temporary
/// file finalization. The codec owns payload representation and restoration of
/// shared state before replayed shrinking. A local run does not invoke it.
///
/// # Errors
///
/// Preserves assertion and codec failures separately from native generation,
/// process, timeout, framing, I/O, and finalization failures.
pub fn ensure_property_with_transport<S, F, A, E, C>(
  strategy: &S,
  context: &'static str,
  config: Config,
  transport: C,
  property: F,
) -> PropertyResult<S::Value, A, E, C::Error>
where
  S: Strategy,
  F: Fn(S::Value) -> Result<A, E>,
  C: PropertyTransport<S::Value, A, E>,
{
  attach_context(
    TestRunner::new(config).run_typed_with_transport(strategy, property, transport),
    context,
  )
}

/// Attach context without changing any native payload or configuration.
fn attach_context<V, A, E, T>(outcome: PropertyResult<V, A, E, T>, context: &'static str) -> PropertyResult<V, A, E, T> {
  match outcome {
    Ok(mut run) => {
      run.context = context;
      Ok(run)
    }
    Err(mut failure) => {
      failure.context = context;
      failure.run.context = context;
      Err(failure)
    }
  }
}

#[cfg(test)]
mod tests {
  use core::cell::Cell;
  use core::convert::Infallible;
  use std::fs::Metadata;
  use std::fs::metadata;
  use std::io;
  use std::path::PathBuf;

  use strict_test_support::ComparisonFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Box;
  use crate::test_runner::CaseOrigin;
  use crate::test_runner::EvaluationOutcome;
  use crate::test_runner::MapFailurePersistence;
  use crate::test_runner::PropertyCause;

  /// The native evidence returned by range-bound assertions.
  type RangeOutcome = PropertyResult<u32, u32, PredicateFailure<u32>>;

  /// Native scalar runs with no property assertion failure.
  type ScalarRun = PropertyResult<u32, u32, Infallible>;
  /// Native sample runs used to compare deterministic sequences.
  type SampleRun = PropertyResult<u64, u64, Infallible>;
  /// A concrete allocation retains complete assertion subjects on failure.
  type Check<S> = Result<(), Box<PredicateFailure<S>>>;
  /// All four supported seed-resolution inputs.
  type SeedResolutions = [RngSeed; 4];
  /// Configuration, run, requested path, and native filesystem observation.
  type PersistenceProbe = (Config, RangeOutcome, PathBuf, io::Result<Metadata>);
  /// Persisting runner, original failing run, and baseline/replayed passing runs.
  type PersistenceReplay = (TestRunner, PropertyResult<u32, Infallible, u32>, [ScalarRun; 2]);

  #[test]
  fn ensure_property_passes_when_the_property_holds() -> Check<RangeOutcome> {
    ensure_that(
      ensure_property(&(0_u32..10), "range bound", |input| {
        ensure_that(input, "below ten", |subject| *subject < 10)
      }),
      "the report retains successful subjects and strict context",
      |outcome| {
        outcome.as_ref().is_ok_and(|run| {
          run.context == "range bound"
            && run
              .evaluations
              .iter()
              .all(|record| matches!(record.outcome, EvaluationOutcome::Returned(Ok(value)) if value < 10))
        })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn ensure_property_reports_falsified_properties_with_minimal_input() -> Check<RangeOutcome> {
    ensure_that(
      ensure_property(&(1_u32..32), "input stays below one", |input| {
        ensure_that(input, "below one", |subject| *subject < 1)
      }),
      "falsification retains the minimized input and original assertion subject",
      |outcome| {
        outcome.as_ref().is_err_and(|report| {
          report.context == "input stays below one"
            && matches!(report.cause, PropertyCause::Falsified { counterexample: 1, ref failure, .. } if failure.subject == 1)
        })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn ensure_property_reports_aborted_runs() -> Check<ScalarRun> {
    let rejecting = (0_u32..10).prop_filter("rejecting strategy", |_| false);
    ensure_that(
      ensure_property(&rejecting, "generation abort", Ok),
      "generation abort does not invent an assertion failure or evaluation",
      |outcome| {
        outcome
          .as_ref()
          .is_err_and(|report| matches!(report.cause, PropertyCause::Aborted(_)) && report.run.evaluations.is_empty())
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn ensure_property_with_config_honors_the_caller_config() -> Check<(ScalarRun, u32)> {
    let executed = Cell::new(0_u32);
    let config = Config {
      cases: 7,
      failure_persistence: None,
      rng_seed: RngSeed::Fixed(11),
      ..Config::default()
    };
    let run = ensure_property_with_config(&(0_u32..100), "explicit config", config, |input| {
      executed.set(executed.get().saturating_add(1));
      Ok(input)
    });
    ensure_that(
      (run, executed.get()),
      "configured cases execute exactly once each and retain their subjects",
      |observed| {
        observed.1 == 7
          && observed
            .0
            .as_ref()
            .is_ok_and(|report| report.statistics.successes == 7 && report.evaluations.len() == 7)
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn explicit_persistence_preserves_replay_counting_and_generated_rng_sequence() -> Check<PersistenceReplay> {
    let config = Config {
      cases: 4,
      source_file: Some(file!()),
      failure_persistence: Some(Box::new(MapFailurePersistence::default())),
      rng_seed: RngSeed::Fixed(11),
      ..Config::default()
    };
    let mut persisting = TestRunner::new(config);
    let failure = persisting.run_typed(&(0_u32..1_000), Err);
    let replay_config = persisting.config().clone();
    let baseline_config = Config {
      failure_persistence: None,
      ..replay_config
    };
    let baseline = ensure_property_with_config(&(0_u32..1_000), "baseline", baseline_config, Ok);
    let replayed = ensure_property_with_config(&(0_u32..1_000), "replayed", replay_config, Ok);
    ensure_that((persisting, failure, [baseline, replayed]), "explicit persistence replays a saved failure without counting it or perturbing generated samples", |observed| {
      let [Ok(ref baseline_run), Ok(ref replay_run)] = observed.2 else {
        return false;
      };
      let Some(persisted) = replay_run.evaluations.first() else {
        return false;
      };
      observed.0.config().failure_persistence.as_ref().is_some_and(|backend| backend.load_persisted_failures2(Some(file!())).len() == 1)
        && observed.1.as_ref().is_err_and(|report| matches!(report.cause, PropertyCause::Falsified { counterexample: 0, failure: 0, .. }))
        && persisted.origin == CaseOrigin::Persisted
        && baseline_run.statistics.successes == 4
        && replay_run.statistics.successes == 4
        && baseline_run.evaluations.len() == 4
        && replay_run.evaluations.len() == 5
        && baseline_run.evaluations.first().is_some_and(|first| {
          matches!((&first.outcome, &persisted.outcome), (&EvaluationOutcome::Returned(Ok(initial)), &EvaluationOutcome::Returned(Ok(recovered))) if initial == recovered)
        })
        && baseline_run.evaluations.iter().zip(replay_run.evaluations.iter().skip(1)).all(|(baseline_case, replay_case)| {
          baseline_case.origin == CaseOrigin::Generated && replay_case.origin == CaseOrigin::Generated
            && matches!((&baseline_case.outcome, &replay_case.outcome), (&EvaluationOutcome::Returned(Ok(expected)), &EvaluationOutcome::Returned(Ok(actual))) if expected == actual)
        })
    })
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn strict_default_config_preserves_ordinary_proptest_defaults() -> Check<(Config, Config)> {
    ensure_that(
      (strict_default_config(), Config::default()),
      "strict defaults override only persistence and seeding",
      |observed| {
        let mut expected = observed.1.clone();
        expected.failure_persistence = None;
        expected.rng_seed = observed.0.rng_seed;
        observed.0 == expected
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn resolve_seed_maps_raw_values_onto_rng_seeds() -> Result<(), Box<ComparisonFailure<SeedResolutions, SeedResolutions>>> {
    ensure_eq(
      [
        resolve_seed(None),
        resolve_seed(Some("random")),
        resolve_seed(Some("42")),
        resolve_seed(Some("garbage")),
      ],
      [
        RngSeed::Fixed(DETERMINISTIC_SEED),
        RngSeed::Random,
        RngSeed::Fixed(42),
        RngSeed::Fixed(DETERMINISTIC_SEED),
      ],
      "seed resolution preserves absent, entropy, explicit, and invalid cases",
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// Construct explicit seed configuration without mutating process environment.
  fn seeded_config(seed: RngSeed) -> Config {
    Config {
      failure_persistence: None,
      rng_seed: seed,
      ..Config::default()
    }
  }

  #[test]
  fn fixed_seed_replays_the_same_samples() -> Check<[SampleRun; 2]> {
    let runs = [(); 2].map(|()| {
      ensure_property_with_config(
        &(0_u64..1_000),
        "sample sequence",
        seeded_config(RngSeed::Fixed(DETERMINISTIC_SEED)),
        Ok,
      )
    });
    ensure_that(runs, "a fixed seed reproduces the complete native sample sequence", |observed| {
      let [Ok(ref first_run), Ok(ref second_run)] = *observed else {
        return false;
      };
      first_run.evaluations.len() == second_run.evaluations.len()
        && first_run.evaluations.iter().zip(&second_run.evaluations).all(|(left, right)| {
          matches!((&left.outcome, &right.outcome),
            (&EvaluationOutcome::Returned(Ok(left_subject)), &EvaluationOutcome::Returned(Ok(right_subject)))
              if left_subject == right_subject)
        })
    })
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn random_seed_still_drives_the_property() -> Check<[RangeOutcome; 2]> {
    let passing = ensure_property_with_config(&(0_u32..10), "random pass", seeded_config(RngSeed::Random), |sample| {
      ensure_that(sample, "below ten", |subject| *subject < 10)
    });
    let failing = ensure_property_with_config(&(1_u32..32), "random failure", seeded_config(RngSeed::Random), |sample| {
      ensure_that(sample, "below one", |subject| *subject < 1)
    });
    ensure_that(
      [passing, failing],
      "entropy preserves both passing and shrinking behavior",
      |outcomes| matches!(*outcomes, [Ok(_), Err(ref report)] if matches!(report.cause, PropertyCause::Falsified { counterexample: 1, .. })),
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn strict_runs_never_write_persistence_files() -> Check<PersistenceProbe> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("proptest-regressions")
      .join("strict.txt");
    let outcome = ensure_property(&(1_u32..32), "persistence probe", |input| {
      ensure_that(input, "below one", |subject| *subject < 1)
    });
    let observed_path = metadata(&path);
    ensure_that(
      (strict_default_config(), outcome, path, observed_path),
      "strict defaults disable persistence even after a minimized failure",
      |observed| {
        observed.0.failure_persistence.is_none()
          && observed.3.as_ref().is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
          && observed.1.as_ref().is_err_and(|report| {
            matches!(report.cause, PropertyCause::Falsified {
              counterexample: 1,
              ..
            })
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}
