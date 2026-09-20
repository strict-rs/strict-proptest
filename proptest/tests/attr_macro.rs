//-
// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Runtime contracts of typed attribute-generated property wrappers.

#![cfg(feature = "attr-macro")]

#[cfg(test)]
mod tests {
  use proptest::strategy::Just;
  use proptest::strict::strict_default_config;
  use proptest::test_runner::Config;
  use proptest::test_runner::EvaluationOutcome;
  use proptest::test_runner::PropertyCause;
  use proptest::test_runner::PropertyResult;
  use strict_test_support::ComparisonFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_that;

  /// A result alias must preserve both native comparison subjects.
  type Comparison = Result<(i32, i32), ComparisonFailure<i32, i32>>;

  /// Regression for issue 601: plain and destructured mutable bindings survive.
  #[proptest::property_test]
  fn attr_macro_does_not_clobber_mutability(mut x: i32, (mut y, _z): (i32, i32)) -> Comparison {
    x = x.saturating_sub(x);
    y = y.saturating_sub(y);
    ensure_eq(x, y, "reassigned mut bindings agree after zeroing")
  }

  /// The harness skips this intentionally falsifying fixture; its wrapper is callable.
  #[ignore = "falsifying fixture exercised through its returned typed outcome"]
  #[proptest::property_test]
  fn falsifying_wrapper_surfaces_test_failure(x: i32) -> Result<i32, PredicateFailure<i32>> {
    ensure_that(x, "input stays below one", |observed| *observed < 1)
  }

  /// The native counterexample has a concrete tuple type visible to the consumer.
  type Falsification = PropertyResult<(i32,), i32, PredicateFailure<i32>>;

  #[test]
  fn generated_wrapper_returns_test_failure_instead_of_panicking() -> Result<(), PredicateFailure<Falsification>> {
    ensure_that(
      falsifying_wrapper_surfaces_test_failure(),
      "the original failure and minimal argument tuple agree",
      |outcome| {
        outcome.as_ref().is_err_and(|report| {
          report.run.argument_labels == ["x"]
            && matches!(report.cause, PropertyCause::Falsified { counterexample: (1,), ref failure, .. } if failure.subject == 1)
        })
      },
    )
    .map(drop)
  }

  #[proptest::property_test(config = Config { cases: 3, ..strict_default_config() })]
  fn custom_strategy_returns_owned_subject(
    #[strategy = Just(String::from("owned"))] text: String,
  ) -> Result<String, PredicateFailure<String>> {
    ensure_that(text, "custom strategy produces owned text", |observed| observed == "owned")
  }

  /// Owned returns and their complete property execution evidence.
  type OwnedRun = PropertyResult<(String,), String, PredicateFailure<String>>;

  #[test]
  fn generated_wrapper_retains_every_success() -> Result<(), PredicateFailure<OwnedRun>> {
    ensure_that(
      custom_strategy_returns_owned_subject(),
      "configured case count and owned successes reach callers",
      |outcome| {
        outcome.as_ref().is_ok_and(|run| {
          run.statistics.successes == 3
            && run.evaluations.len() == 3
            && run.argument_labels == ["text"]
            && run
              .evaluations
              .iter()
              .all(|evaluation| matches!(&evaluation.outcome, EvaluationOutcome::Returned(Ok(subject)) if subject == "owned"))
        })
      },
    )
    .map(drop)
  }
}
