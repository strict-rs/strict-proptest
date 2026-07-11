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

use crate::std_facade::Rc;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::{
    LazyValueTree, NewTree, Strategy, TupleUnion, TupleUnionActive2,
    TupleUnionValueTree, ValueTree, WeightedStrategy, float_to_weight, statics,
};
use crate::test_runner::TestRunner;

// Re-export the type for easier usage.
pub use crate::option::{Probability, prob};

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
type MapErr<T, E> =
    statics::Map<E, WrapErr<<T as Strategy>::Value, <E as Strategy>::Value>>;
/// The `Ok`-producing half of a `Result` union: the `T` strategy mapped
/// through `WrapOk` so its generated values arrive as `Ok`.
type MapOk<T, E> =
    statics::Map<T, WrapOk<<T as Strategy>::Value, <E as Strategy>::Value>>;

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
impl<T: Strategy + fmt::Debug, E: Strategy + fmt::Debug> fmt::Debug
    for MaybeOk<T, E>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MaybeOk({:?})", self.0)
    }
}
impl<T: Strategy + fmt::Debug, E: Strategy + fmt::Debug> fmt::Debug
    for MaybeErr<T, E>
{
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
pub fn maybe_ok<T: Strategy, E: Strategy>(
    ok_strategy: T,
    err_strategy: E,
) -> MaybeOk<T, E> {
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
            Rc::new(statics::Map::new(
                err_strategy,
                WrapErr(PhantomData, PhantomData),
            )),
        ),
        (
            ok_weight,
            Rc::new(statics::Map::new(
                ok_strategy,
                WrapOk(PhantomData, PhantomData),
            )),
        ),
    )))
}

/// Create a strategy for `Result`s where `Ok` values are taken from
/// `ok_strategy` and `Err` values are taken from `err_strategy`.
///
/// `Ok` and `Err` are chosen with equal probability.
///
/// Generated values shrink to `Ok`.
pub fn maybe_err<T: Strategy, E: Strategy>(
    ok_strategy: T,
    err_strategy: E,
) -> MaybeErr<T, E> {
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
        (
            ok_weight,
            Rc::new(statics::Map::new(
                ok_strategy,
                WrapOk(PhantomData, PhantomData),
            )),
        ),
        (
            err_weight,
            Rc::new(statics::Map::new(
                err_strategy,
                WrapErr(PhantomData, PhantomData),
            )),
        ),
    )))
}

#[cfg(test)]
mod test {
    use crate::test_runner::{Reason, test_runner_without_persistence};

    use strict_test_support::{TestFailure, ensure, ensure_some};

    use super::*;
    use crate::strategy::Just;

    fn count_ok_of_1000(
        strategy: impl Strategy<Value = Result<(), ()>>,
    ) -> Result<u32, TestFailure> {
        let mut runner = TestRunner::deterministic();
        let mut count = 0_u32;
        for _ in 0..1000 {
            let generated = ensure_some(
                strategy.new_tree(&mut runner).ok(),
                "result strategy generates a value tree",
            )?;
            count =
                count.saturating_add(u32::from(generated.current().is_ok()));
        }

        Ok(count)
    }

    #[allow(
        clippy::single_call_fn,
        reason = "the result shrink test names the maybe_err direction toward Ok"
    )]
    fn ensure_maybe_err_case_shrinks_to_ok<V>(
        val: &mut V,
    ) -> Result<(), TestFailure>
    where
        V: ValueTree<Value = Result<(), ()>>,
    {
        if val.current().is_ok() {
            ensure(!val.simplify(), "an Ok case cannot simplify")?;
            return ensure(val.current().is_ok(), "the case stays Ok");
        }

        ensure(val.simplify(), "an Err case simplifies")?;
        ensure(val.current().is_ok(), "maybe_err shrinks toward Ok")
    }

    #[allow(
        clippy::single_call_fn,
        reason = "the result shrink test names the maybe_ok direction toward Err"
    )]
    fn ensure_maybe_ok_case_shrinks_to_err<V>(
        val: &mut V,
    ) -> Result<(), TestFailure>
    where
        V: ValueTree<Value = Result<(), ()>>,
    {
        if val.current().is_err() {
            ensure(!val.simplify(), "an Err case cannot simplify")?;
            return ensure(val.current().is_err(), "the case stays Err");
        }

        ensure(val.simplify(), "an Ok case simplifies")?;
        ensure(val.current().is_err(), "maybe_ok shrinks toward Err")
    }

    #[test]
    fn probability_defaults_to_0p5() -> Result<(), TestFailure> {
        let default_err_weight =
            count_ok_of_1000(maybe_err(Just(()), Just(())))?;
        ensure(
            default_err_weight > 400 && default_err_weight < 600,
            "maybe_err defaults to a balanced split",
        )?;
        let default_ok_weight = count_ok_of_1000(maybe_ok(Just(()), Just(())))?;
        ensure(
            default_ok_weight > 400 && default_ok_weight < 600,
            "maybe_ok defaults to a balanced split",
        )
    }

    #[test]
    fn probability_handled_correctly() -> Result<(), TestFailure> {
        let mostly_ok_from_low_err =
            count_ok_of_1000(maybe_err_weighted(0.1, Just(()), Just(())))?;
        ensure(
            mostly_ok_from_low_err > 800 && mostly_ok_from_low_err < 950,
            "a 0.1 err weight yields mostly Ok",
        )?;

        let mostly_err_from_high_err =
            count_ok_of_1000(maybe_err_weighted(0.9, Just(()), Just(())))?;
        ensure(
            mostly_err_from_high_err > 50 && mostly_err_from_high_err < 150,
            "a 0.9 err weight yields mostly Err",
        )?;

        let mostly_ok_from_high_ok =
            count_ok_of_1000(maybe_ok_weighted(0.9, Just(()), Just(())))?;
        ensure(
            mostly_ok_from_high_ok > 800 && mostly_ok_from_high_ok < 950,
            "a 0.9 ok weight yields mostly Ok",
        )?;

        let mostly_err_from_low_ok =
            count_ok_of_1000(maybe_ok_weighted(0.1, Just(()), Just(())))?;
        ensure(
            mostly_err_from_low_ok > 50 && mostly_err_from_low_ok < 150,
            "a 0.1 ok weight yields mostly Err",
        )
    }

    #[test]
    fn shrink_to_correct_case() -> Result<(), TestFailure> {
        let mut runner = test_runner_without_persistence();
        {
            let input = maybe_err(Just(()), Just(()));
            for _ in 0..64 {
                let mut val = ensure_some(
                    input.new_tree(&mut runner).ok(),
                    "maybe_err strategy generates a value tree",
                )?;
                ensure_maybe_err_case_shrinks_to_ok(&mut val)?;
            }
        }
        {
            let input = maybe_ok(Just(()), Just(()));
            for _ in 0..64 {
                let mut val = ensure_some(
                    input.new_tree(&mut runner).ok(),
                    "maybe_ok strategy generates a value tree",
                )?;
                ensure_maybe_ok_case_shrinks_to_err(&mut val)?;
            }
        }
        Ok(())
    }

    #[test]
    fn test_sanity() -> Result<(), Reason> {
        check_strategy_sanity(maybe_ok(0_i32..100_i32, 0_i32..100_i32), None)?;
        check_strategy_sanity(maybe_err(0_i32..100_i32, 0_i32..100_i32), None)
    }
}
