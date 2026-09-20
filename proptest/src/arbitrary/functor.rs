//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Provides higher order `Arbitrary` traits.
//! This is mainly for use by `proptest_derive`.
//!
//! ## Stability note
//!
//! This trait is mainly defined for `proptest_derive` to simplify the
//! mechanics of deriving recursive types. If you have custom containers
//! and want to support recursive for those, it is a good idea to implement
//! this trait.
//!
//! There are clearer and terser ways that work better with
//! inference such as using `proptest::collection::vec(..)`
//! to achieve the same result.
//!
//! For these reasons, the traits here are deliberately
//! not exported in a convenient way.

use crate::std_facade::fmt;
use crate::strategy::BoxedStrategy;
use crate::strategy::Strategy;

/// `ArbitraryF1` lets you lift a [`Strategy`] to unary
/// type constructors such as `Box`, `Vec`, and `Option`.
///
/// The trait corresponds to
/// [Haskell QuickCheck's `Arbitrary1` type class][HaskellQC].
///
/// [HaskellQC]:
/// https://hackage.haskell.org/package/QuickCheck-2.10.1/docs/Test-QuickCheck-Arbitrary.html#t:Arbitrary1
///
/// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
pub trait ArbitraryF1<A: fmt::Debug>: fmt::Debug + Sized {
  //==========================================================================
  // Implementation note #1
  //==========================================================================
  // It might be better to do this with generic associated types by
  // having an associated type:
  //
  // `type Strategy<A>: Strategy<Value = Self>;`
  //
  // But with this setup we will likely loose the ability to add bounds
  // such as `Hash + Eq` on `A` which is needed for `HashSet`. We might
  // be able to regain this ability with a ConstraintKinds feature.
  //
  // This alternate formulation will likely work better with type inference.
  //
  //==========================================================================
  // Implementation note #2
  //==========================================================================
  // Until `-> impl Trait` has been stabilized, `BoxedStrategy` must be
  // used. This incurs an unfortunate performance penalty - but since
  // we are dealing with testing, it is better to provide slowed down and
  // somewhat less general functionality than no functionality at all.
  // Implementations should just use `.boxed()` in the end.
  //==========================================================================

  /// The type of parameters that [`lift1_with`] accepts for
  /// configuration of the lifted and generated [`Strategy`]. Parameters
  /// must implement [`Default`].
  ///
  /// [`lift1_with`]:
  ///     trait.ArbitraryF1.html#tymethod.lift1_with
  ///
  /// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
  /// [`Default`]:
  ///     https://doc.rust-lang.org/nightly/std/default/trait.Default.html
  type Parameters: Default;

  /// Lifts a given [`Strategy`] to a new [`Strategy`] for the (presumably)
  /// bigger type. This is useful for lifting a `Strategy` for `SomeType`
  /// to a container such as `Vec<SomeType>`.
  ///
  /// Calling this for the type `X` is the equivalent of using
  /// [`X::lift1_with(base, Default::default())`].
  ///
  /// This method is defined in the trait for optimization for the
  /// default if you want to do that. It is a logic error to not
  /// preserve the semantics when overriding.
  ///
  /// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
  ///
  /// [`X::lift1_with(base, Default::default())`]:
  ///     trait.ArbitraryF1.html#tymethod.lift1_with
  fn lift1<AS>(base: AS) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = A> + 'static,
  {
    Self::lift1_with(base, Self::Parameters::default())
  }

  /// Lifts a given [`Strategy`] to a new [`Strategy`] for the (presumably)
  /// bigger type. This is useful for lifting a `Strategy` for `SomeType`
  /// to a container such as `Vec` of `SomeType`. The composite strategy is
  /// passed the arguments given in `args`.
  ///
  /// If you wish to use the [`default()`] arguments,
  /// use [`lift1`] instead.
  ///
  /// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
  ///
  /// [`lift1`]: trait.ArbitraryF1.html#method.lift1
  ///
  /// [`default()`]:
  ///     https://doc.rust-lang.org/nightly/std/default/trait.Default.html
  fn lift1_with<AS>(base: AS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = A> + 'static;
}

/// `ArbitraryF2` lets you lift [`Strategy`] to binary
/// type constructors such as `Result`, `HashMap`.
///
/// The trait corresponds to
/// [Haskell QuickCheck's `Arbitrary2` type class][HaskellQC].
///
/// [HaskellQC]:
/// https://hackage.haskell.org/package/QuickCheck-2.10.1/docs/Test-QuickCheck-Arbitrary.html#t:Arbitrary2
///
/// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
pub trait ArbitraryF2<A: fmt::Debug, B: fmt::Debug>: fmt::Debug + Sized {
  /// The type of parameters that [`lift2_with`] accepts for
  /// configuration of the lifted and generated [`Strategy`]. Parameters
  /// must implement [`Default`].
  ///
  /// [`lift2_with`]: trait.ArbitraryF2.html#tymethod.lift2_with
  ///
  /// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
  ///
  /// [`Default`]:
  ///     https://doc.rust-lang.org/nightly/std/default/trait.Default.html
  type Parameters: Default;

