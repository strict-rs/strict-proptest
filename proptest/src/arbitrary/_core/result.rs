//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::result`.

use core::convert::Infallible;
use core::fmt;
use core::result::IntoIter;

use crate::arbitrary::Arbitrary;
use crate::arbitrary::SMapped;
use crate::arbitrary::any_with;
use crate::arbitrary::functor;
use crate::result::MaybeOk;
use crate::result::Probability;
use crate::result::maybe_ok_weighted;
use crate::strategy::BoxedStrategy;
use crate::strategy::Just;
use crate::strategy::Strategy;
use crate::strategy::statics::static_map;

// `string::ParseError` aliases `Infallible`, which also aliases `!` on current
// nightly Rust. One implementation per inhabited side covers all those names.
arbitrary!([A: Arbitrary] Result<A, Infallible>,
    SMapped<A, Self>, A::Parameters;
    args => static_map(any_with::<A>(args), Ok::<A, Infallible>)
);
arbitrary!([A: Arbitrary] Result<Infallible, A>,
    SMapped<A, Self>, A::Parameters;
    args => static_map(any_with::<A>(args), Err::<Infallible, A>)
);
lift1!([] Result<A, Infallible>; Ok);

// We assume that `MaybeOk` is canonical as it's the most likely Strategy
// a user wants.

arbitrary!([A: Arbitrary, B: Arbitrary] Result<A, B>,
    MaybeOk<A::Strategy, B::Strategy>,
    product_type![Probability, A::Parameters, B::Parameters];
    args => {
        let product_unpack![prob, ok_params, err_params] = args;
        let (probability, ok_strategy, err_strategy) =
            (prob, any_with::<A>(ok_params), any_with::<B>(err_params));
        maybe_ok_weighted(probability, ok_strategy, err_strategy)
    }
);

impl<A: fmt::Debug, E: Arbitrary> functor::ArbitraryF1<A> for Result<A, E>
where
  E::Strategy: 'static,
{
  type Parameters = product_type![Probability, E::Parameters];

  fn lift1_with<AS>(base: AS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = A> + 'static,
  {
    let product_unpack![prob, err_params] = args;
    let (probability, ok_strategy, err_strategy) = (prob, base, any_with::<E>(err_params));
    maybe_ok_weighted(probability, ok_strategy, err_strategy).boxed()
  }
}

impl<A: fmt::Debug, B: fmt::Debug> functor::ArbitraryF2<A, B> for Result<A, B> {
  type Parameters = Probability;

  fn lift2_with<AS, BS>(fst: AS, snd: BS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = A> + 'static,
    BS: Strategy<Value = B> + 'static,
  {
    maybe_ok_weighted(args, fst, snd).boxed()
  }
}

arbitrary!([A: Arbitrary] IntoIter<A>,
    SMapped<Result<A, ()>, Self>,
    <Result<A, ()> as Arbitrary>::Parameters;
    args => static_map(any_with::<Result<A, ()>>(args), Result::into_iter)
);

lift1!(['static] IntoIter<A>, Probability; base, args => {
    maybe_ok_weighted(args, base, Just(())).prop_map(Result::into_iter)
});

#[cfg(test)]
mod test {
  use std::string::ParseError;

  use super::*;
  use crate::arbitrary::StrategyFor;
  use crate::strategy::NewTree;
  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  use crate::test_runner::PropertyResult;

  no_panic_test!(
      result    => Result<u8, u16>,
      into_iter => IntoIter<u8>,
      result_a_parse_error => Result<u8, ParseError>,
      result_parse_error_a => Result<ParseError, u8>
  );

  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  no_panic_test!(
      result_ok_never => Result<u8, !>,
      result_never_err => Result<!, u8>
  );

  /// Preserve generated values and comparisons for a native never-error result.
  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  type NeverLiftResult =
    PropertyResult<Result<u8, !>, (Result<u8, !>, Result<u8, !>), strict_test_support::ComparisonFailure<Result<u8, !>, Result<u8, !>>>;

  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  #[test]
  fn result_never_lift_preserves_payload() -> NeverLiftResult {
    use crate::arbitrary::functor::ArbitraryF1 as _;
    use crate::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let strategy = Result::<u8, !>::lift1_with(Just(37), ());
    runner.run_typed(&strategy, |value| {
      strict_test_support::ensure_eq(value, Ok(37), "lifting a strategy into Result<T, !> preserves its Ok payload")
    })
  }

  /// Native tree results for both one-sided inhabited Result strategies.
  type InhabitedTrees = (
    NewTree<StrategyFor<Result<u8, Infallible>>>,
    NewTree<StrategyFor<Result<Infallible, u8>>>,
  );

  #[test]
  fn result_infallible_variants_generate_only_inhabited_side() -> Result<(), strict_test_support::PredicateFailure<InhabitedTrees>> {
    use crate::arbitrary::any;
    use crate::strategy::Strategy as _;
    use crate::strategy::ValueTree as _;
    use crate::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let ok_tree = any::<Result<u8, Infallible>>().new_tree(&mut runner);
    let err_tree = any::<Result<Infallible, u8>>().new_tree(&mut runner);
    strict_test_support::ensure_that(
      (ok_tree, err_tree),
      "Result with an uninhabited variant generates only its inhabited side",
      |observed| {
        observed.0.as_ref().is_ok_and(|tree| tree.current().is_ok()) && observed.1.as_ref().is_ok_and(|tree| tree.current().is_err())
      },
    )
    .map(drop)
  }
}
