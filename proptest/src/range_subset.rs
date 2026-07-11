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

use rand::RngExt as _;

use core::error::Error;
use core::fmt;
use core::hash::Hash;
use core::ops::Range;

use crate::bits::{BitSetLike as _, VarBitSet};
use crate::collection::EmptySizeRange;
use crate::num::sample_uniform_incl;
use crate::sample::SizeRange;
use crate::std_facade::{HashMap, Vec, string::ToString as _};
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::{Strategy, ValueTree};
use crate::test_runner::{Reason, TestRunner};

/// Sample subsets whose size are within `size` from the given `range`.
///
/// This is roughly analogous to `rand::sample`, except that it samples _without_ replacement.
///
pub fn range_subset<T>(
    range: Range<T>,
    size: impl Into<SizeRange>,
) -> RangeSubset<T>
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
        len: usize,
    },
}

impl fmt::Display for RangeSubsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::EmptySizeRange(inner) => inner.fmt(f),
            Self::TooLarge { size_end_incl, len } => write!(
                f,
                "Maximum size of subset {size_end_incl} exceeds length of \
                 input {len}"
            ),
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
pub fn try_range_subset<T>(
    range: Range<T>,
    size: impl Into<SizeRange>,
) -> Result<RangeSubset<T>, RangeSubsetError>
where
    T: Copy + Ord + fmt::Debug,
    Range<T>: ExactSizeIterator<Item = T>,
{
    let len = range.len();
    let size_range = size.into();

    size_range
        .ensure_nonempty()
        .map_err(RangeSubsetError::EmptySizeRange)?;
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
    size: SizeRange,
}