  /// Lifts two given strategies to a new [`Strategy`] for the (presumably)
  /// bigger type. This is useful for lifting a `Strategy` for `Type1`
  /// and one for `Type2` to a container such as `HashMap<Type1, Type2>`.
  ///
  /// Calling this for the type `X` is the equivalent of using
  /// [`X::lift2_with(base, Default::default())`].
  ///
  /// This method is defined in the trait for optimization for the
  /// default if you want to do that. It is a logic error to not
  /// preserve the semantics when overriding.
  ///
  /// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
  ///
  /// [`X::lift2_with(base, Default::default())`]:
  ///     trait.Arbitrary.html#tymethod.lift2_with
  fn lift2<AS, BS>(fst: AS, snd: BS) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = A> + 'static,
    BS: Strategy<Value = B> + 'static,
  {
    Self::lift2_with(fst, snd, Self::Parameters::default())
  }

  /// Lifts two given strategies to a new [`Strategy`] for the (presumably)
  /// bigger type. This is useful for lifting a `Strategy` for `Type1`
  /// and one for `Type2` to a container such as `HashMap<Type1, Type2>`.
  /// The composite strategy is passed the arguments given in `args`.
  ///
  /// If you wish to use the [`default()`] arguments,
  /// use [`lift2`] instead.
  ///
  /// [`Strategy`]: ../proptest/strategy/trait.Strategy.html
  ///
  /// [`lift2`]: trait.ArbitraryF2.html#method.lift2
  ///
  /// [`default()`]:
  ///     https://doc.rust-lang.org/nightly/std/default/trait.Default.html
  fn lift2_with<AS, BS>(fst: AS, snd: BS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = A> + 'static,
    BS: Strategy<Value = B> + 'static;
}

/// Generates an `ArbitraryF1` impl that lifts a base `Strategy` over a
/// single-type-parameter container (`Box`, `Vec`, `Option`, ...), so
/// `proptest_derive` and the arbitrary tiers need only one line rather than a
/// full higher-order impl.
///
/// The arms cover, in order: a full hand-written `lift1_with` body; a
/// params-defaulted body (`Parameters = ()`); a `prop_map`-via-mapper body;
/// and a `prop_map_into` default that maps the base value into the container.
macro_rules! lift1 {
    ([$($bounds : tt)*] $typ: ty, $params: ty;
     $base: ident, $args: ident => $logic: expr) => {
        impl<A: ::core::fmt::Debug + $($bounds)*>
        $crate::arbitrary::functor::ArbitraryF1<A>
        for $typ {
            type Parameters = $params;

            fn lift1_with<S>($base: S, $args: Self::Parameters)
                -> $crate::strategy::BoxedStrategy<Self>
            where
                S: $crate::strategy::Strategy<Value = A> + 'static
            {
                $crate::strategy::Strategy::boxed($logic)
            }
        }
    };
    ([$($bounds : tt)*] $typ: ty; $base: ident => $logic: expr) => {
        lift1!([$($bounds)*] $typ, (); $base, _args => $logic);
    };
    ([$($bounds : tt)*] $typ: ty; $mapper: expr) => {
        lift1!(['static + $($bounds)*] $typ; base =>
            $crate::strategy::Strategy::prop_map(base, $mapper));
    };
    ([$($bounds : tt)*] $typ: ty) => {
        lift1!(['static + $($bounds)*] $typ; base =>
            $crate::strategy::Strategy::prop_map_into(base));
    };
}

/// Generate a binary lifting implementation with caller-owned type bounds,
/// parameters, and strategy construction. Boxing stays at the trait boundary.
macro_rules! lift2 {
    ([$($bounds:tt)*] $typ:ty, $first:ty, $second:ty, $params:ty;
     $fst:ident, $snd:ident, $args:ident => $logic:expr) => {
        impl<$($bounds)*> $crate::arbitrary::functor::ArbitraryF2<$first, $second> for $typ {
            type Parameters = $params;

            fn lift2_with<AS, BS>($fst: AS, $snd: BS, $args: Self::Parameters)
                -> $crate::strategy::BoxedStrategy<Self>
            where
                AS: $crate::strategy::Strategy<Value = $first> + 'static,
                BS: $crate::strategy::Strategy<Value = $second> + 'static,
            {
                $crate::strategy::Strategy::boxed($logic)
            }
        }
    };
}

#[cfg(test)]
mod tests {
  #[cfg(feature = "std")]
  use core::hash::BuildHasherDefault;
  use core::iter::Chain;
  use core::iter::Zip;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::ArbitraryF2 as _;
  use crate::std_facade::BTreeMap;
  #[cfg(feature = "std")]
  use crate::std_facade::HashMap;
  use crate::std_facade::Vec;
  use crate::std_facade::btree_map;
  #[cfg(feature = "std")]
  use crate::std_facade::hash_map;
  use crate::strategy::Just;
  use crate::strategy::Strategy;
  use crate::strategy::ValueTree as _;
  use crate::test_runner::Config;
  use crate::test_runner::Reason;
  use crate::test_runner::TestRunner;

  /// A complete generated sequence or its native generation failure.
  type Sample<T> = Result<Vec<T>, Reason>;

  /// Terminal assertion retaining every generated sequence and native rejection.
  type SampleCheck<T, const N: usize> = Result<(), PredicateFailure<[Sample<T>; N]>>;

  /// Generate one lifted container and retain its yielded items or rejection.
  fn generated_items<S: Strategy>(strategy: &S) -> Sample<<S::Value as IntoIterator>::Item>
  where
    S::Value: IntoIterator,
  {
    let mut runner = TestRunner::new(Config {
      max_local_rejects: 8,
      failure_persistence: None,
      ..Config::default()
    });
    strategy.new_tree(&mut runner).map(|tree| tree.current().into_iter().collect())
  }

  #[test]
  fn zip_lifting_stops_at_the_shorter_input() -> SampleCheck<(u8, u16), 2> {
    let observed = [
      generated_items(&Zip::lift2(Just([1_u8, 2].into_iter()), Just([10_u16].into_iter()))),
      generated_items(&Zip::lift2(Just([1_u8].into_iter()), Just([].into_iter()))),
    ];
    ensure_that(observed, "zip pairs inputs in order and never yields an unpaired item", |samples| {
      let [short, empty] = samples.each_ref();
      short.as_ref().is_ok_and(|items| items.as_slice() == [(1, 10)]) && empty.as_ref().is_ok_and(Vec::is_empty)
    })
    .map(drop)
  }

  #[test]
  fn chain_lifting_keeps_both_inputs_in_order() -> SampleCheck<u8, 2> {
    let observed = [
      generated_items(&Chain::lift2(Just([1_u8, 2].into_iter()), Just([3_u8].into_iter()))),
      generated_items(&Chain::lift2(Just([].into_iter()), Just([3_u8].into_iter()))),
    ];
    ensure_that(observed, "chain preserves both sequences even when the first is empty", |samples| {
      let [both, tail] = samples.each_ref();
      both.as_ref().is_ok_and(|items| items.as_slice() == [1, 2, 3]) && tail.as_ref().is_ok_and(|items| items.as_slice() == [3])
    })
    .map(drop)
  }

  /// Exercise the same size and collision contract for maps and their owning iterators.
  macro_rules! map_lifting_test {
    ($name:ident, $map:ty) => {
      #[test]
      fn $name() -> SampleCheck<(u8, u16), 3> {
        let observed = [0_usize, 1, 2].map(|size| generated_items(&<$map>::lift2_with(Just(7_u8), Just(11_u16), size.into())));
        ensure_that(
          observed,
          "map lifting keeps entries and rejects impossible unique-key counts",
          |samples| {
            let [empty, one, collision] = samples.each_ref();
            empty.as_ref().is_ok_and(Vec::is_empty)
              && one.as_ref().is_ok_and(|entries| entries.as_slice() == [(7, 11)])
              && collision.is_err()
          },
        )
        .map(drop)
      }
    };
  }

  map_lifting_test!(btree_map_lifting_preserves_size, BTreeMap<u8, u16>);
  map_lifting_test!(btree_map_iterator_lifting_preserves_size, btree_map::IntoIter<u8, u16>);

  #[cfg(feature = "std")]
  map_lifting_test!(hash_map_iterator_lifting_preserves_size, hash_map::IntoIter<u8, u16>);

  #[cfg(feature = "std")]
  map_lifting_test!(
    custom_hasher_map_lifting_preserves_size,
    HashMap<u8, u16, BuildHasherDefault<hash_map::DefaultHasher>>
  );
}
