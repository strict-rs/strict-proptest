//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::mem;

use crate::std_facade::Arc;
use crate::std_facade::fmt;
use crate::strategy::fuse::Fuse;
use crate::strategy::traits::NewTree;
use crate::strategy::traits::Strategy;
use crate::strategy::traits::ValueTree;
use crate::test_runner::Reason;
use crate::test_runner::TestRunner;
use crate::tuple::TupleValueTree;

/// Adaptor that flattens a `Strategy` which produces other `Strategy`s into a
/// `Strategy` that picks one of those strategies and then picks values from
/// it.
#[derive(Debug, Clone, Copy)]
#[must_use = "strategies do nothing unless used"]
pub struct Flatten<S> {
  /// The strategy whose generated values are themselves strategies to be
  /// flattened.
  source: S,
}

impl<S: Strategy> Flatten<S> {
  /// Wrap `source` to flatten it.
  #[allow(
    clippy::single_call_fn,
    reason = "wrap a strategy-producing source so prop_flat_map can flatten its output"
  )]
  pub const fn new(source: S) -> Self {
    Self {
      source,
    }
  }
}

impl<S: Strategy> Strategy for Flatten<S>
where
  S::Value: Strategy,
{
  type Tree = FlattenValueTree<S::Tree>;
  type Value = <S::Value as Strategy>::Value;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    let meta = self.source.new_tree(runner)?;
    FlattenValueTree::new(runner, meta)
  }
}

/// The `ValueTree` produced by `Flatten`.
pub struct FlattenValueTree<S: ValueTree>
where
  S::Value: Strategy,
{
  /// The outer, strategy-producing value tree; shrinking it selects a
  /// different inner strategy to draw from.
  meta: Fuse<S>,
  /// The value tree derived from the currently chosen inner strategy, and the
  /// one `current()` actually reads.
  current: Fuse<<S::Value as Strategy>::Tree>,
  /// The value to fall back to once successive `complicate()` calls on the
  /// underlying trees have all returned `false`.
  final_complication: Option<Fuse<<S::Value as Strategy>::Tree>>,
  // When `simplify()` or `complicate()` causes a new `Strategy` to be
  // chosen, we need to find a new failing input for that case. To do this,
  // we implement `complicate()` by regenerating values up to a number of
  // times corresponding to the maximum number of test cases. A `simplify()`
  // which does not cause a new strategy to be chosen always resets
  // `complicate_regen_remaining` to 0.
  //
  // This does unfortunately depart from the direct interpretation of
  // simplify/complicate as binary search, but is still easier to think about
  // than other implementations of higher-order strategies.
  /// A private clone of the runner used to regenerate inner value trees when
  /// shrinking switches to a new inner strategy.
  runner: TestRunner,
  /// How many more times `complicate()` may regenerate the inner tree while
  /// hunting for a new failing input; seeded from `Config::cases`.
  complicate_regen_remaining: u32,
}

impl<S> Clone for FlattenValueTree<S>
where
  S: ValueTree + Clone,
  S::Value: Strategy + Clone,
  <S::Value as Strategy>::Tree: Clone,
{
  fn clone(&self) -> Self {
    Self {
      meta: self.meta.clone(),
      current: self.current.clone(),
      final_complication: self.final_complication.clone(),
      runner: self.runner.clone(),
      complicate_regen_remaining: self.complicate_regen_remaining,
    }
  }
}

impl<S> fmt::Debug for FlattenValueTree<S>
where
  S: ValueTree + fmt::Debug,
  S::Value: Strategy,
  <S::Value as Strategy>::Tree: fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("FlattenValueTree")
      .field("meta", &self.meta)
      .field("current", &self.current)
      .field("final_complication", &self.final_complication)
      .field("runner", &self.runner)
      .field("complicate_regen_remaining", &self.complicate_regen_remaining)
      .finish()
  }
}

