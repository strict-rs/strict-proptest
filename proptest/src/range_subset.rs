//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for generating values by taking samples of index ranges.
//!
//! Note that the strategies in this module are not native combinators; that
//! is, the input range is not itself a strategy, but is rather fixed when
//! the strategy is created.

use core::error::Error;
use core::fmt;
use core::hash::Hash;
use core::ops::Range;

use rand::RngExt as _;

use crate::bits::BitSetLike as _;
use crate::bits::BitSetValueTree;
use crate::bits::VarBitSet;
use crate::collection::EmptySizeRange;
use crate::num::sample_uniform_incl;
use crate::sample::SizeRange;
use crate::std_facade::HashMap;
use crate::std_facade::Vec;
use crate::std_facade::string::ToString as _;
use crate::strategy::Strategy;
use crate::strategy::ValueTree;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::test_runner::Reason;
use crate::test_runner::TestRunner;

/// Sample subsets whose size are within `size` from the given `range`.
///
/// This is roughly analogous to `rand::sample`, except that it samples _without_ replacement.
pub fn range_subset<T>(range: Range<T>, size: impl Into<SizeRange>) -> RangeSubset<T>
where
  T: Copy + Ord + fmt::Debug,
  Range<T>: ExactSizeIterator<Item = T>,
{
  RangeSubset {
    range,
    size: size.into(),
  }
}

/// Error returned by [`try_range_subset`] when the requested size range
/// cannot select a subset of the index range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeSubsetError {
  /// The requested size range is empty.
  EmptySizeRange(EmptySizeRange),
  /// The requested maximum subset size exceeds the range length.
  TooLarge {
    /// Inclusive maximum of the requested size range.
    size_end_incl: usize,
    /// Length of the input range.
    len:           usize,
  },
}

impl fmt::Display for RangeSubsetError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match *self {
      Self::EmptySizeRange(inner) => inner.fmt(f),
      Self::TooLarge {
        size_end_incl,
        len,
      } => write!(f, "Maximum size of subset {size_end_incl} exceeds length of input {len}"),
    }
  }
}

impl Error for RangeSubsetError {}

/// Fallible form of [`range_subset`]: returns a typed error instead of
/// panicking when `size` is an empty range or exceeds the range length.
///
/// ## Errors
///
/// Returns `RangeSubsetError::EmptySizeRange` when `size` is a zero-length
/// range, or `RangeSubsetError::TooLarge` when the inclusive maximum of
/// `size` exceeds the number of elements in `range`.
#[allow(
  clippy::single_call_fn,
  reason = "validate size and range bounds, then build the fallible RangeSubset strategy"
)]
pub fn try_range_subset<T>(range: Range<T>, size: impl Into<SizeRange>) -> Result<RangeSubset<T>, RangeSubsetError>
where
  T: Copy + Ord + fmt::Debug,
  Range<T>: ExactSizeIterator<Item = T>,
{
  let len = range.len();
  let size_range = size.into();

  size_range.ensure_nonempty().map_err(RangeSubsetError::EmptySizeRange)?;
  if size_range.end_incl() > len {
    return Err(RangeSubsetError::TooLarge {
      size_end_incl: size_range.end_incl(),
      len,
    });
  }
  Ok(RangeSubset {
    range,
    size: size_range,
  })
}

/// Strategy to generate `Vec`s by sampling a subset from an index range.
///
/// This is created by the `range_subset` function in the same module.
#[derive(Debug)]
pub struct RangeSubset<T> {
  /// Index range each generated subset is sampled from, without
  /// replacement.
  range: Range<T>,
  /// Bounds on how many indices a generated subset contains.
  size:  SizeRange,
}

