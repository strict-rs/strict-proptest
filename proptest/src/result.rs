//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for combining delegate strategies into `std::Result`s.
//!
//! That is, the strategies here are for producing `Ok` _and_ `Err` cases. To
//! simply adapt a strategy producing `T` into `Result<T, something>` which is
//! always `Ok`, you can do something like `base_strategy.prop_map(Ok)` to
//! simply wrap the generated values.
//!
//! Note that there are two nearly identical APIs for doing this, termed "maybe
//! ok" and "maybe err". The difference between the two is in how they shrink;
//! "maybe ok" treats `Ok` as the special case and shrinks to `Err`;
//! conversely, "maybe err" treats `Err` as the special case and shrinks to
//! `Ok`. Which to use largely depends on the code being tested; if the code
//! typically handles errors by immediately bailing out and doing nothing else,
//! "maybe ok" is likely more suitable, as shrinking will cause the code to
//! take simpler paths. On the other hand, functions that need to make a
//! complicated or fragile "back out" process on error are better tested with
//! "maybe err" since the success case results in an easier to understand code
//! path.

use core::fmt;
use core::marker::PhantomData;

// Re-export the type for easier usage.
pub use crate::option::{
  Probability,
  prob,
};
use crate::std_facade::Rc;
use crate::strategy::LazyValueTree;
use crate::strategy::NewTree;
use crate::strategy::Strategy;
use crate::strategy::TupleUnion;
use crate::strategy::TupleUnionActive2;
use crate::strategy::TupleUnionValueTree;
use crate::strategy::ValueTree;
use crate::strategy::WeightedStrategy;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::float_to_weight;
use crate::strategy::statics;
use crate::test_runner::TestRunner;

/// `MapFn` wrapping a generated success value into `Ok`.
///
/// Applied to the `Ok` arm of the `Result` unions so a delegate strategy's
/// values arrive as `Result::Ok`; the `PhantomData` fixes the `T` and `E`
/// types without storing anything.
struct WrapOk<T, E>(PhantomData<T>, PhantomData<E>);
impl<T, E> Clone for WrapOk<T, E> {
  fn clone(&self) -> Self {
    Self(PhantomData, PhantomData)
  }
}
impl<T, E> fmt::Debug for WrapOk<T, E> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "WrapOk")
  }
}
impl<T: fmt::Debug, E: fmt::Debug> statics::MapFn<T> for WrapOk<T, E> {
  type Output = Result<T, E>;
  fn apply(&self, inner: T) -> Result<T, E> {
    Ok(inner)
  }
}
/// `MapFn` wrapping a generated failure value into `Err`.
///
/// Applied to the `Err` arm of the `Result` unions so a delegate strategy's
/// values arrive as `Result::Err`; the `PhantomData` fixes the `T` and `E`
/// types without storing anything.
struct WrapErr<T, E>(PhantomData<T>, PhantomData<E>);
impl<T, E> Clone for WrapErr<T, E> {
  fn clone(&self) -> Self {
    Self(PhantomData, PhantomData)
  }
}
impl<T, E> fmt::Debug for WrapErr<T, E> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "WrapErr")
  }
}
impl<T: fmt::Debug, E: fmt::Debug> statics::MapFn<E> for WrapErr<T, E> {
  type Output = Result<T, E>;
  fn apply(&self, err: E) -> Result<T, E> {
    Err(err)
  }
}

/// The `Err`-producing half of a `Result` union: the `E` strategy mapped
/// through `WrapErr` so its generated values arrive as `Err`.
type MapErr<T, E> = statics::Map<E, WrapErr<<T as Strategy>::Value, <E as Strategy>::Value>>;
/// The `Ok`-producing half of a `Result` union: the `T` strategy mapped
/// through `WrapOk` so its generated values arrive as `Ok`.
type MapOk<T, E> = statics::Map<T, WrapOk<<T as Strategy>::Value, <E as Strategy>::Value>>;

