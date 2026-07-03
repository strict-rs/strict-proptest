// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[macro_use]
extern crate proptest_derive;
use proptest::prelude::*;
use proptest::strict::{TestResult, ensure_property};
use strict_test_support::{ensure, ensure_eq};

#[derive(Debug, Arbitrary)]
enum T0 {
    #[proptest(params = "u8", value = "T0::V0(params / 2)")]
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

fn add(x: u8) -> u8 {
    x + 1
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
        |v| {
            let T0::V0(x) = v;
            ensure_eq(&x, &2, "the value expression halves the param")
        },
    )
}

#[test]
fn t1_test() -> TestResult {
    ensure_property(
        &any_with::<T1>(4),
        "a struct-variant value expression reads params",
        |v| {
            let T1::V0 { field: x } = v;
            ensure_eq(&x, &8, "the value expression doubles the param")
        },
    )
}

#[test]
fn t2_test_true() -> TestResult {
    ensure_property(
        &any_with::<T2>(4),
        "a field value expression sees a power-of-two param",
        |v| {
            let T2::V0(x) = v;
            ensure(x, "the power-of-two check holds for four")
        },
    )
}

#[test]
fn t2_test_false() -> TestResult {
    ensure_property(
        &any_with::<T2>(10),
        "a field value expression sees a non-power-of-two param",
        |v| {
            let T2::V0(x) = v;
            ensure(!x, "the power-of-two check fails for ten")
        },
    )
}

#[test]
fn t3_test() -> TestResult {
    ensure_property(
        &any_with::<T3>(4),
        "a struct-variant field value expression squares params",
        |v| {
            let T3::V0 { field: x } = v;
            ensure_eq(&x, &16, "the value expression squares the param")
        },
    )
}

#[test]
fn t4_test() -> TestResult {
    ensure_property(
        &any_with::<T4>(4),
        "a struct field value expression subtracts from params",
        |v| ensure_eq(&v.field, &1, "the value expression subtracts three"),
    )
}

#[test]
fn t5_test() -> TestResult {
    ensure_property(
        &any_with::<T5>(4),
        "a fn-call value expression receives params",
        |v| ensure_eq(&v.0, &5, "the fn-call value adds one"),
    )
}
