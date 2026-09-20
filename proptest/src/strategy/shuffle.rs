//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::mem::swap;

use rand::RngExt as _;

use crate::num;
use crate::std_facade::Cell;
use crate::std_facade::Vec;
use crate::std_facade::VecDeque;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::traits::NewTree;
use crate::strategy::traits::Strategy;
use crate::strategy::traits::ValueTree;
use crate::test_runner::TestRng;
use crate::test_runner::TestRunner;

/// `Strategy` shuffle adaptor.
///
/// See `Strategy::prop_shuffle()`.
#[derive(Clone, Debug)]
#[must_use = "strategies do nothing unless used"]
pub struct Shuffle<S>(pub(super) S);

/// A value which can be used with the `prop_shuffle` combinator.
///
/// This is not a general-purpose trait. Its methods are prefixed with
/// `shuffle_` to avoid the compiler suggesting them or this trait as
/// corrections in errors.
pub trait Shuffleable {
  /// Return the length of this collection.
  fn shuffle_len(&self) -> usize;
  /// Swap the elements at the given indices.
  fn shuffle_swap(&mut self, first_index: usize, second_index: usize);
}

/// Swap the elements at `first_index` and `second_index` when both indices
/// are in bounds; an out-of-bounds pair leaves the slice untouched. Callers
/// draw indices below `shuffle_len()`, so the untouched arm is unreachable
/// in normal operation.
fn swap_if_in_bounds<T>(slice: &mut [T], first_index: usize, second_index: usize) {
  if first_index == second_index {
    return;
  }
  let (lo, hi) = if first_index < second_index {
    (first_index, second_index)
  } else {
    (second_index, first_index)
  };
  if let Some((head, tail)) = slice.split_at_mut_checked(hi)
    && let (Some(first), Some(second)) = (head.get_mut(lo), tail.first_mut())
  {
    swap(first, second);
  }
}

/// Implement `Shuffleable` for a slice- or array-like type by delegating
/// `shuffle_len` to `len()` and `shuffle_swap` to the bounds-guarded
/// `swap_if_in_bounds`.
macro_rules! shuffleable {
    ($($t:tt)*) => {
        impl<T> Shuffleable for $($t)* {
            fn shuffle_len(&self) -> usize {
                self.len()
            }

            fn shuffle_swap(&mut self, first_index: usize, second_index: usize) {
                swap_if_in_bounds(self, first_index, second_index);
            }
        }
    }
}

shuffleable!([T]);
shuffleable!(Vec<T>);

impl<T> Shuffleable for VecDeque<T> {
  fn shuffle_len(&self) -> usize {
    self.len()
  }

  /// `VecDeque::swap` panics out of bounds, so both indices are guarded
  /// first; callers draw indices below `shuffle_len()`.
  fn shuffle_swap(&mut self, first_index: usize, second_index: usize) {
    if first_index < self.len() && second_index < self.len() {
      self.swap(first_index, second_index);
    }
  }
}
// Zero- and 1-length arrays aren't usefully shuffleable, but are included to
// simplify external macros that may try to use them anyway.
shuffleable!([T; 0]);
shuffleable!([T; 1]);
shuffleable!([T; 2]);
shuffleable!([T; 3]);
shuffleable!([T; 4]);
shuffleable!([T; 5]);
shuffleable!([T; 6]);
shuffleable!([T; 7]);
shuffleable!([T; 8]);
shuffleable!([T; 9]);
shuffleable!([T; 10]);
shuffleable!([T; 11]);
shuffleable!([T; 12]);
shuffleable!([T; 13]);
shuffleable!([T; 14]);
shuffleable!([T; 15]);
shuffleable!([T; 16]);
shuffleable!([T; 17]);
shuffleable!([T; 18]);
shuffleable!([T; 19]);
shuffleable!([T; 20]);
shuffleable!([T; 21]);
shuffleable!([T; 22]);
shuffleable!([T; 23]);
shuffleable!([T; 24]);
shuffleable!([T; 25]);
shuffleable!([T; 26]);
shuffleable!([T; 27]);
shuffleable!([T; 28]);
shuffleable!([T; 29]);
shuffleable!([T; 30]);
shuffleable!([T; 31]);
shuffleable!([T; 32]);

impl<S: Strategy> Strategy for Shuffle<S>
where
  S::Value: Shuffleable,
{
  type Tree = ShuffleValueTree<S::Tree>;
  type Value = S::Value;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    let rng = runner.new_rng();

    self.0.new_tree(runner).map(|inner| ShuffleValueTree {
      inner,
      rng,
      dist: Cell::new(None),
      simplifying_inner: false,
    })
  }
}