opaque_strategy_wrapper! {
    /// Strategy which generates `Result`s using `Ok` and `Err` values from two
    /// delegate strategies.
    ///
    /// Shrinks to `Err`.
    #[derive(Clone)]
    pub struct MaybeOk[<T, E>][where T : Strategy, E : Strategy]
        (TupleUnion<(WeightedStrategy<MapErr<T, E>>, WeightedStrategy<MapOk<T, E>>)>)
        -> MaybeOkValueTree<T, E>;
    /// `ValueTree` type corresponding to `MaybeOk`.
    pub struct MaybeOkValueTree[<T, E>][where T : Strategy, E : Strategy]
        (TupleUnionValueTree<(
            Option<LazyValueTree<statics::Map<E, WrapErr<T::Value, E::Value>>>>,
            Option<LazyValueTree<statics::Map<T, WrapOk<T::Value, E::Value>>>>,
        ), TupleUnionActive2<
            <statics::Map<E, WrapErr<T::Value, E::Value>> as Strategy>::Tree,
            <statics::Map<T, WrapOk<T::Value, E::Value>> as Strategy>::Tree,
        >>)
        -> Result<T::Value, E::Value>;
}

opaque_strategy_wrapper! {
    /// Strategy which generates `Result`s using `Ok` and `Err` values from two
    /// delegate strategies.
    ///
    /// Shrinks to `Ok`.
    #[derive(Clone)]
    pub struct MaybeErr[<T, E>][where T : Strategy, E : Strategy]
        (TupleUnion<(WeightedStrategy<MapOk<T, E>>, WeightedStrategy<MapErr<T, E>>)>)
        -> MaybeErrValueTree<T, E>;
    /// `ValueTree` type corresponding to `MaybeErr`.
    pub struct MaybeErrValueTree[<T, E>][where T : Strategy, E : Strategy]
        (TupleUnionValueTree<(
            Option<LazyValueTree<statics::Map<T, WrapOk<T::Value, E::Value>>>>,
            Option<LazyValueTree<statics::Map<E, WrapErr<T::Value, E::Value>>>>,
        ), TupleUnionActive2<
            <statics::Map<T, WrapOk<T::Value, E::Value>> as Strategy>::Tree,
            <statics::Map<E, WrapErr<T::Value, E::Value>> as Strategy>::Tree,
        >>)
        -> Result<T::Value, E::Value>;
}

// These need to exist for the same reason as the one on `OptionStrategy`
impl<T: Strategy + fmt::Debug, E: Strategy + fmt::Debug> fmt::Debug for MaybeOk<T, E> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "MaybeOk({:?})", self.0)
  }
}
impl<T: Strategy + fmt::Debug, E: Strategy + fmt::Debug> fmt::Debug for MaybeErr<T, E> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "MaybeErr({:?})", self.0)
  }
}

impl<T: Strategy, E: Strategy> Clone for MaybeOkValueTree<T, E>
where
  T::Tree: Clone,
  E::Tree: Clone,
{
  fn clone(&self) -> Self {
    Self(self.0.clone())
  }
}

impl<T: Strategy, E: Strategy> fmt::Debug for MaybeOkValueTree<T, E>
where
  T::Tree: fmt::Debug,
  E::Tree: fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "MaybeOkValueTree({:?})", self.0)
  }
}

impl<T: Strategy, E: Strategy> Clone for MaybeErrValueTree<T, E>
where
  T::Tree: Clone,
  E::Tree: Clone,
{
  fn clone(&self) -> Self {
    Self(self.0.clone())
  }
}

impl<T: Strategy, E: Strategy> fmt::Debug for MaybeErrValueTree<T, E>
where
  T::Tree: fmt::Debug,
  E::Tree: fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "MaybeErrValueTree({:?})", self.0)
  }
}

