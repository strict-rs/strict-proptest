// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Typed evaluation storage behind the native runner's shared case walk.

use core::fmt;
use core::mem;
#[cfg(feature = "std")]
use std::panic;
#[cfg(feature = "std")]
use std::panic::AssertUnwindSafe;
#[cfg(feature = "std")]
use std::time::Instant;

use super::CaseOrigin;
use super::EstablishedFailure;
use super::Evaluation;
use super::EvaluationId;
use super::EvaluationOutcome;
use super::ExecutionError;
use super::ExecutionEvent;
use super::Interruption;
use super::PropertyCause;
use super::PropertyFailure;
use super::PropertyResult;
use super::PropertyRun;
use super::ResultCache;
use super::ResultCacheKey;
use super::RunStatistics;
use super::TestRunner;
use super::errors::TestCaseOk;
use super::execution::CaseVerdict;
use super::execution::Execution;
use super::execution::RunFailure;
use super::transport::EvaluationChannel;
use crate::std_facade::Box;
#[cfg(feature = "std")]
use crate::std_facade::String;

/// Shared-runner completion with identities referring to this adapter's payload storage.
type TypedRunResult<V, T> = Result<(), RunFailure<V, EvaluationId, ExecutionError<T>>>;

/// The stopping cause and any established pair retained after an engine failure.
type FailureCause<V, E, T> = (PropertyCause<V, E, T>, Option<EstablishedFailure<V>>);

/// Owns callback results while the algorithm and cache exchange identities.
pub(super) struct TypedExecution<F, C, A, E> {
  /// The property callback.
  pub(super) property: F,
  /// Local execution or the caller-declared fork transport.
  pub(super) channel:  C,
  /// Caller-selected cache policy, storing evaluation identities only.
  pub(super) cache:    Box<dyn ResultCache>,
  /// Complete reached evidence.
  pub(super) run:      PropertyRun<A, E>,
}

impl<F, C, A, E> TypedExecution<F, C, A, E> {
  /// Resolve a cached or newly-recorded outcome without cloning its payload.
  fn verdict<T>(&self, id: EvaluationId, passed: TestCaseOk) -> Result<CaseVerdict<EvaluationId>, ExecutionError<T>> {
    match self.run.evaluations.get(id.0).map(|record| &record.outcome) {
      Some(&EvaluationOutcome::Returned(Ok(_))) => Ok(CaseVerdict::Passed(passed)),
      Some(
        &EvaluationOutcome::Returned(Err(_))
        | &EvaluationOutcome::Interrupted(_)
        | &EvaluationOutcome::TimedOut {
          ..
        },
      ) => Ok(CaseVerdict::Failed(id)),
      Some(&EvaluationOutcome::RetainedFailure) | None => Err(ExecutionError::InvalidEvaluation(id)),
    }
  }

  /// Move the selected failure to its counterexample, leaving its history identity.
  fn cause<V, T>(&mut self, stopped: RunFailure<V, EvaluationId, ExecutionError<T>>) -> FailureCause<V, E, T> {
    let cause = match stopped {
      RunFailure::Aborted(reason) => PropertyCause::Aborted(reason),
      RunFailure::Engine {
        error,
        failing,
      } => {
        return (
          PropertyCause::Engine(error),
          failing.map(|(evaluation, counterexample)| EstablishedFailure {
            counterexample,
            evaluation,
          }),
        );
      }
      RunFailure::Falsified(id, counterexample) => {
        let Some(record) = self.run.evaluations.get_mut(id.0) else {
          return (
            PropertyCause::Engine(ExecutionError::InvalidEvaluation(id)),
            Some(EstablishedFailure {
              counterexample,
              evaluation: id,
            }),
          );
        };
        match mem::replace(&mut record.outcome, EvaluationOutcome::RetainedFailure) {
          EvaluationOutcome::Returned(Err(failure)) => PropertyCause::Falsified {
            counterexample,
            failure,
            evaluation: id,
          },
          EvaluationOutcome::Interrupted(interruption) => PropertyCause::Interrupted {
            counterexample,
            interruption,
            evaluation: id,
          },
          outcome @ EvaluationOutcome::TimedOut {
            timeout_ms, ..
          } => {
            record.outcome = outcome;
            PropertyCause::Interrupted {
              counterexample,
              interruption: Interruption::TimedOut {
                timeout_ms,
              },
              evaluation: id,
            }
          }
          outcome @ (EvaluationOutcome::Returned(Ok(_)) | EvaluationOutcome::RetainedFailure) => {
            record.outcome = outcome;
            PropertyCause::Engine(ExecutionError::InvalidEvaluation(id))
          }
        }
      }
    };
    (cause, None)
  }