impl<S: ValueTree> FlattenValueTree<S>
where
  S::Value: Strategy,
{
  /// Build the flattened value tree from the outer tree `meta`, generating
  /// the first inner tree from its current value.
  ///
  /// # Errors
  ///
  /// Returns the failure `Reason` if generating that first inner tree fails.
  #[allow(
    clippy::single_call_fn,
    reason = "grow the flattened value tree from the outer value's first inner tree"
  )]
  fn new(runner: &mut TestRunner, meta: S) -> Result<Self, Reason> {
    let current = meta.current().new_tree(runner)?;
    Ok(Self {
      meta: Fuse::new(meta),
      current: Fuse::new(current),
      final_complication: None,
      runner: runner.partial_clone(),
      complicate_regen_remaining: 0,
    })
  }
}

impl<S: ValueTree> ValueTree for FlattenValueTree<S>
where
  S::Value: Strategy,
{
  type Value = <S::Value as Strategy>::Value;

  fn current(&self) -> Self::Value {
    self.current.current()
  }

  fn simplify(&mut self) -> bool {
    self.complicate_regen_remaining = 0;

    if self.current.simplify() {
      // Now that we've simplified the derivative value, we can't
      // re-complicate the meta value unless it gets simplified again.
      // We also mustn't complicate back to whatever's in
      // `final_complication` since the new state of `self.current` is
      // the most complicated state.
      self.meta.disallow_complicate();
      self.final_complication = None;
      true
    } else if !self.meta.simplify() {
      false
    } else if let Ok(tree) = self.meta.current().new_tree(&mut self.runner) {
      // Shift current into final_complication and `tree` into
      // `current`. We also need to prevent that value from
      // complicating beyond the current point in the future
      // since we're going to return `true` from `simplify()`
      // ourselves.
      self.current.disallow_complicate();
      let mut final_complication = Fuse::new(tree);
      mem::swap(&mut final_complication, &mut self.current);
      self.final_complication = Some(final_complication);
      // Initially complicate by regenerating the chosen value.
      self.complicate_regen_remaining = self.runner.config().cases;
      true
    } else {
      false
    }
  }

  fn complicate(&mut self) -> bool {
    // The regen budget is only consulted (and the runner-wide budget
    // only charged) while regeneration complications remain.
    if self.complicate_regen_remaining > 0 && !self.runner.flat_map_regen() {
      self.complicate_regen_remaining = 0;
    }
    if self.complicate_regen_remaining > 0 {
      self.complicate_regen_remaining = self.complicate_regen_remaining.saturating_sub(1);

      if let Ok(tree) = self.meta.current().new_tree(&mut self.runner) {
        self.current = Fuse::new(tree);
        return true;
      }
    }

    if self.current.complicate() {
      return true;
    }

    if self.meta.complicate()
      && let Ok(tree) = self.meta.current().new_tree(&mut self.runner)
    {
      self.complicate_regen_remaining = self.runner.config().cases;
      self.current = Fuse::new(tree);
      return true;
    }

    if let Some(tree) = self.final_complication.take() {
      self.current = tree;
      true
    } else {
      false
    }
  }
}

/// Similar to `Flatten`, but does not shrink the input strategy.
///
/// See `Strategy::prop_ind_flat_map()` fore more details.
#[derive(Clone, Copy, Debug)]
pub struct IndFlatten<S>(pub(super) S);

impl<S: Strategy> Strategy for IndFlatten<S>
where
  S::Value: Strategy,
{
  type Tree = <S::Value as Strategy>::Tree;
  type Value = <S::Value as Strategy>::Value;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    let inner = self.0.new_tree(runner)?;
    inner.current().new_tree(runner)
  }
}

/// Similar to `Map` plus `Flatten`, but does not shrink the input strategy and
/// passes the original input through.
///
/// See `Strategy::prop_ind_flat_map2()` for more details.
pub struct IndFlattenMap<S, F> {
  /// The strategy generating the input value passed through in slot 0.
  pub(super) source: S,
  /// The closure deriving the slot-1 strategy from that input, held behind
  /// an `Arc` so the wrapper clones cheaply.
  pub(super) fun:    Arc<F>,
}

impl<S: fmt::Debug, F> fmt::Debug for IndFlattenMap<S, F> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("IndFlattenMap")
      .field("source", &self.source)
      .field("fun", &"<function>")
      .finish()
  }
}

impl<S: Clone, F> Clone for IndFlattenMap<S, F> {
  fn clone(&self) -> Self {
    Self {
      source: self.source.clone(),
      fun:    Arc::clone(&self.fun),
    }
  }
}