impl<T> Strategy for RangeSubset<T>
where
  T: Copy + Eq + Hash + fmt::Debug,
  Range<T>: ExactSizeIterator<Item = T>,
{
  type Tree = RangeSubsetValueTree<T>;
  type Value = Vec<T>;

  fn new_tree(&self, runner: &mut TestRunner) -> Result<Self::Tree, Reason> {
    self.size.ensure_nonempty().map_err(|error| Reason::from(error.to_string()))?;
    let range_len = self.range.len();
    if self.size.end_incl() > range_len {
      return Err(Reason::from(
        RangeSubsetError::TooLarge {
          size_end_incl: self.size.end_incl(),
          len:           range_len,
        }
        .to_string(),
      ));
    }
    let (min_size, max_size) = (self.size.start(), self.size.end_incl());

    let count = sample_uniform_incl(runner, min_size, max_size)?;

    let mut swaps: HashMap<T, T> = HashMap::default();

    let mut values: Vec<T> = Vec::default();

    let rng = runner.rng();

    // # Performance
    //
    // Thanks to specialization this `O(n)` access of `range.nth(…)` ends up being `O(1)`.

    // # Safety
    //
    // The offsets `i`/`j` get sampled from `0..count`/`0..range.len()`,
    // (where `0..count` is shorter, or equal in length to `0..range.len()`)
    // so unwrapping `range.nth(i).unwrap()` is safe:

    // Apply a Fisher-Yates shuffle:
    for i in 0..count {
      let j: usize = rng.random_range(i..range_len);

      let iv = self
        .range
        .clone()
        .nth(i)
        .ok_or_else(|| Reason::from("range_subset sampled source index out of range"))?;
      let vi = *swaps.get(&iv).unwrap_or(&iv);

      let jv = self
        .range
        .clone()
        .nth(j)
        .ok_or_else(|| Reason::from("range_subset sampled swap index out of range"))?;
      let vj = *swaps.get(&jv).unwrap_or(&jv);

      let _previous_i = swaps.insert(iv, vj);
      let _previous_j = swaps.insert(jv, vi);
      values.push(vj);
    }

    let included_values = BitSetValueTree::new(VarBitSet::saturated(count), min_size, 0);

    Ok(RangeSubsetValueTree {
      values,
      included_values,
    })
  }
}

/// `RangeSubsetValueTree` corresponding to `RangeSubset`.
#[derive(Debug, Clone)]
pub struct RangeSubsetValueTree<T> {
  /// Sampled indices in the order the Fisher-Yates shuffle drew them.
  values:          Vec<T>,
  /// The current selection and its minimum-size-aware shrink state.
  included_values: BitSetValueTree<VarBitSet>,
}

impl<T> ValueTree for RangeSubsetValueTree<T>
where
  T: Copy + Eq + Hash + fmt::Debug,
  Range<T>: ExactSizeIterator<Item = T>,
{
  type Value = Vec<T>;

  fn current(&self) -> Self::Value {
    self
      .values
      .iter()
      .enumerate()
      .filter_map(|(index, sampled)| self.included_values.bits().test(index).then_some(*sampled))
      .collect()
  }

  fn simplify(&mut self) -> bool {
    self.included_values.simplify()
  }

  fn complicate(&mut self) -> bool {
    self.included_values.complicate()
  }
}

#[cfg(test)]
mod test {
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::sample::subset_frequencies;
  use crate::std_facade::BTreeSet;
  use crate::strategy::trace_shrink_steps;

  /// An assertion retains its complete concrete input.
  type Check<S> = Result<(), PredicateFailure<S>>;

  /// Both invalid range-subset constructor requests.
  type InvalidRequests = [Result<RangeSubset<usize>, RangeSubsetError>; 2];

  /// Native generated subsets, preserving generation failures.
  type SubsetSamples = Vec<Result<Vec<usize>, Reason>>;
  /// Fallible construction followed by the native generation result.
  type ConstructedSubset = Result<(RangeSubset<usize>, Result<Vec<usize>, Reason>), RangeSubsetError>;

  /// A complete shrink walk, stop attempt, and one-step restoration.
  type SubsetShrink = (RangeSubsetValueTree<usize>, Vec<Vec<usize>>, bool, bool, Vec<usize>, bool);