/// `ValueTree` shuffling adaptor.
///
/// See `Strategy::prop_shuffle()`.
#[derive(Clone, Debug)]
pub struct ShuffleValueTree<V> {
  /// The source value tree producing the collection to be shuffled.
  inner:             V,
  /// The generator driving the swap pass; cloned on every `current()` so the
  /// permutation is reproduced identically each time.
  rng:               TestRng,
  /// The maximum amount to move any one element during shuffling.
  ///
  /// This is `Cell` since we can't determine the bounds of the value until
  /// the first call to `current()`. (We technically _could_ by generating a
  /// value in `new_tree` and checking its length, but that would be a 100%
  /// slowdown.)
  dist:              Cell<Option<num::usize::BinarySearch>>,
  /// Whether we've started simplifying `inner`. After this point, we can no
  /// longer simplify or complicate `dist`.
  simplifying_inner: bool,
}

impl<V: ValueTree> ShuffleValueTree<V>
where
  V::Value: Shuffleable,
{
  /// Lazily initialise `dist` to a binary search seeded with `dflt` if it is
  /// not set yet.
  fn ensure_dist_initialized(&self, dflt: usize) {
    if self.dist.get().is_none() {
      self.dist.set(Some(num::usize::BinarySearch::new(dflt)));
    }
  }

  /// Force `dist` to be initialised from the current value's length so that
  /// later shrink calls behave consistently even when invoked out of order.
  fn force_init_dist(&self) {
    if self.dist.get().is_none() {
      self.ensure_dist_initialized(self.current().shuffle_len());
    }
  }
}

impl<V: ValueTree> ValueTree for ShuffleValueTree<V>
where
  V::Value: Shuffleable,
{
  type Value = V::Value;

  fn current(&self) -> V::Value {
    let mut permuted = self.inner.current();
    let len = permuted.shuffle_len();
    // The maximum distance to swap elements. This could be larger than
    // the permuted collection if it has reduced size during shrinking;
    // that's OK, since we only use this to filter swaps.
    self.ensure_dist_initialized(len);
    let Some(max_swap) = self.dist.get().map(|dist| dist.current()) else {
      return permuted;
    };

    // If empty collection or all swaps will be filtered out, there's
    // nothing to shuffle.
    if 0 == len || 0 == max_swap {
      return permuted;
    }

    let mut rng = self.rng.clone();

    for start_index in 0..len.saturating_sub(1) {
      // Determine the other index to be swapped, then skip the swap if
      // it is too far. This ordering is critical, as it ensures that we
      // generate the same sequence of random numbers every time.
      let end_index = rng.random_range(start_index..len);
      if end_index.saturating_sub(start_index) <= max_swap {
        permuted.shuffle_swap(start_index, end_index);
      }
    }

    permuted
  }

  fn simplify(&mut self) -> bool {
    if self.simplifying_inner {
      self.inner.simplify()
    } else {
      // Ensure that we've initialised `dist` to *something* to give
      // consistent non-panicking behaviour even if called in an
      // unexpected sequence.
      self.force_init_dist();
      let Some(dist) = self.dist.get_mut().as_mut() else {
        self.simplifying_inner = true;
        return self.inner.simplify();
      };
      if dist.simplify() {
        true
      } else {
        self.simplifying_inner = true;
        self.inner.simplify()
      }
    }
  }

  fn complicate(&mut self) -> bool {
    if self.simplifying_inner {
      self.inner.complicate()
    } else {
      self.force_init_dist();
      self.dist.get_mut().as_mut().is_some_and(ValueTree::complicate)
    }
  }
}

#[cfg(test)]
mod test {
  use core::array::from_fn;
  use core::cmp::Reverse;
  use std::borrow::ToOwned as _;
  use std::collections::BTreeSet;
  use std::collections::HashSet;

  use strict_test_support::ComparisonFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::collection;
  use crate::std_facade::Box;
  use crate::strategy::just::Just;
  use crate::strategy::trace_shrink_steps;
  use crate::test_runner::Reason;
  use crate::test_runner::test_runner_without_persistence;

  /// Each permutation's native generation result.
  type Permutations = Vec<Result<Vec<i32>, Reason>>;
  /// Native before/after array comparisons at the swap boundary.
  type SwapOutcome<const WIDTH: usize, const CASES: usize> = Result<(), ComparisonFailure<[[i32; WIDTH]; CASES], [[i32; WIDTH]; CASES]>>;
  /// Deque states reached after one valid and two invalid swaps.
  type DequeSnapshots = [VecDeque<i32>; 3];

