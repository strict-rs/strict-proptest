//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::Arc;
use crate::std_facade::fmt;
#[cfg(test)]
use crate::strategy::CheckStrategySanityOptions;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::traits::NewTree;
use crate::strategy::traits::Strategy;
use crate::strategy::traits::ValueTree;
use crate::test_runner::Reason;
use crate::test_runner::TestRunner;

/// Generate until the predicate accepts, charging every rejected value to
/// the runner's existing local-rejection budget.
pub(super) fn new_filtered_tree<S: Strategy>(
  source: &S,
  whence: &Reason,
  accepts: impl Fn(&S::Value) -> bool,
  runner: &mut TestRunner,
) -> NewTree<S> {
  loop {
    let tree = source.new_tree(runner)?;
    if accepts(&tree.current()) {
      return Ok(tree);
    }
    runner.reject_local(whence.clone())?;
  }
}

/// Recover an accepted source candidate by complicating without adding
/// generation attempts or consuming the runner's rejection budget.
pub(super) fn recover_filtered_value<T: ValueTree>(source: &mut T, accepts: impl Fn(&T::Value) -> bool) -> bool {
  while !accepts(&source.current()) {
    if !source.complicate() {
      return false;
    }
  }
  true
}

/// `Strategy` and `ValueTree` filter adaptor.
///
/// See `Strategy::prop_filter()`.
#[must_use = "strategies do nothing unless used"]
pub struct Filter<S, F> {
  /// The strategy or value tree whose values are being filtered.
  pub(super) source: S,
  /// The reason recorded with the runner each time a value is rejected.
  pub(super) whence: Reason,
  /// The predicate deciding acceptance, held behind an `Arc` so the wrapper
  /// clones cheaply.
  pub(super) fun:    Arc<F>,
}

impl<S, F> Filter<S, F> {
  /// Wrap `source` so that only values accepted by `fun` are produced,
  /// recording `whence` with the runner on each rejection.
  #[allow(
    clippy::single_call_fn,
    reason = "cache a Filter combinator's predicate and rejection reason behind an Arc"
  )]
  pub(super) fn new(source: S, whence: Reason, fun: F) -> Self {
    Self {
      source,
      whence,
      fun: Arc::new(fun),
    }
  }
}

impl_debug_struct!(Filter<S, F> [S: fmt::Debug] |self| {
  source: self.source,
  whence: self.whence,
  fun: "<function>",
});

impl_clone_shared_fn!(Filter<S, F> |self| {
  whence: "unused".into(),
});

impl<S: Strategy, F: Fn(&S::Value) -> bool> Strategy for Filter<S, F> {
  type Tree = Filter<S::Tree, F>;
  type Value = S::Value;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    new_filtered_tree(&self.source, &self.whence, |candidate| (self.fun)(candidate), runner).map(|source| Filter {
      source,
      whence: self.whence.clone(),
      fun: Arc::clone(&self.fun),
    })
  }
}

impl<S: ValueTree, F: Fn(&S::Value) -> bool> Filter<S, F> {
  /// After the source shrinks, `complicate()` it back until the predicate
  /// accepts the current value again. If no accepted value can be recovered,
  /// report that this shrink step produced no usable change.
  fn ensure_acceptable(&mut self) -> bool {
    recover_filtered_value(&mut self.source, |candidate| (self.fun)(candidate))
  }
}

impl<S: ValueTree, F: Fn(&S::Value) -> bool> ValueTree for Filter<S, F> {
  type Value = S::Value;

  delegate_value_tree!(source, ensure_acceptable);
}

#[cfg(test)]
pub(super) mod test {
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Box;
  use crate::std_facade::Vec;
  use crate::strategy::Just;
  use crate::strategy::statics;
  use crate::strategy::trace_shrink_steps;
  use crate::test_runner::Config;
  use crate::test_runner::test_runner_without_persistence;

  /// A complete shrink walk, or the native generation failure.
  type FilterWalk<S> = Result<(<S as Strategy>::Tree, Vec<<S as Strategy>::Value>), Reason>;

  /// All sampled filter walks retained on either assertion outcome.
  type CheckedFilterWalks<S> = Result<Vec<FilterWalk<S>>, Box<PredicateFailure<Vec<FilterWalk<S>>>>>;

  /// Check generated values and every reached shrink state against a predicate.
  pub(in crate::strategy) fn check_filtered_shrinking<S: Strategy>(
    input: &S,
    accepts: impl Fn(&S::Value) -> bool,
  ) -> CheckedFilterWalks<S> {
    let walks: Vec<_> = (0..256)
      .map(|_| {
        input.new_tree(&mut test_runner_without_persistence()).map(|tree| {
          let (reached, mut values) = trace_shrink_steps(tree);
          values.push(reached.current());
          (reached, values)
        })
      })
      .collect();
    ensure_that(
      walks,
      "generation and every shrink state preserve the filter contract",
      |observed| {
        observed
          .iter()
          .all(|walk| walk.as_ref().is_ok_and(|reached| reached.1.iter().all(&accepts)))
      },
    )
    .map_err(Box::new)
  }

  #[test]
  fn test_filter() -> Result<(), impl fmt::Debug> {
    let input = (0..256_i32).prop_filter("%3", |&candidate| 0 == candidate.rem_euclid(3));
    check_filtered_shrinking(&input, |value| value.rem_euclid(3) == 0).map(drop)
  }

  #[test]
  fn test_filter_sanity() -> Result<(), Reason> {
    check_strategy_sanity(
      (0..256_i32).prop_filter("!%5", |&candidate| 0 != candidate.rem_euclid(5)),
      Some(CheckStrategySanityOptions {
        // Due to internal rejection sampling, `simplify()` can
        // converge back to what `complicate()` would do.
        strict_complicate_after_simplify: false,
        ..CheckStrategySanityOptions::default()
      }),
    )
  }

  #[test]
  fn both_filter_forms_stop_at_the_local_rejection_limit() -> Result<(), impl fmt::Debug> {
    #[derive(Clone, Copy, Debug)]
    struct Reject;

    impl statics::FilterFn<i32> for Reject {
      fn apply(&self, _: &i32) -> bool {
        false
      }
    }

    let config = Config {
      max_local_rejects: 0,
      failure_persistence: None,
      ..Config::default()
    };
    let mut closure_runner = TestRunner::new(config.clone());
    let mut static_runner = TestRunner::new(config);
    let closure_filter = Just(1_i32).prop_filter("closure rejection", |_| false);
    let static_filter = statics::Filter::new(Just(1_i32), "static rejection".into(), Reject);
    let closure_result = closure_filter.new_tree(&mut closure_runner);
    let static_result = static_filter.new_tree(&mut static_runner);
    ensure_that(
      (
        (closure_runner, closure_filter, closure_result),
        (static_runner, static_filter, static_result),
      ),
      "both predicate representations return the runner's rejection-budget failure",
      |observed| {
        observed
          .0
          .2
          .as_ref()
          .is_err_and(|error| error.message() == "Too many local rejects")
          && observed
            .1
            .2
            .as_ref()
            .is_err_and(|error| error.message() == "Too many local rejects")
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}
