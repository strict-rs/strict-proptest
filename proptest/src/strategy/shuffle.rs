//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{Cell, Vec, VecDeque};

use rand::RngExt;

use crate::num;
use crate::strategy::traits::*;
use crate::test_runner::*;

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
fn swap_if_in_bounds<T>(
    slice: &mut [T],
    first_index: usize,
    second_index: usize,
) {
    if first_index == second_index {
        return;
    }
    let (lo, hi) = if first_index < second_index {
        (first_index, second_index)
    } else {
        (second_index, first_index)
    };
    if let Some((head, tail)) = slice.split_at_mut_checked(hi) {
        if let (Some(first), Some(second)) =
            (head.get_mut(lo), tail.first_mut())
        {
            core::mem::swap(first, second);
        }
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
    inner: V,
    /// The generator driving the swap pass; cloned on every `current()` so the
    /// permutation is reproduced identically each time.
    rng: TestRng,
    /// The maximum amount to move any one element during shuffling.
    ///
    /// This is `Cell` since we can't determine the bounds of the value until
    /// the first call to `current()`. (We technically _could_ by generating a
    /// value in `new_tree` and checking its length, but that would be a 100%
    /// slowdown.)
    dist: Cell<Option<num::usize::BinarySearch>>,
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
        let max_swap = self.dist.get().unwrap().current();

        // If empty collection or all swaps will be filtered out, there's
        // nothing to shuffle.
        if 0 == len || 0 == max_swap {
            return permuted;
        }

        let mut rng = self.rng.clone();

        for start_index in 0..len - 1 {
            // Determine the other index to be swapped, then skip the swap if
            // it is too far. This ordering is critical, as it ensures that we
            // generate the same sequence of random numbers every time.
            let end_index = rng.random_range(start_index..len);
            if end_index - start_index <= max_swap {
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
            if self.dist.get_mut().as_mut().unwrap().simplify() {
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
            self.dist.get_mut().as_mut().unwrap().complicate()
        }
    }
}

#[cfg(test)]
mod test {
    use std::borrow::ToOwned;
    use std::collections::HashSet;
    use std::{format, vec};

    use strict_test_support::{TestFailure, ensure, ensure_eq, ensure_some};

    use super::*;
    use crate::collection;
    use crate::strategy::just::Just;

    static VALUES: &[i32] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    ];

    #[test]
    fn generates_different_permutations() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        let mut seen = HashSet::<Vec<i32>>::new();

        let input = Just(VALUES.to_owned()).prop_shuffle();

        for _ in 0..1024 {
            let mut value = ensure_some(
                input.new_tree(&mut runner).ok(),
                "shuffle strategy generates a value tree",
            )?
            .current();

            ensure(
                seen.insert(value.clone()),
                "no permutation is generated twice",
            )?;

            value.sort();
            ensure(
                VALUES == &value[..],
                "every permutation keeps the original elements",
            )?;
        }
        Ok(())
    }

    #[test]
    fn simplify_reduces_shuffle_amount() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();

        let input = Just(VALUES.to_owned()).prop_shuffle();
        for _ in 0..1024 {
            let mut value = ensure_some(
                input.new_tree(&mut runner).ok(),
                "shuffle strategy generates a value tree",
            )?;

            let mut prev_dist = i32::MAX;
            loop {
                let shuffled = value.current();
                // Compute the "shuffle distance" by summing the absolute
                // distance of each element's displacement.
                let mut dist = 0;
                for (ix, &nominal) in shuffled.iter().enumerate() {
                    dist += (nominal - ix as i32).abs();
                }

                ensure(
                    dist <= prev_dist,
                    "each simplify step reduces the shuffle distance",
                )?;

                prev_dist = dist;
                if !value.simplify() {
                    break;
                }
            }

            // When fully simplified, the result is in the original order.
            ensure_eq(
                &0,
                &prev_dist,
                "full simplification restores the original order",
            )?;
        }
        Ok(())
    }

    #[test]
    fn simplify_complicate_contract_upheld() {
        check_strategy_sanity(
            collection::vec(0i32..1000, 5..10).prop_shuffle(),
            None,
        );
    }

    #[test]
    fn swap_if_in_bounds_swaps_in_bounds_pairs() -> Result<(), TestFailure> {
        let mut values = [1, 2, 3, 4];
        swap_if_in_bounds(&mut values, 0, 3);
        ensure_eq(
            &"[4, 2, 3, 1]".to_owned(),
            &format!("{values:?}"),
            "an in-bounds pair swaps both elements",
        )?;
        swap_if_in_bounds(&mut values, 2, 2);
        ensure_eq(
            &"[4, 2, 3, 1]".to_owned(),
            &format!("{values:?}"),
            "equal indices leave the slice unchanged",
        )
    }

    #[test]
    fn swap_if_in_bounds_ignores_out_of_bounds_pairs() -> Result<(), TestFailure>
    {
        let mut values = [1, 2, 3];
        swap_if_in_bounds(&mut values, 0, 3);
        swap_if_in_bounds(&mut values, 5, 1);
        swap_if_in_bounds(&mut values, 9, 9);
        ensure_eq(
            &"[1, 2, 3]".to_owned(),
            &format!("{values:?}"),
            "out-of-bounds pairs leave the slice untouched",
        )
    }

    #[test]
    fn vec_deque_swap_is_bounds_guarded() -> Result<(), TestFailure> {
        let mut deque: VecDeque<i32> = VecDeque::from(vec![1, 2, 3]);
        deque.shuffle_swap(0, 2);
        ensure_eq(
            &"[3, 2, 1]".to_owned(),
            &format!("{deque:?}"),
            "an in-bounds pair swaps both deque elements",
        )?;
        deque.shuffle_swap(0, 3);
        deque.shuffle_swap(7, 1);
        ensure_eq(
            &"[3, 2, 1]".to_owned(),
            &format!("{deque:?}"),
            "out-of-bounds pairs leave the deque untouched",
        )
    }
}
