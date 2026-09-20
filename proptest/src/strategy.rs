//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Defines the core traits used by Proptest.

#[cfg(test)]
use core::ops::Range;

use crate::std_facade::Rc;
#[cfg(test)]
use crate::std_facade::Vec;
#[cfg(test)]
use crate::std_facade::vec;

/// The `Filter` combinator (`Strategy::prop_filter`): rejection sampling
/// that discards generated values a predicate does not accept.
mod filter;
/// The `FilterMap` combinator (`Strategy::prop_filter_map`): a fused map and
/// filter that keeps only the values its closure maps to `Some`.
mod filter_map;
/// The flat-map combinators (`Flatten`, `IndFlatten`, `IndFlattenMap`) that
/// derive a new strategy from each generated value and pick from it.
mod flatten;
/// The `Fuse` adaptor which guards a `ValueTree` against out-of-order or
/// post-`false` `simplify()`/`complicate()` calls.
mod fuse;
/// The constant strategies `Just` and `LazyJust`, which always produce the
/// same value and never shrink.
mod just;
/// `LazyValueTree`, a value tree whose generation is deferred until the first
/// time it is used, letting unions skip branches they never pick.
mod lazy;
/// The value-transforming combinators `Map`, `MapInto`, and `Perturb`, which
/// reshape generated values while shrinking in the source's terms.
mod map;
/// The `Recursive` combinator (`Strategy::prop_recursive`) for generating
/// self-nesting structures with a bounded depth and size.
mod recursive;
/// The `Shuffle` combinator (`Strategy::prop_shuffle`) which permutes the
/// contents of the collections a strategy produces.
mod shuffle;
/// The core `Strategy` and `ValueTree` traits, their boxing adaptors, and the
/// `check_strategy_sanity` contract checker.
mod traits;
/// The weighted-choice combinators `Union` and `TupleUnion` backing
/// `prop_oneof!` and `Strategy::prop_union`.
mod unions;

pub use self::filter::*;
pub use self::filter_map::*;
pub use self::flatten::*;
pub use self::fuse::*;
pub use self::just::*;
pub use self::lazy::*;
pub use self::map::*;
pub use self::recursive::*;
pub use self::shuffle::*;
pub use self::traits::*;
pub use self::unions::*;

pub mod statics;

/// A weighted choice between two concrete strategy types.
type BinaryUnion<A, B> = TupleUnion<(WeightedStrategy<A>, WeightedStrategy<B>)>;

/// Build a two-branch union with the given probability of initially selecting
/// the second branch. Shrinking still prefers the first branch, and invalid
/// probabilities retain the zero-weight generation failure.
pub(crate) fn binary_union<A: Strategy, B: Strategy<Value = A::Value>>(
  probability_of_second: f64,
  first: A,
  second: B,
) -> BinaryUnion<A, B> {
  let (second_weight, first_weight) = float_to_weight(probability_of_second);
  TupleUnion::new(((first_weight, Rc::new(first)), (second_weight, Rc::new(second))))
}

/// Retain a value tree and the values reached by successful simplifications.
#[cfg(test)]
pub(crate) fn trace_shrink_steps<V: ValueTree>(mut tree: V) -> (V, Vec<V::Value>) {
  let mut values = vec![tree.current()];
  while tree.simplify() {
    values.push(tree.current());
  }
  (tree, values)
}

#[cfg(test)]
use strict_test_support::PredicateFailure;
#[cfg(test)]
use strict_test_support::ensure_that;

#[cfg(test)]
use crate::std_facade::fmt;
#[cfg(test)]
use crate::test_runner::Reason;
#[cfg(test)]
use crate::test_runner::TestRunner;

/// Inspect native generation outcomes without losing errors while counting the
/// candidates accepted by a frequency test's predicate.
#[cfg(test)]
pub(crate) fn sample_count_in_range<T, E>(samples: &[Result<T, E>], expected: Range<usize>, accepts: impl Fn(&T) -> bool) -> bool {
  samples.iter().all(Result::is_ok) && expected.contains(&samples.iter().filter(|sample| sample.as_ref().is_ok_and(&accepts)).count())
}

/// A reached tree and every native candidate, or the original generation failure.
#[cfg(test)]
type ShrinkWalk<S> = Result<(<S as Strategy>::Tree, Vec<<S as Strategy>::Value>), Reason>;

/// A failed product-shrinking check retains every generation and shrink observation.
#[cfg(test)]
type ProductShrinkFailure<S> = PredicateFailure<Vec<ShrinkWalk<S>>>;

/// Terminal tests discard completed success evidence while retaining native failures.
#[cfg(test)]
pub(crate) type ProductShrinkCheck<S> = Result<(), ProductShrinkFailure<S>>;

/// Retain the complete search for a minimal failing candidate, including the
/// final unsuccessful simplify or complicate attempt.
#[cfg(test)]
#[allow(
  clippy::single_call_fn,
  reason = "the native shrink trace separates search execution from the product minimality assertions"
)]
fn trace_failing_shrink<V: ValueTree>(mut tree: V, passes: impl Fn(&V::Value) -> bool) -> (V, Vec<V::Value>) {
  let mut values = vec![tree.current()];
  if passes(&tree.current()) {
    return (tree, values);
  }
  loop {
    let advanced = if passes(&tree.current()) {
      tree.complicate()
    } else {
      tree.simplify()
    };
    values.push(tree.current());
    if !advanced {
      break;
    }
  }
  (tree, values)
}

/// The original product property shared by the array and tuple shrink checks.
#[cfg(test)]
const fn product_passes((left, right): (i32, i32)) -> bool {
  left.saturating_mul(right) <= 9
}

/// Check native product walks at the minimal failing multiplication boundary
/// and require enough initially failing cases to exercise shrinking.
#[cfg(test)]
pub(crate) fn check_product_shrinking<S: Strategy>(
  input: &S,
  runner: &mut TestRunner,
  components: impl Fn(&S::Value) -> (i32, i32),
) -> Result<Vec<ShrinkWalk<S>>, ProductShrinkFailure<S>>
where
  S::Tree: fmt::Debug,
{
  let passes = |value: &S::Value| product_passes(components(value));
  let walks = (0..256)
    .map(|_| input.new_tree(runner).map(|tree| trace_failing_shrink(tree, passes)))
    .collect();
  let minimal_walk = |walk: &ShrinkWalk<S>| {
    let Ok(reached) = walk.as_ref() else {
      return false;
    };
    let Some(initial) = reached.1.first() else {
      return false;
    };
    if passes(initial) {
      return reached.1.len() == 1;
    }
    let (left, right) = components(&reached.0.current());
    !product_passes((left, right)) && product_passes((left.saturating_sub(1), right)) && product_passes((left, right.saturating_sub(1)))
  };
  ensure_that(
    walks,
    "failing products shrink minimally left to right, with enough generated failures",
    |subjects: &Vec<ShrinkWalk<S>>| {
      subjects.iter().all(minimal_walk)
        && subjects
          .iter()
          .filter(|walk| {
            walk
              .as_ref()
              .is_ok_and(|reached| reached.1.first().is_some_and(|initial| !passes(initial)))
          })
          .count()
          > 32
    },
  )
}