  static VALUES: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19];
  /// Shuffle trees together with the permutations and distances observed during shrinking.
  type ShuffleWalks = Vec<Result<ShuffleWalk, Reason>>;
  /// Reached shuffle tree with each permutation and its displacement.
  type ShuffleWalk = (ShuffleValueTree<Just<Vec<i32>>>, Vec<(Vec<i32>, u32)>);

  #[test]
  fn generates_different_permutations() -> Result<(), PredicateFailure<Permutations>> {
    let mut runner = test_runner_without_persistence();
    let input = Just(VALUES.to_owned()).prop_shuffle();
    let samples: Vec<_> = (0..1024)
      .map(|_| input.new_tree(&mut runner).map(|tree| tree.current()))
      .collect();
    ensure_that(
      samples,
      "all 1024 permutations are distinct and preserve the original elements",
      |observed| {
        observed.iter().all(|sample| {
          sample
            .as_ref()
            .is_ok_and(|values| values.len() == VALUES.len() && values.iter().collect::<BTreeSet<_>>() == VALUES.iter().collect())
        }) && observed
          .iter()
          .filter_map(|sample| sample.as_ref().ok())
          .collect::<HashSet<_>>()
          .len()
          == 1024
      },
    )
    .map(drop)
  }

  /// Observe a permutation and its total element displacement together.
  #[allow(
    clippy::single_call_fn,
    reason = "displacement measurement is independent of the shrink traversal and its monotonicity assertion"
  )]
  fn shuffle_distance(value: Vec<i32>) -> (Vec<i32>, u32) {
    let distance = value.iter().enumerate().fold(0_u32, |total, (index, nominal)| {
      total.saturating_add(nominal.abs_diff(i32::try_from(index).unwrap_or(i32::MAX)))
    });
    (value, distance)
  }

  #[test]
  fn simplify_reduces_shuffle_amount() -> Result<(), PredicateFailure<ShuffleWalks>> {
    let mut runner = test_runner_without_persistence();
    let input = Just(VALUES.to_owned()).prop_shuffle();
    let walks: ShuffleWalks = (0..1024)
      .map(|_| input.new_tree(&mut runner))
      .map(|generation| {
        generation.map(|initial_tree| {
          let (tree, values) = trace_shrink_steps(initial_tree);
          let observations = values.into_iter().map(shuffle_distance).collect();
          (tree, observations)
        })
      })
      .collect();
    let converges_to_source = |walk: &Result<ShuffleWalk, Reason>| {
      let Ok(ref reached) = *walk else {
        return false;
      };
      reached.1.iter().map(|step| Reverse(step.1)).is_sorted()
        && reached.1.last().is_some_and(|step| step.1 == 0 && step.0 == VALUES)
        && reached.0.current() == VALUES
    };
    ensure_that(
      walks,
      "each simplification reduces displacement and ultimately restores source order",
      |observed| observed.iter().all(converges_to_source),
    )
    .map(drop)
  }

  #[test]
  fn simplify_complicate_contract_upheld() -> Result<(), Reason> {
    check_strategy_sanity(collection::vec(0_i32..1000, 5..10).prop_shuffle(), None)
  }

  #[test]
  fn swap_if_in_bounds_swaps_in_bounds_pairs() -> SwapOutcome<4, 2> {
    let mut values = [1, 2, 3, 4];
    swap_if_in_bounds(&mut values, 0, 3);
    let swapped = values;
    swap_if_in_bounds(&mut values, 2, 2);
    ensure_eq(
      [swapped, values],
      [[4, 2, 3, 1]; 2],
      "in-bounds pairs swap, while equal indices preserve the slice",
    )
    .map(drop)
  }

  #[test]
  fn swap_if_in_bounds_ignores_out_of_bounds_pairs() -> SwapOutcome<3, 3> {
    let mut values = [1, 2, 3];
    swap_if_in_bounds(&mut values, 0, 3);
    let first = values;
    swap_if_in_bounds(&mut values, 5, 1);
    let second = values;
    swap_if_in_bounds(&mut values, 9, 9);
    ensure_eq(
      [first, second, values],
      [[1, 2, 3]; 3],
      "each out-of-bounds pair preserves the slice",
    )
    .map(drop)
  }

  #[test]
  fn vec_deque_swap_is_bounds_guarded() -> Result<(), Box<ComparisonFailure<DequeSnapshots, DequeSnapshots>>> {
    let mut deque = VecDeque::from([1, 2, 3]);
    deque.shuffle_swap(0, 2);
    let swapped = deque.clone();
    deque.shuffle_swap(0, 3);
    let first_invalid = deque.clone();
    deque.shuffle_swap(7, 1);
    ensure_eq(
      [swapped, first_invalid, deque],
      from_fn(|_| VecDeque::from([3, 2, 1])),
      "in-bounds deque indices swap and each out-of-bounds pair preserves the result",
    )
    .map(drop)
    .map_err(Box::new)
  }
}