impl<S: Strategy, R: Strategy, F: Fn(S::Value) -> R> Strategy for IndFlattenMap<S, F> {
  type Tree = TupleValueTree<(S::Tree, R::Tree)>;
  type Value = (S::Value, R::Value);

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    let left = self.source.new_tree(runner)?;
    let right_source = (self.fun)(left.current());
    let right = right_source.new_tree(runner)?;

    Ok(TupleValueTree::new((left, right)))
  }
}

#[cfg(test)]
mod test {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_some;

  use super::*;
  use crate::strategy::check_strategy_sanity;
  use crate::strategy::just::Just;
  use crate::test_runner::Config;
  use crate::test_runner::RngAlgorithm;
  use crate::test_runner::TestCaseError;
  use crate::test_runner::TestError;
  use crate::test_runner::TestRng;
  use crate::test_runner::runner_test_config;

  #[test]
  fn test_flat_map() -> Result<(), TestFailure> {
    // Pick random integer A, then random integer B which is ±5 of A and
    // assert that B <= A if A > 10000. Shrinking should always converge to
    // A=10001, B=10002.
    let input = (0..65536).prop_flat_map(|first| (Just(first), (first - 5..first + 5)));

    let mut failures = 0;
    let mut runner = TestRunner::new_with_rng(
      Config {
        max_shrink_iters: u32::MAX - 1,
        ..runner_test_config()
      },
      TestRng::deterministic_rng(RngAlgorithm::default()),
    );
    for _ in 0..1000 {
      let case = ensure_some(input.new_tree(&mut runner).ok(), "flat_map strategy generates a value tree")?;
      let result = runner.run_one(case, |(first, second)| match (first, second) {
        (left, right) if left <= 10000 || right <= left => Ok(()),
        _ => Err(TestCaseError::fail("fail")),
      });

      match result {
        Ok(_) => {}
        Err(TestError::Fail(_, falsified)) => {
          failures += 1;
          ensure((10001, 10002) == falsified, "shrinking converges to the minimal dependent pair")?;
        }
        _ => ensure(false, "run_one yields either a success or a failed case")?,
      }
    }

    ensure(failures > 250, "enough cases falsified")
  }

  #[test]
  fn test_flat_map_sanity() -> Result<(), Reason> {
    check_strategy_sanity((0..65536).prop_flat_map(|first| (Just(first), (first - 5..first + 5))), None)
  }

  #[test]
  fn flat_map_respects_regen_limit() -> Result<(), TestFailure> {
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    let input = (0..65536)
      .prop_flat_map(|_| 0..65536)
      .prop_flat_map(|_| 0..65536)
      .prop_flat_map(|_| 0..65536)
      .prop_flat_map(|_| 0..65536)
      .prop_flat_map(|_| 0..65536);

    // Arteficially make the first case fail and all others pass, so that
    // the regeneration logic futilely searches for another failing
    // example and eventually gives up. Unfortunately, the test is sort of
    // semi-decidable; if the limit *doesn't* work, the test just runs
    // almost forever.
    let pass = AtomicBool::new(false);
    let mut runner = TestRunner::new(Config {
      max_flat_map_regens: 1000,
      ..runner_test_config()
    });
    let case = ensure_some(input.new_tree(&mut runner).ok(), "nested flat_map strategy generates a value tree")?;
    let outcome = runner.run_one(case, |_| {
      // Only the first run fails, all others succeed
      if pass.fetch_or(true, Ordering::SeqCst) {
        Ok(())
      } else {
        Err(TestCaseError::fail("first case fails by design"))
      }
    });
    ensure(
      outcome.is_err(),
      "the deliberately-failing first case makes the bounded regen search terminate with a failure",
    )
  }

  #[test]
  fn test_ind_flat_map_sanity() -> Result<(), Reason> {
    check_strategy_sanity((0..65536).prop_ind_flat_map(|first| (Just(first), (first - 5..first + 5))), None)
  }

  #[test]
  fn test_ind_flat_map2_sanity() -> Result<(), Reason> {
    check_strategy_sanity((0..65536).prop_ind_flat_map2(|first| first - 5..first + 5), None)
  }
}