  /// Finalize replay, preserve any primary failure, and return owned evidence.
  pub(super) fn complete<V, T>(mut self, result: TypedRunResult<V, T>, statistics: RunStatistics) -> PropertyResult<V, A, E, T>
  where
    C: EvaluationChannel<V, A, E, Error = T>,
  {
    self.run.statistics = statistics;
    let completed = !matches!(result, Err(RunFailure::Engine { .. }));
    let mut finalization = self.channel.finish(completed).into_iter();
    let cause = match result {
      Err(failure) => self.cause(failure),
      Ok(()) => match finalization.next() {
        Some(error) => (PropertyCause::Engine(error), None),
        None => return Ok(self.run),
      },
    };
    Err(Box::new(PropertyFailure {
      context:             self.run.context,
      run:                 self.run,
      cause:               cause.0,
      established_failure: cause.1,
      finalization:        finalization.collect(),
    }))
  }
}

impl<V, F, C, A, E> Execution<V> for TypedExecution<F, C, A, E>
where
  V: fmt::Debug,
  F: Fn(V) -> Result<A, E>,
  C: EvaluationChannel<V, A, E>,
{
  type Failure = EvaluationId;
  type Error = ExecutionError<C::Error>;

  fn evaluate(
    &mut self,
    runner: &TestRunner,
    observed: &mut V,
    input: V,
    origin: CaseOrigin,
  ) -> Result<CaseVerdict<EvaluationId>, Self::Error> {
    let key = self.cache.key(&ResultCacheKey::new(&input));
    if let Some(id) = self.cache.get(key) {
      let verdict = self.verdict(id, TestCaseOk::CacheHitSuccess)?;
      self.run.events.push(ExecutionEvent::Cached(id));
      return Ok(verdict);
    }
    let id = EvaluationId(self.run.evaluations.len());
    let replayed = self.channel.replay(id, observed, origin)?;
    let Some(outcome) = replayed else {
      let outcome = invoke(&self.property, input, runner.config().timeout());
      // Store the native result even if publishing it fails.
      let published = self.channel.record(id, observed, origin, &outcome);
      self.run.evaluations.push(Evaluation {
        origin,
        outcome,
      });
      self.run.events.push(ExecutionEvent::Evaluated(id));
      published?;
      self.cache.put(key, id);
      let passed = if origin == CaseOrigin::Persisted {
        TestCaseOk::PersistedCaseSuccess
      } else {
        TestCaseOk::NewCaseSuccess
      };
      return self.verdict(id, passed);
    };
    self.run.evaluations.push(Evaluation {
      origin,
      outcome,
    });
    self.run.events.push(ExecutionEvent::Evaluated(id));
    self.cache.put(key, id);
    let passed = if origin == CaseOrigin::Persisted {
      TestCaseOk::PersistedCaseSuccess
    } else {
      TestCaseOk::ReplayFromForkSuccess
    };
    self.verdict(id, passed)
  }

  fn backtrack(&mut self) -> Result<(), Self::Error> {
    self.channel.backtrack()?;
    self.run.events.push(ExecutionEvent::Backtracked);
    Ok(())
  }

  fn shrink_budget(&mut self, exhausted: bool) -> Result<bool, Self::Error> {
    self.channel.shrink_budget(exhausted)
  }

  fn is_in_fork(&self) -> bool {
    self.channel.is_in_fork()
  }
}

