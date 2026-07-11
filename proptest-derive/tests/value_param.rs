// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for `#[proptest(value = ...)]` combined with
//! `#[proptest(params = ...)]`.
//!
//! Derives `Arbitrary` for structs and enum variants whose `value`
//! expression reads the supplied `params`, then drives each type with
//! `any_with` to confirm the generated field equals the value computed
//! from the parameter (halving, doubling, squaring, subtraction, and a
//! `fn` call).

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::strict::{TestResult, ensure_property};
    use proptest_derive::Arbitrary;
    use strict_test_support::{ensure, ensure_eq};

    #[derive(Debug, Arbitrary)]
    enum T0 {
        #[proptest(params = "u8", value = "T0::V0(params.div_euclid(2))")]
        V0(u8),
    }

    #[derive(Debug, Arbitrary)]
    enum T1 {
        #[proptest(params = "u8", value = "T1::V0 { field: params * 2 }")]
        V0 { field: u8 },
    }

    #[derive(Debug, Arbitrary)]
    enum T2 {
        V0(#[proptest(params = "u8", value = "params.is_power_of_two()")] bool),
    }

    #[derive(Debug, Arbitrary)]
    enum T3 {
        V0 {
            #[proptest(params = "u8", value = "params * params")]
            field: u8,
        },
    }

    #[derive(Debug, Arbitrary)]
    struct T4 {
        #[proptest(params = "u8", value = "params - 3")]
        field: u8,
    }

    #[allow(
        clippy::single_call_fn,
        reason = "fn-path value expression that adds one to the params argument for T5"
    )]
    const fn add(x: u8) -> u8 {
        x.saturating_add(1)
    }

    #[derive(Debug, Arbitrary)]
    struct T5(#[proptest(params = "u8", value = "add(params)")] u8);

    #[test]
    fn asserting_arbitrary() {
        fn assert_arbitrary<T: Arbitrary>() {}

        assert_arbitrary::<T0>();
        assert_arbitrary::<T1>();
        assert_arbitrary::<T2>();
        assert_arbitrary::<T3>();
        assert_arbitrary::<T4>();
        assert_arbitrary::<T5>();
    }

    #[test]
    fn t0_test() -> TestResult {
        ensure_property(
            &any_with::<T0>(4),
            "a tuple-variant value expression reads params",
            |sample| {
                let T0::V0(x) = sample;
                ensure_eq(&x, &2, "the value expression halves the param")
            },
        )
    }

    #[test]
    fn t1_test() -> TestResult {
        ensure_property(
            &any_with::<T1>(4),
            "a struct-variant value expression reads params",
            |sample| {
                let T1::V0 { field: x } = sample;
                ensure_eq(&x, &8, "the value expression doubles the param")
            },
        )
    }

    #[test]
    fn t2_test_true() -> TestResult {
        ensure_property(
            &any_with::<T2>(4),
            "a field value expression sees a power-of-two param",
            |sample| {
                let T2::V0(x) = sample;
                ensure(x, "the power-of-two check holds for four")
            },
        )
    }

    #[test]
    fn t2_test_false() -> TestResult {
        ensure_property(
            &any_with::<T2>(10),
            "a field value expression sees a non-power-of-two param",
            |sample| {
                let T2::V0(x) = sample;
                ensure(!x, "the power-of-two check fails for ten")
            },
        )
    }

    #[test]
    fn t3_test() -> TestResult {
        ensure_property(
            &any_with::<T3>(4),
            "a struct-variant field value expression squares params",
            |sample| {
                let T3::V0 { field: x } = sample;
                ensure_eq(&x, &16, "the value expression squares the param")
            },
        )
    }

    #[test]
    fn t4_test() -> TestResult {
        ensure_property(
            &any_with::<T4>(4),
            "a struct field value expression subtracts from params",
            |sample| {
                ensure_eq(
                    &sample.field,
                    &1,
                    "the value expression subtracts three",
                )
            },
        )
    }

    #[test]
    fn t5_test() -> TestResult {
        ensure_property(
            &any_with::<T5>(4),
            "a fn-call value expression receives params",
            |sample| ensure_eq(&sample.0, &5, "the fn-call value adds one"),
        )
    }
}
