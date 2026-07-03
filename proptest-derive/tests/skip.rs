// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(never_type)]
#![allow(dead_code, unreachable_code)]

use proptest::prelude::{Arbitrary, any};
use proptest::strict::{TestResult, ensure_property};
use proptest_derive::Arbitrary;
use strict_test_support::ensure;

#[derive(Debug, Arbitrary, PartialEq)]
enum Ty1 {
    V1,
    V2(!),
    #[proptest(skip)]
    V3,
}

#[derive(Debug, Arbitrary, PartialEq)]
enum Ty2 {
    V1,
    V2,
    #[proptest(skip)]
    V3,
    #[proptest(skip)]
    V4,
}

#[test]
fn ty1_always_v1() -> TestResult {
    ensure_property(
        &any::<Ty1>(),
        "skipped and uninhabited variants never generate",
        |v| {
            ensure(
                v == Ty1::V1,
                "only the inhabited, unskipped variant appears",
            )
        },
    )
}

#[test]
fn ty_always_1_or_2() -> TestResult {
    ensure_property(
        &any::<Ty2>(),
        "multiple skipped variants never generate",
        |v| {
            ensure(
                v == Ty2::V1 || v == Ty2::V2,
                "only the unskipped variants appear",
            )
        },
    )
}

#[test]
fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<Ty1>();
    assert_arbitrary::<Ty2>();
}