/// Catch an unwinding panic without changing returned assertion payloads.
#[cfg(feature = "std")]
#[allow(
  clippy::single_call_fn,
  reason = "keep panic and timeout supervision separate from typed evaluation storage and replay"
)]
fn invoke<V, A, E>(property: &impl Fn(V) -> Result<A, E>, input: V, timeout_ms: u32) -> EvaluationOutcome<A, E> {
  let started = Instant::now();
  match super::scoped_panic_hook::suppress_panic_hook(|| panic::catch_unwind(AssertUnwindSafe(|| property(input)))) {
    Ok(result) => {
      let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
      if timeout_ms != 0 && elapsed_ms > u64::from(timeout_ms) && result.is_ok() {
        EvaluationOutcome::TimedOut {
          result,
          timeout_ms,
          elapsed_ms,
        }
      } else {
        EvaluationOutcome::Returned(result)
      }
    }
    Err(payload) => {
      let reason = payload
        .downcast::<&'static str>()
        .map(|message| (*message).into())
        .or_else(|non_static| non_static.downcast::<String>().map(|message| (*message).into()))
        .or_else(|non_string| non_string.downcast::<Box<str>>().map(|message| (*message).into()))
        .unwrap_or_else(|_| "<unknown panic value>".into());
      EvaluationOutcome::Interrupted(Interruption::Panicked(reason))
    }
  }
}

/// Evaluate directly where the platform does not provide unwind supervision.
#[cfg(not(feature = "std"))]
#[allow(
  clippy::single_call_fn,
  reason = "keep platform-specific callback invocation separate from typed evaluation storage and replay"
)]
fn invoke<V, A, E>(property: &impl Fn(V) -> Result<A, E>, input: V, _: u32) -> EvaluationOutcome<A, E> {
  EvaluationOutcome::Returned(property(input))
}

#[cfg(test)]
mod tests {
  use core::cell::Cell;
  #[cfg(feature = "std")]
  use core::convert::Infallible;
  use std::process::ExitCode;
  use std::process::Termination as _;
  #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
  use std::thread;
  #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
  use std::time::Duration;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::num::u32::BinarySearch;
  use crate::std_facade::Rc;
  use crate::std_facade::String;
  use crate::std_facade::Vec;
  #[cfg(feature = "fork")]
  use crate::strategy::Just;
  use crate::strategy::NewTree;
  use crate::strategy::Strategy;
  #[cfg(feature = "std")]
  use crate::strategy::ValueTree;
  use crate::test_runner::Config;
  use crate::test_runner::RngSeed;
  use crate::test_runner::TestCaseError;
  use crate::test_runner::TestError;
  #[cfg(feature = "std")]
  use crate::test_runner::basic_result_cache;