impl<T> Strategy for RangeSubset<T>
where
    T: Copy + Eq + Hash + fmt::Debug,
    Range<T>: ExactSizeIterator<Item = T>,
{
    type Tree = RangeSubsetValueTree<T>;
    type Value = Vec<T>;

    fn new_tree(&self, runner: &mut TestRunner) -> Result<Self::Tree, Reason> {
        self.size
            .ensure_nonempty()
            .map_err(|error| Reason::from(error.to_string()))?;
        let range_len = self.range.len();
        if self.size.end_incl() > range_len {
            return Err(Reason::from(
                RangeSubsetError::TooLarge {
                    size_end_incl: self.size.end_incl(),
                    len: range_len,
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

            let iv = self.range.clone().nth(i).ok_or_else(|| {
                Reason::from("range_subset sampled source index out of range")
            })?;
            let vi = *swaps.get(&iv).unwrap_or(&iv);

            let jv = self.range.clone().nth(j).ok_or_else(|| {
                Reason::from("range_subset sampled swap index out of range")
            })?;
            let vj = *swaps.get(&jv).unwrap_or(&jv);

            let _previous_i = swaps.insert(iv, vj);
            let _previous_j = swaps.insert(jv, vi);
            values.push(vj);
        }

        let included_values = VarBitSet::saturated(count);

        Ok(RangeSubsetValueTree {
            values,
            included_values,
            shrink: 0,
            prev_shrink: None,
            min_size,
        })
    }
}

/// `RangeSubsetValueTree` corresponding to `RangeSubset`.
#[derive(Debug, Clone)]
pub struct RangeSubsetValueTree<T> {
    /// Sampled indices in the order the Fisher-Yates shuffle drew them.
    values: Vec<T>,
    /// Which positions in `values` remain part of the current subset.
    included_values: VarBitSet,
    /// Next position in `values` to try excluding while simplifying.
    shrink: usize,
    /// Position excluded by the last `simplify`, restored by `complicate`.
    prev_shrink: Option<usize>,
    /// Lower size bound; shrinking never drops below this many elements.
    min_size: usize,
}

impl<T> ValueTree for RangeSubsetValueTree<T>
where
    T: Copy + Eq + Hash + fmt::Debug,
    Range<T>: ExactSizeIterator<Item = T>,
{
    type Value = Vec<T>;

    fn current(&self) -> Self::Value {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(index, sampled)| {
                self.included_values.test(index).then_some(*sampled)
            })
            .collect()
    }

    fn simplify(&mut self) -> bool {
        if self.included_values.len() <= self.min_size {
            return false;
        }

        while self.shrink < self.values.len()
            && !self.included_values.test(self.shrink)
        {
            self.shrink = self.shrink.saturating_add(1);
        }

        if self.shrink >= self.values.len() {
            self.prev_shrink = None;
            false
        } else {
            self.prev_shrink = Some(self.shrink);
            self.included_values.clear(self.shrink);
            self.shrink = self.shrink.saturating_add(1);
            true
        }
    }

    fn complicate(&mut self) -> bool {
        if let Some(shrink) = self.prev_shrink.take() {
            self.included_values.set(shrink);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use crate::std_facade::BTreeSet;

    use strict_test_support::{TestFailure, ensure, ensure_eq, ensure_some};

    use super::*;

    #[test]
    fn sample_range() -> Result<(), TestFailure> {
        static INDICES: Range<usize> = 0..8;
        let mut size_counts: [usize; 8] = [0; 8];
        let mut value_counts: [usize; 8] = [0; 8];

        let mut runner = TestRunner::deterministic();
        let input = range_subset(INDICES.clone(), 3..7);

        for _ in 0..2048 {
            let value = ensure_some(
                input.new_tree(&mut runner).ok(),
                "range_subset generates a value tree",
            )?
            .current();
            // Generated the correct number of items
            ensure(
                (3..7).contains(&value.len()),
                "the subset length stays within the requested size range",
            )?;
            // Chose distinct items
            ensure_eq(
                &value.len(),
                &value.iter().copied().collect::<BTreeSet<_>>().len(),
                "the subset contains only distinct items",
            )?;

            if let Some(count) = size_counts.get_mut(value.len()) {
                *count += 1;
            }

            for selected_index in value {
                let count = ensure_some(
                    value_counts.get_mut(selected_index),
                    "range_subset only generates indices from the input range",
                )?;
                *count += 1;
            }
        }

        for count in size_counts.iter().take(7).skip(3) {
            ensure(
                (256..1024).contains(count),
                "each size in the requested range is chosen a plausible \
                 number of times",
            )?;
        }

        for &index_count in &value_counts {
            ensure(
                (1024..1500).contains(&index_count),
                "each index is chosen a plausible number of times",
            )?;
        }
        Ok(())
    }

    #[test]
    fn test_sample_sanity() -> Result<(), Reason> {
        check_strategy_sanity(range_subset(0..5, 1..3), None)
    }

    #[test]
    fn try_range_subset_accepts_a_valid_request() -> Result<(), TestFailure> {
        let strategy = ensure_some(
            try_range_subset(0..8, 3..7).ok(),
            "try_range_subset accepts a size range within the range length",
        )?;
        let mut runner = TestRunner::deterministic();
        let value = ensure_some(
            strategy.new_tree(&mut runner).ok(),
            "the fallibly constructed strategy generates",
        )?
        .current();
        ensure(
            (3..7).contains(&value.len()),
            "the sampled subset honors the size range",
        )
    }

    #[test]
    fn try_range_subset_rejects_invalid_requests() -> Result<(), TestFailure> {
        ensure(
            matches!(
                try_range_subset(0..8, 2..2),
                Err(RangeSubsetError::EmptySizeRange(_))
            ),
            "try_range_subset rejects an empty size range",
        )?;
        ensure_eq(
            &ensure_some(
                try_range_subset(0..3, 1..=9).err(),
                "try_range_subset rejects a size range beyond the range \
                 length",
            )?,
            &RangeSubsetError::TooLarge {
                size_end_incl: 9,
                len: 3,
            },
            "the typed error names the requested size and range length",
        )
    }

    #[test]
    fn subset_empty_range_works() -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();
        let input = range_subset(0..0, 0..1);
        ensure(
            Vec::<usize>::new()
                == ensure_some(
                    input.new_tree(&mut runner).ok(),
                    "range_subset generates a value tree",
                )?
                .current(),
            "an empty index range yields the empty subset",
        )
    }

    #[test]
    fn subset_full_range_works() -> Result<(), TestFailure> {
        let range = 1..4;
        let mut runner = TestRunner::deterministic();
        let input = range_subset(range.clone(), 3);
        let mut values = ensure_some(
            input.new_tree(&mut runner).ok(),
            "range_subset generates a value tree",
        )?
        .current();
        values.sort_unstable();
        ensure(
            Vec::<usize>::from_iter(range) == values,
            "a full-width subset covers the whole range",
        )
    }
}