  #[test]
  fn shrinking_respects_minimum_size_and_restores_last_removal() -> Check<Vec<Result<SubsetShrink, Reason>>> {
    let mut runner = TestRunner::deterministic();
    let strategy = range_subset(0_usize..8, 3..7);
    let observations = (0..64)
      .map(|_| {
        strategy.new_tree(&mut runner).map(|generated| {
          let (mut tree, values) = trace_shrink_steps(generated);
          let stopped = tree.simplify();
          let restored = tree.complicate();
          let restored_values = tree.current();
          let restored_again = tree.complicate();
          (tree, values, stopped, restored, restored_values, restored_again)
        })
      })
      .collect();
    let respects_minimum = |sample: &Result<SubsetShrink, Reason>| {
      let Ok(ref reached) = *sample else {
        return false;
      };
      let expected_previous = reached.1.iter().nth_back(1);
      reached.1.last().is_some_and(|last| last.len() == 3)
        && reached.1.array_windows::<2>().all(|pair| {
          let [before, after] = pair.each_ref();
          before.get(1..) == Some(after.as_slice())
        })
        && !reached.2
        && reached.3 == expected_previous.is_some()
        && expected_previous.or_else(|| reached.1.last()) == Some(&reached.4)
        && !reached.5
        && reached.0.current() == reached.4
    };
    ensure_that(
      observations,
      "subsets stop at their minimum size and can restore only the last removed element",
      |observed: &Vec<Result<SubsetShrink, Reason>>| observed.iter().all(respects_minimum),
    )
    .map(drop)
  }

  #[test]
  fn sample_range() -> Check<SubsetSamples> {
    let mut runner = TestRunner::deterministic();
    let input = range_subset(0_usize..8, 3..7);
    let samples: SubsetSamples = (0..2048)
      .map(|_| input.new_tree(&mut runner).map(|tree| tree.current()))
      .collect();
    let valid_subset = |sample: &Result<Vec<usize>, Reason>| {
      let Ok(ref values) = *sample else {
        return false;
      };
      (3..7).contains(&values.len())
        && values.iter().all(|value| (0..8).contains(value))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
    };
    ensure_that(
      samples,
      "subsets preserve distinct in-range indices and the requested size and sampling frequencies",
      |observed| observed.iter().all(valid_subset) && subset_frequencies(observed, 3..7, 0..8, 256..1024, 1024..1500),
    )
    .map(drop)
  }

  #[test]
  fn test_sample_sanity() -> Result<(), Reason> {
    check_strategy_sanity(range_subset(0..5, 1..3), None)
  }

  #[test]
  fn try_range_subset_accepts_a_valid_request() -> Check<ConstructedSubset> {
    let generated = try_range_subset(0_usize..8, 3..7).map(|strategy| {
      let value = strategy.new_tree(&mut TestRunner::deterministic()).map(|tree| tree.current());
      (strategy, value)
    });
    ensure_that(
      generated,
      "a valid constructed strategy generates subsets within its size range",
      |result| {
        result
          .as_ref()
          .is_ok_and(|reached| reached.1.as_ref().is_ok_and(|values| (3..7).contains(&values.len())))
      },
    )
    .map(drop)
  }

  #[test]
  fn try_range_subset_rejects_invalid_requests() -> Check<InvalidRequests> {
    ensure_that(
      [try_range_subset(0_usize..8, 2..2), try_range_subset(0_usize..3, 1..=9)],
      "invalid size requests retain distinct empty and oversized diagnostics",
      |results| {
        matches!(*results, [
          Err(RangeSubsetError::EmptySizeRange(_)),
          Err(RangeSubsetError::TooLarge {
            size_end_incl: 9,
            len:           3,
          })
        ])
      },
    )
    .map(drop)
  }

  #[test]
  fn subset_empty_range_works() -> Check<Result<Vec<usize>, Reason>> {
    let result = range_subset(0_usize..0, 0..1)
      .new_tree(&mut TestRunner::deterministic())
      .map(|tree| tree.current());
    ensure_that(result, "an empty index range yields the empty subset", |observed| {
      observed.as_ref().is_ok_and(Vec::is_empty)
    })
    .map(drop)
  }

  #[test]
  fn subset_full_range_works() -> Check<Result<Vec<usize>, Reason>> {
    let result = range_subset(1_usize..4, 3)
      .new_tree(&mut TestRunner::deterministic())
      .map(|tree| tree.current());
    ensure_that(result, "a full-width subset covers the whole range", |observed| {
      observed
        .as_ref()
        .is_ok_and(|values| values.len() == 3 && values.iter().copied().collect::<BTreeSet<_>>() == (1..4).collect())
    })
    .map(drop)
  }
}