/// Create a strategy for `Result`s where `Ok` values are taken from
/// `ok_strategy` and `Err` values are taken from `err_strategy`.
///
/// `Ok` and `Err` are chosen with equal probability.
///
/// Generated values shrink to `Err`.
pub fn maybe_ok<T: Strategy, E: Strategy>(ok_strategy: T, err_strategy: E) -> MaybeOk<T, E> {
  maybe_ok_weighted(0.5, ok_strategy, err_strategy)
}

/// Create a strategy for `Result`s where `Ok` values are taken from
/// `ok_strategy` and `Err` values are taken from `err_strategy`.
///
/// `probability_of_ok` is the probability (between 0.0 and 1.0, exclusive)
/// that `Ok` is initially chosen.
///
/// Generated values shrink to `Err`.
pub fn maybe_ok_weighted<T: Strategy, E: Strategy>(
  probability_of_ok: impl Into<Probability>,
  ok_strategy: T,
  err_strategy: E,
) -> MaybeOk<T, E> {
  let prob = probability_of_ok.into().into();
  let (ok_weight, err_weight) = float_to_weight(prob);

  MaybeOk(TupleUnion::new((
    (
      err_weight,
      Rc::new(statics::Map::new(err_strategy, WrapErr(PhantomData, PhantomData))),
    ),
    (ok_weight, Rc::new(statics::Map::new(ok_strategy, WrapOk(PhantomData, PhantomData)))),
  )))
}

/// Create a strategy for `Result`s where `Ok` values are taken from
/// `ok_strategy` and `Err` values are taken from `err_strategy`.
///
/// `Ok` and `Err` are chosen with equal probability.
///
/// Generated values shrink to `Ok`.
pub fn maybe_err<T: Strategy, E: Strategy>(ok_strategy: T, err_strategy: E) -> MaybeErr<T, E> {
  maybe_err_weighted(0.5, ok_strategy, err_strategy)
}

/// Create a strategy for `Result`s where `Ok` values are taken from
/// `ok_strategy` and `Err` values are taken from `err_strategy`.
///
/// `probability_of_ok` is the probability (between 0.0 and 1.0, exclusive)
/// that `Err` is initially chosen.
///
/// Generated values shrink to `Ok`.
#[allow(
  clippy::single_call_fn,
  reason = "the Err-weighted Result strategy that the maybe_err combinator delegates to"
)]
pub fn maybe_err_weighted<T: Strategy, E: Strategy>(
  probability_of_err: impl Into<Probability>,
  ok_strategy: T,
  err_strategy: E,
) -> MaybeErr<T, E> {
  let prob = probability_of_err.into().into();
  let (err_weight, ok_weight) = float_to_weight(prob);

  MaybeErr(TupleUnion::new((
    (ok_weight, Rc::new(statics::Map::new(ok_strategy, WrapOk(PhantomData, PhantomData)))),
    (
      err_weight,
      Rc::new(statics::Map::new(err_strategy, WrapErr(PhantomData, PhantomData))),
    ),
  )))
}

#[cfg(test)]
mod test {
  use core::ops::Range;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Vec;
  use crate::strategy::Just;
  use crate::test_runner::Reason;
  use crate::test_runner::test_runner_without_persistence;

  /// Complete generated trees or their native generation failures.
  type Samples<S> = Vec<NewTree<S>>;
  /// Native samples from the strategy which shrinks toward Ok.
  type ErrSamples = Samples<MaybeErr<Just<()>, Just<()>>>;
  /// Native samples from the strategy which shrinks toward Err.
  type OkSamples = Samples<MaybeOk<Just<()>, Just<()>>>;
  /// Both weighted strategy directions, each retaining high and low frequency samples.
  type WeightedSamples = ([ErrSamples; 2], [OkSamples; 2]);

  fn sample_results<S: Strategy<Value = Result<(), ()>>>(strategy: S) -> Samples<S> {
    let mut runner = TestRunner::deterministic();
    (0..1000).map(|_| strategy.new_tree(&mut runner)).collect()
  }

