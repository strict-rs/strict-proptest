//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for tuples.

use crate::arbitrary::{Arbitrary, any_with};

/// Implements `Arbitrary` for a tuple of the given arity.
///
/// The tuple's `Parameters` is the `product_type!` of its elements' own
/// parameters and its `Strategy` is the tuple of their strategies (a tuple of
/// strategies is itself a `Strategy`); `arbitrary_with` unpacks the
/// per-element params and delegates each slot to `any_with`.
macro_rules! impl_tuple {
    ($($typ: ident),*) => {
        impl<$($typ : Arbitrary),*> Arbitrary for ($($typ,)*) {
            type Parameters = product_type![$($typ::Parameters),*];
            type Strategy = ($($typ::Strategy,)*);
            fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
                #[allow(non_snake_case)]
                let product_unpack![$($typ),*] = args;
                ($(any_with::<$typ>($typ)),*,)
            }
        }
    };
}

arbitrary!((); ());
impl_tuple!(T0);
impl_tuple!(T0, T1);
impl_tuple!(T0, T1, T2);
impl_tuple!(T0, T1, T2, T3);
impl_tuple!(T0, T1, T2, T3, T4);
impl_tuple!(T0, T1, T2, T3, T4, T5);
impl_tuple!(T0, T1, T2, T3, T4, T5, T6);
impl_tuple!(T0, T1, T2, T3, T4, T5, T6, T7);
impl_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8);
impl_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);

#[cfg(test)]
mod test {
    use strict_test_support::{TestFailure, ensure, ensure_some};

    use super::*;
    use crate::strategy::{Just, Strategy, ValueTree};
    use crate::test_runner::TestRunner;

    no_panic_test!(
        tuple_n10 => ((), bool, u8, u16, u32, u64, i8, i16, i32, i64)
    );

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ParamEcho(u8);

    impl Arbitrary for ParamEcho {
        type Parameters = u8;
        type Strategy = Just<Self>;

        fn arbitrary_with(param: Self::Parameters) -> Self::Strategy {
            Just(ParamEcho(param))
        }
    }

    #[test]
    fn tuple_parameters_preserve_product_order() -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();

        let one = ensure_some(
            any_with::<(ParamEcho,)>(product_pack![7])
                .new_tree(&mut runner)
                .ok(),
            "single-element tuple strategy generates a value tree",
        )?
        .current();
        ensure(
            one == (ParamEcho(7),),
            "a single tuple element receives its parameter",
        )?;

        let two = ensure_some(
            any_with::<(ParamEcho, ParamEcho)>(product_pack![1, 2])
                .new_tree(&mut runner)
                .ok(),
            "two-element tuple strategy generates a value tree",
        )?
        .current();
        ensure(
            two == (ParamEcho(1), ParamEcho(2)),
            "tuple parameters stay in field order",
        )?;

        let reversed = ensure_some(
            any_with::<(ParamEcho, ParamEcho)>(product_pack![2, 1])
                .new_tree(&mut runner)
                .ok(),
            "reversed tuple strategy generates a value tree",
        )?
        .current();
        ensure(
            reversed != (ParamEcho(1), ParamEcho(2)),
            "reversing parameters changes the generated tuple",
        )
    }
}