  /// A deliberately non-Clone, non-Send subject borrowing caller-provided data.
  #[derive(Debug)]
  struct Subject<'a> {
    /// The input observed by the callback.
    value:    u32,
    /// Data whose lifetime is chosen by the caller, not the runner.
    borrowed: &'a str,
    /// A process-local owner that cannot be sent between threads.
    owner:    Rc<()>,
  }

  /// A single deterministic initial candidate with ordinary numeric shrinking.
  #[derive(Debug)]
  struct StartingAt(u32);

  impl Strategy for StartingAt {
    type Tree = BinarySearch;
    type Value = u32;

    fn new_tree(&self, _: &mut TestRunner) -> NewTree<Self> {
      Ok(BinarySearch::new(self.0))
    }
  }

  /// Native report with a caller-selected borrowing lifetime.
  type BorrowedRun<'a> = PropertyResult<u32, Subject<'a>, Subject<'a>>;
  /// Native checks whose subjects remain in a concrete allocation on failure.
  type Check<S> = Result<(), Box<PredicateFailure<S>>>;
  /// Failure-only run paired with its exact callback count.
  #[cfg(feature = "std")]
  type CachedRun = (PropertyResult<u32, Infallible, u32>, u32);
  /// Native legacy rejection result and every attempted candidate.
  type RejectedRun = (TestRunner, Result<bool, TestError<u32>>, Vec<u32>);
  /// Callback admission and the native transport-configuration result.
  #[cfg(feature = "fork")]
  type ForkAdmission = (PropertyResult<u32, u32, Infallible>, bool);
  /// A supervised panic run and every candidate admitted to its callback.
  #[cfg(feature = "std")]
  type PanicRun = (PropertyResult<u32, u32, Infallible>, Vec<u32>);
  /// Supported panic representations paired with their expected diagnostic and native run.
  #[cfg(feature = "std")]
  type PanicRuns = [(&'static str, PanicRun); 4];

  /// Inject an unwind at the property boundary while observing its complete shrink walk.
  #[cfg(feature = "std")]
  fn panicking_run<P: Send + 'static>(payload: impl Fn() -> P) -> PanicRun {
    let calls = Cell::new(Vec::new());
    let config = Config {
      cases: 1,
      max_shrink_iters: 64,
      failure_persistence: None,
      ..Config::default()
    };
    let result = TestRunner::new(config).run_typed(&StartingAt(100), |value| {
      let mut attempted = calls.take();
      attempted.push(value);
      calls.set(attempted);
      if value >= 5 {
        panic::resume_unwind(Box::new(payload()));
      }
      Ok(value)
    });
    (result, calls.into_inner())
  }

  #[test]
  #[cfg(feature = "std")]
  fn panic_payloads_remain_interruptions_through_shrinking() -> Check<PanicRuns> {
    let observations = [
      ("static panic", panicking_run(|| "static panic")),
      ("owned panic", panicking_run(|| String::from("owned panic"))),
      ("boxed panic", panicking_run(|| Box::<str>::from("boxed panic"))),
      ("<unknown panic value>", panicking_run(|| 17_u32)),
    ];
    let preserves_panic = |&(message, (ref result, ref calls)): &(&'static str, PanicRun)| {
      let Err(ref report) = *result else {
        return false;
      };
      let PropertyCause::Interrupted {
        counterexample: 5,
        interruption: Interruption::Panicked(ref reason),
        evaluation,
      } = report.cause
      else {
        return false;
      };
      reason.message() == message
        && report.established_failure.is_none()
        && report.finalization.is_empty()
        && calls.len() == report.run.evaluations.len()
        && calls.get(evaluation.0) == Some(&5)
        && report
          .run
          .evaluations
          .iter()
          .zip(calls)
          .enumerate()
          .all(|(index, (record, value))| match record.outcome {
            EvaluationOutcome::Returned(Ok(returned)) => returned == *value && returned < 5,
            EvaluationOutcome::Interrupted(Interruption::Panicked(ref panic)) => *value >= 5 && panic.message() == message,
            EvaluationOutcome::RetainedFailure => index == evaluation.0,
            EvaluationOutcome::Returned(Err(never)) => match never {},
            EvaluationOutcome::Interrupted(
              Interruption::TimedOut {
                ..
              }
              | Interruption::ChildExited {
                ..
              },
            )
            | EvaluationOutcome::TimedOut {
              ..
            } => false,
          })
    };
    ensure_that(
      observations,
      "every panic representation preserves its diagnostic and minimized candidate without inventing an assertion failure or reevaluating \
       the retained case",
      |runs| runs.iter().all(preserves_panic),
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// Drive a borrowing property without imposing a static lifetime on its result.
  fn borrowing_run(borrowed: &str, minimum_failure: u32, max_shrink_iters: u32) -> BorrowedRun<'_> {
    let owner = Rc::new(());
    let config = Config {
      cases: 1,
      max_shrink_iters,
      failure_persistence: None,
      rng_seed: RngSeed::Fixed(0x5EED),
      ..Config::default()
    };
    TestRunner::new(config).run_typed(&StartingAt(100), |value| {
      let subject = Subject {
        value,
        borrowed,
        owner: Rc::clone(&owner),
      };
      if value >= minimum_failure {
        Err(subject)
      } else {
        Ok(subject)
      }
    })
  }

  #[test]
  fn preserves_owned_and_borrowed_success_subjects() -> ExitCode {
    let caller_owned = String::from("caller data");
    ensure_that(
      borrowing_run(&caller_owned, 101, 100),
      "successful reports retain non-Clone subjects and their resource owners",
      |outcome| {
        let Ok(run) = outcome.as_ref() else {
          return false;
        };
        run.statistics.successes == 1
          && matches!(*run.evaluations.as_slice(), [Evaluation {
        origin: CaseOrigin::Generated,
        outcome: EvaluationOutcome::Returned(Ok(ref subject)),
      }] if subject.value == 100 && subject.borrowed == "caller data" && Rc::strong_count(&subject.owner) == 1)
      },
    )
    .map(drop)
    .report()
  }

  #[test]
  fn minimizes_without_replacing_the_matching_failure() -> ExitCode {
    let caller_owned = String::from("borrowed failure");
    ensure_that(
      borrowing_run(&caller_owned, 5, 100),
      "the minimized counterexample retains its own original failure and the reached history",
      |outcome| {
        let Err(report) = outcome.as_ref() else {
          return false;
        };
        let PropertyCause::Falsified {
          counterexample,
          ref failure,
          evaluation,
        } = report.cause
        else {
          return false;
        };
        counterexample == 5
          && failure.value == counterexample
          && failure.borrowed == "borrowed failure"
          && report
            .run
            .evaluations
            .get(evaluation.0)
            .is_some_and(|record| matches!(record.outcome, EvaluationOutcome::RetainedFailure))
          && report.run.evaluations.iter().all(|record| match record.outcome {
            EvaluationOutcome::Returned(Ok(ref subject)) => subject.value < 5,
            EvaluationOutcome::Returned(Err(ref subject)) => subject.value >= 5,
            EvaluationOutcome::RetainedFailure => true,
            EvaluationOutcome::Interrupted(_)
            | EvaluationOutcome::TimedOut {
              ..
            } => false,
          })
      },
    )
    .map(drop)
    .report()
  }

  #[test]
  fn zero_budget_returns_the_original_failing_pair() -> Check<BorrowedRun<'static>> {
    ensure_that(
      borrowing_run("unshrunk", 5, 0),
      "zero shrink budget preserves the original pair with exactly one evaluation",
      |outcome| {
        let Err(report) = outcome.as_ref() else {
          return false;
        };
        matches!(report.cause, PropertyCause::Falsified { counterexample: 100, ref failure, .. } if failure.value == 100)
          && report.run.evaluations.len() == 1
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn passing_attempt_at_budget_does_not_replace_the_failure() -> Check<BorrowedRun<'static>> {
    ensure_that(
      borrowing_run("backtracking", 80, 1),
      "budget backtracking retains the established failure without evaluating it again",
      |outcome| {
        let Err(report) = outcome.as_ref() else {
          return false;
        };
        matches!(report.cause, PropertyCause::Falsified { counterexample: 100, ref failure, .. } if failure.value == 100)
          && report.run.evaluations.len() == 2
          && report
            .run
            .evaluations
            .iter()
            .any(|record| matches!(record.outcome, EvaluationOutcome::Returned(Ok(ref subject)) if subject.value == 50))
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn rejected_attempt_at_budget_does_not_replace_the_failure() -> Check<RejectedRun> {
    let calls = Cell::new(Vec::new());
    let mut runner = TestRunner::new(Config {
      max_shrink_iters: 1,
      failure_persistence: None,
      ..Config::default()
    });
    let result = runner.run_one(BinarySearch::new(100), |value| {
      let mut attempted = calls.take();
      attempted.push(value);
      calls.set(attempted);
      Err(if value == 100 {
        TestCaseError::fail("original failing candidate")
      } else {
        TestCaseError::reject("intermediate candidate rejected")
      })
    });
    ensure_that(
      (runner, result, calls.into_inner()),
      "rejection cannot replace the established failure or trigger reevaluation",
      |observed| {
        observed.2 == [100, 50]
          && matches!(observed.1, Err(TestError::Fail(ref reason, 100)) if *reason == "original failing candidate".into())
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
  fn passing_attempt_at_time_budget_retains_the_failing_pair() -> Check<PropertyResult<u32, u32, u32>> {
    let config = Config {
      cases: 1,
      max_shrink_time: 100,
      failure_persistence: None,
      ..Config::default()
    };
    let result = TestRunner::new(config).run_typed(&StartingAt(100), |value| {
      if value == 100 {
        Err(value)
      } else {
        thread::sleep(Duration::from_millis(120));
        Ok(value)
      }
    });
    ensure_that(
      result,
      "a passing timed shrink leaves the original native counterexample and failure paired",
      |observed| {
        observed.as_ref().is_err_and(|report| {
          matches!(report.cause, PropertyCause::Falsified {
            counterexample: 100,
            failure: 100,
            ..
          }) && matches!(*report.run.evaluations.as_slice(), [
            Evaluation {
              outcome: EvaluationOutcome::RetainedFailure,
              ..
            },
            Evaluation {
              outcome: EvaluationOutcome::Returned(Ok(50)),
              ..
            }
          ]) && report.run.events.contains(&ExecutionEvent::Backtracked)
        })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// A valid shrink walk that revisits one candidate, exercising cached failures.
  #[cfg(feature = "std")]
  #[derive(Clone, Copy, Debug)]
  struct RepeatedCandidate(u8);

  #[cfg(feature = "std")]
  impl Strategy for RepeatedCandidate {
    type Tree = Self;
    type Value = u32;

    fn new_tree(&self, _: &mut TestRunner) -> NewTree<Self> {
      Ok(*self)
    }
  }

  #[cfg(feature = "std")]
  impl ValueTree for RepeatedCandidate {
    type Value = u32;

    fn current(&self) -> u32 {
      match self.0 {
        0 => 8,
        1 | 2 => 4,
        3 => 2,
        _ => 1,
      }
    }

    fn simplify(&mut self) -> bool {
      if self.0 >= 4 {
        return false;
      }
      self.0 = self.0.saturating_add(1);
      true
    }

    fn complicate(&mut self) -> bool {
      false
    }
  }

  #[test]
  #[cfg(feature = "std")]
  fn cached_failures_reference_the_original_evaluation() -> Check<CachedRun> {
    let calls = Cell::new(0_u32);
    let config = Config {
      cases: 1,
      failure_persistence: None,
      result_cache: basic_result_cache,
      ..Config::default()
    };
    let result = TestRunner::new(config).run_typed(&RepeatedCandidate(0), |value| {
      calls.set(calls.get().saturating_add(1));
      Err(value)
    });
    ensure_that(
      (result, calls.get()),
      "cache hits reuse failure identities without invoking the property or cloning payloads",
      |observed| {
        let Err(ref report) = observed.0 else {
          return false;
        };
        observed.1 == 4
          && report.run.evaluations.len() == 4
          && report.run.events.contains(&ExecutionEvent::Cached(EvaluationId(1)))
          && matches!(report.cause, PropertyCause::Falsified {
            counterexample: 1,
            failure: 1,
            ..
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  #[cfg(feature = "fork")]
  fn fork_configuration_requires_transport_before_invocation() -> Check<ForkAdmission> {
    let called = Cell::new(false);
    let config = Config {
      fork: true,
      failure_persistence: None,
      ..Config::default()
    };
    let result = TestRunner::new(config).run_typed(&Just(3_u32), |value| {
      called.set(true);
      Ok(value)
    });
    ensure_that(
      (result, called.get()),
      "fork selection cannot silently become in-process execution",
      |observed| {
        !observed.1
          && observed.0.as_ref().is_err_and(|failure| {
            matches!(failure.cause, PropertyCause::Engine(ExecutionError::TransportRequired)) && failure.run.evaluations.is_empty()
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  #[cfg(feature = "timeout")]
  fn timeout_configuration_requires_transport_before_invocation() -> Check<ForkAdmission> {
    let called = Cell::new(false);
    let result = TestRunner::new(Config {
      fork: false,
      timeout: 1,
      failure_persistence: None,
      ..Config::default()
    })
    .run_typed(&Just(3_u32), |value| {
      called.set(true);
      Ok(value)
    });
    ensure_that(
      (result, called.get()),
      "timeout implies fork and therefore requires transport before callback admission",
      |observed| {
        !observed.1
          && observed.0.as_ref().is_err_and(|report| {
            matches!(report.cause, PropertyCause::Engine(ExecutionError::TransportRequired)) && report.run.evaluations.is_empty()
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}