  fn has_ok_count<V: ValueTree<Value = Result<(), ()>>>(samples: &[Result<V, Reason>], expected: Range<usize>) -> bool {
    samples.iter().all(Result::is_ok)
      && expected.contains(
        &samples
          .iter()
          .filter(|sample| sample.as_ref().is_ok_and(|tree| tree.current().is_ok()))
          .count(),
      )
  }

  #[test]
  fn probability_defaults_to_0p5() -> Result<(), PredicateFailure<(ErrSamples, OkSamples)>> {
    ensure_that(
      (
        sample_results(maybe_err(Just(()), Just(()))),
        sample_results(maybe_ok(Just(()), Just(()))),
      ),
      "both Result strategies default to a balanced split",
      |subjects| has_ok_count(&subjects.0, 401..600) && has_ok_count(&subjects.1, 401..600),
    )
    .map(drop)
  }

  #[test]
  fn probability_handled_correctly() -> Result<(), PredicateFailure<WeightedSamples>> {
    ensure_that(
      (
        [
          sample_results(maybe_err_weighted(0.1, Just(()), Just(()))),
          sample_results(maybe_err_weighted(0.9, Just(()), Just(()))),
        ],
        [
          sample_results(maybe_ok_weighted(0.9, Just(()), Just(()))),
          sample_results(maybe_ok_weighted(0.1, Just(()), Just(()))),
        ],
      ),
      "both weighted Result strategies preserve the requested high and low Ok frequencies",
      |subjects| {
        subjects
          .0
          .iter()
          .zip([801..950, 51..150])
          .all(|(draws, bounds)| has_ok_count(draws, bounds))
          && subjects
            .1
            .iter()
            .zip([801..950, 51..150])
            .all(|(draws, bounds)| has_ok_count(draws, bounds))
      },
    )
    .map(drop)
  }

  /// Reached tree, original case, simplify result, and resulting case.
  type ShrinkStep<V> = (V, Result<(), ()>, bool, Result<(), ()>);
  /// Both shrink directions retain every case and native generation error.
  type ShrinkDirections = (
    Vec<Result<ShrinkStep<MaybeErrValueTree<Just<()>, Just<()>>>, Reason>>,
    Vec<Result<ShrinkStep<MaybeOkValueTree<Just<()>, Just<()>>>, Reason>>,
  );

  fn shrink_once<V: ValueTree<Value = Result<(), ()>>>(mut tree: V) -> ShrinkStep<V> {
    let before = tree.current();
    let simplified = tree.simplify();
    let after = tree.current();
    (tree, before, simplified, after)
  }

  #[test]
  fn shrink_to_correct_case() -> Result<(), PredicateFailure<ShrinkDirections>> {
    let mut runner = test_runner_without_persistence();
    let toward_ok = maybe_err(Just(()), Just(()));
    let ok_steps = (0..64).map(|_| toward_ok.new_tree(&mut runner).map(shrink_once)).collect();
    let toward_err = maybe_ok(Just(()), Just(()));
    let err_steps = (0..64).map(|_| toward_err.new_tree(&mut runner).map(shrink_once)).collect();
    ensure_that(
      (ok_steps, err_steps),
      "Result cases shrink only toward the designated variant",
      |subjects: &ShrinkDirections| {
        subjects.0.iter().all(|step| {
          step
            .as_ref()
            .is_ok_and(|reached| reached.2 == reached.1.is_err() && reached.3.is_ok())
        }) && subjects.1.iter().all(|step| {
          step
            .as_ref()
            .is_ok_and(|reached| reached.2 == reached.1.is_ok() && reached.3.is_err())
        })
      },
    )
    .map(drop)
  }

  #[test]
  fn test_sanity() -> Result<(), Reason> {
    check_strategy_sanity(maybe_ok(0_i32..100_i32, 0_i32..100_i32), None)?;
    check_strategy_sanity(maybe_err(0_i32..100_i32, 0_i32..100_i32), None)
  }
}
