//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::ops`.

#[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
use core::ops::CoroutineState;
use core::ops::Range;
use core::ops::RangeFrom;
use core::ops::RangeFull;
use core::ops::RangeInclusive;
use core::ops::RangeTo;
use core::ops::RangeToInclusive;

#[cfg(feature = "alt-stable")]
use crate::alt_stable::CoroutineState as StableCoroutineState;
use crate::arbitrary::Arbitrary;
use crate::arbitrary::SMapped;
use crate::arbitrary::any_with;
#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
use crate::arbitrary::functor;
use crate::std_facade::Rc;
#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
use crate::strategy::BoxedStrategy;
use crate::strategy::Strategy as _;
#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
use crate::strategy::TupleUnion;
#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
use crate::strategy::WeightedStrategy;
use crate::strategy::statics::static_map;

arbitrary!(RangeFull; ..);
wrap_ctor!(RangeFrom, |endpoint| endpoint..);
wrap_ctor!(RangeTo, |endpoint| ..endpoint);

wrap_ctor!(RangeToInclusive, |endpoint| ..=endpoint);

arbitrary!(
    [A: PartialOrd + Arbitrary] RangeInclusive<A>,
    SMapped<(A, A), Self>, product_type![A::Parameters, A::Parameters];
    args => static_map(any_with::<(A, A)>(args),
        |(first, second)| if second < first { second..=first } else { first..=second })
);

lift1!([PartialOrd] RangeInclusive<A>; base => {
    let base = Rc::new(base);
    (Rc::clone(&base), base).prop_map(|(first, second)| if second < first { second..=first } else { first..=second })
});

arbitrary!(
    [A: PartialOrd + Arbitrary] Range<A>,
    SMapped<(A, A), Self>, product_type![A::Parameters, A::Parameters];
    args => static_map(any_with::<(A, A)>(args),
        |(first, second)| if second < first { second..first } else { first..second })
);

lift1!([PartialOrd] Range<A>; base => {
    let base = Rc::new(base);
    (Rc::clone(&base), base).prop_map(|(first, second)| if second < first { second..first } else { first..second })
});

#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
/// Implement `Arbitrary` for coroutine-state-shaped enums.
macro_rules! coroutine_state_arbitrary {
    ($typ:ident) => {
        arbitrary!(
            [Y: Arbitrary, R: Arbitrary] $typ<Y, R>,
            TupleUnion<(WeightedStrategy<SMapped<Y, Self>>, WeightedStrategy<SMapped<R, Self>>)>,
            product_type![Y::Parameters, R::Parameters];
            args => {
                let product_unpack![y, complete_params] = args;
                prop_oneof![
                    static_map(any_with::<Y>(y), $typ::Yielded),
                    static_map(any_with::<R>(complete_params), $typ::Complete)
                ]
            }
        );
    };
}

#[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
coroutine_state_arbitrary!(CoroutineState);

#[cfg(feature = "alt-stable")]
coroutine_state_arbitrary!(StableCoroutineState);

#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
use core::fmt;

#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
/// Implement functor lifting for coroutine-state-shaped enums.
macro_rules! coroutine_state_functor {
  ($typ:ident) => {
    impl<A: fmt::Debug + 'static, B: fmt::Debug + 'static> functor::ArbitraryF2<A, B> for $typ<A, B> {
      type Parameters = ();

      fn lift2_with<AS, BS>(fst: AS, snd: BS, _args: Self::Parameters) -> BoxedStrategy<Self>
      where
        AS: crate::strategy::Strategy<Value = A> + 'static,
        BS: crate::strategy::Strategy<Value = B> + 'static,
      {
        prop_oneof![fst.prop_map($typ::Yielded), snd.prop_map($typ::Complete)].boxed()
      }
    }
  };
}

#[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
coroutine_state_functor!(CoroutineState);

#[cfg(feature = "alt-stable")]
coroutine_state_functor!(StableCoroutineState);

#[cfg(test)]
mod test {
  use super::*;

  no_panic_test!(
      range_full => RangeFull,
      range_from => RangeFrom<usize>,
      range_to   => RangeTo<usize>,
      range      => Range<usize>,
      range_inclusive => RangeInclusive<usize>,
      range_to_inclusive => RangeToInclusive<usize>
  );

  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  no_panic_test!(
      generator_state => CoroutineState<u32, u64>
  );

  #[cfg(feature = "alt-stable")]
  no_panic_test!(
      stable_generator_state => StableCoroutineState<u32, u64>
  );

  #[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
  macro_rules! coroutine_state_generates_both_variants {
    ($name:ident, $typ:ident) => {
      #[test]
      fn $name() -> Result<(), strict_test_support::TestFailure> {
        use strict_test_support::ensure;
        use strict_test_support::ensure_some;

        use crate::arbitrary::any;
        use crate::strategy::Strategy;
        use crate::strategy::ValueTree;
        use crate::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = any::<$typ<bool, bool>>();
        let mut saw_yielded = false;
        let mut saw_complete = false;

        for _ in 0..64 {
          let sample = ensure_some(
            strategy.new_tree(&mut runner).ok(),
            "CoroutineState strategy generates a value tree",
          )?
          .current();
          saw_yielded |= matches!(sample, $typ::Yielded(_));
          saw_complete |= matches!(sample, $typ::Complete(_));
        }

        ensure(saw_yielded, "CoroutineState generation can yield")?;
        ensure(saw_complete, "CoroutineState generation can complete")
      }
    };
  }

  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  coroutine_state_generates_both_variants!(coroutine_state_generates_both_variants, CoroutineState);

  #[cfg(feature = "alt-stable")]
  coroutine_state_generates_both_variants!(stable_coroutine_state_generates_both_variants, StableCoroutineState);
}
