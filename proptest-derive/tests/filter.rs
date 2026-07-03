// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use proptest::prelude::*;
use proptest::strict::{TestResult, ensure_property};
use proptest_derive::Arbitrary;
use strict_test_support::ensure;

fn even(x: &usize) -> bool {
    x.is_multiple_of(2)
}

fn rem3(x: &usize) -> bool {
    x.is_multiple_of(3)
}

#[derive(Copy, Clone)]
struct Param(usize);

impl Default for Param {
    fn default() -> Self {
        Param(100)
    }
}

#[derive(Debug, Arbitrary)]
#[proptest(filter("|x| x.foo % 3 == 0"))]
struct T0 {
    #[proptest(no_params, filter(even))]
    foo: usize,
    #[proptest(filter("|x| x % 2 == 1"))]
    bar: usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x % 2 == 1")]
    baz: usize,
    #[proptest(value = "42", filter(even))]
    quux: usize,
    #[proptest(params(Param), strategy("0..=params.0"), filter("|x| *x > 2"))]
    wibble: usize,
}

#[derive(Debug, Arbitrary)]
#[proptest(params(Param))]
#[proptest(filter("|x| x.foo % 3 == 0"))]
struct T1 {
    #[proptest(filter(even))]
    foo: usize,
    #[proptest(filter("|x| x % 2 == 1"))]
    bar: usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x % 2 == 1")]
    baz: usize,
    #[proptest(value = "42", filter(even))]
    quux: usize,
    #[proptest(strategy("0..=params.0"), filter("|x| *x > 2"))]
    wibble: usize,
}

#[derive(Debug, Arbitrary)]
#[proptest(filter("|x| x.0 % 3 == 0"))]
struct T2(
    #[proptest(no_params, filter(even))] usize,
    #[proptest(filter("|x| x % 2 == 1"))] usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x % 2 == 1")] usize,
    #[proptest(value = "42", filter(even))] usize,
    #[proptest(params(Param), strategy("0..=params.0"), filter("|x| *x > 2"))]
    usize,
);

#[derive(Debug, Arbitrary)]
#[proptest(filter("|x| x.0 % 3 == 0"))]
struct T3(
    #[proptest(no_params, filter(even))] usize,
    #[proptest(filter("|x| x % 2 == 1"))] usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x % 2 == 1")] usize,
    #[proptest(value = "42", filter(even))] usize,
    #[proptest(params(Param), strategy("0..=params.0"), filter("|x| *x > 2"))]
    usize,
);

fn is_v0(v: &T4) -> bool {
    matches!(v, T4::V0 { .. })
}

#[derive(Debug, Arbitrary)]
#[proptest(filter(is_v0))]
enum T4 {
    V0 {
        #[proptest(filter(even))]
        field: usize,
    },
    V1,
}

fn t5_v0_rem_3(v: &T5) -> bool {
    if let T5::V0 { field } = v {
        rem3(field)
    } else {
        false
    }
}

fn t5_v1_rem_5(v: &T5) -> bool {
    if let T5::V1(field) = v {
        field.is_multiple_of(5)
    } else {
        false
    }
}

#[derive(Debug, Arbitrary)]
enum T5 {
    #[proptest(filter(t5_v0_rem_3))]
    V0 {
        #[proptest(filter(even))]
        field: usize,
    },
    #[proptest(
        strategy("(0..1000usize).prop_map(T5::V1)"),
        filter(t5_v1_rem_5)
    )]
    V1(usize),
}

fn t6_v0_rem_3(v: &T6) -> bool {
    if let T6::V0 { field } = v {
        rem3(field)
    } else {
        false
    }
}

fn t6_v1_rem_5(v: &T6) -> bool {
    if let T6::V1(field) = v {
        field.is_multiple_of(5)
    } else {
        false
    }
}

#[derive(Debug, Arbitrary)]
#[proptest(params(Param))]
enum T6 {
    #[proptest(filter(t6_v0_rem_3))]
    V0 {
        #[proptest(filter(even))]
        field: usize,
    },
    #[proptest(strategy("(0..params.0).prop_map(T6::V1)"), filter(t6_v1_rem_5))]
    V1(usize),
}

#[derive(Debug, Arbitrary)]
struct T7 {
    #[proptest(filter(even), filter(rem3))]
    foo: usize,
}

#[test]
fn t0_test() -> TestResult {
    ensure_property(
        &any::<T0>(),
        "every filter spelling holds on a named struct",
        |v| {
            ensure(
                even(&v.foo) && rem3(&v.foo),
                "field and container filters compose",
            )?;
            ensure(!even(&v.bar), "the closure-string filter holds")?;
            ensure(
                !even(&v.baz) && v.baz < 100,
                "the filter composes with a strategy",
            )?;
            ensure(
                even(&v.quux) && v.quux == 42,
                "the filter composes with a value",
            )?;
            ensure(
                v.wibble > 2 && v.wibble <= 100,
                "the filter composes with params and a strategy",
            )
        },
    )
}

#[test]
fn t1_test() -> TestResult {
    ensure_property(
        &any::<T1>(),
        "every filter spelling holds under container params",
        |v| {
            ensure(
                even(&v.foo) && v.foo % 3 == 0,
                "field and container filters compose",
            )?;
            ensure(!even(&v.bar), "the closure-string filter holds")?;
            ensure(
                !even(&v.baz) && v.baz < 100,
                "the filter composes with a strategy",
            )?;
            ensure(
                even(&v.quux) && v.quux == 42,
                "the filter composes with a value",
            )?;
            ensure(
                v.wibble > 2 && v.wibble <= 100,
                "the filter composes with the params strategy",
            )
        },
    )
}

#[test]
fn t2_test() -> TestResult {
    ensure_property(
        &any::<T2>(),
        "every filter spelling holds on a tuple struct",
        |v| {
            ensure(
                even(&v.0) && v.0 % 3 == 0,
                "field and container filters compose",
            )?;
            ensure(!even(&v.1), "the closure-string filter holds")?;
            ensure(
                !even(&v.2) && v.2 < 100,
                "the filter composes with a strategy",
            )?;
            ensure(
                even(&v.3) && v.3 == 42,
                "the filter composes with a value",
            )?;
            ensure(
                v.4 > 2 && v.4 <= 100,
                "the filter composes with params and a strategy",
            )
        },
    )
}

#[test]
fn t3_test() -> TestResult {
    ensure_property(
        &any::<T3>(),
        "the duplicate tuple-struct spelling holds",
        |v| {
            ensure(
                even(&v.0) && v.0 % 3 == 0,
                "field and container filters compose",
            )?;
            ensure(!even(&v.1), "the closure-string filter holds")?;
            ensure(
                !even(&v.2) && v.2 < 100,
                "the filter composes with a strategy",
            )?;
            ensure(
                even(&v.3) && v.3 == 42,
                "the filter composes with a value",
            )?;
            ensure(
                v.4 > 2 && v.4 <= 100,
                "the filter composes with params and a strategy",
            )
        },
    )
}

#[test]
fn t4_test() -> TestResult {
    ensure_property(
        &any::<T4>(),
        "a container fn-filter keeps only the matching variant",
        |v| {
            ensure(
                if let T4::V0 { field } = v {
                    even(&field)
                } else {
                    false
                },
                "only V0 with an even field survives the filters",
            )
        },
    )
}

#[test]
fn t5_test() -> TestResult {
    ensure_property(
        &any::<T5>(),
        "variant-level filters hold per variant",
        |v| match v {
            T5::V0 { field } => ensure(
                rem3(&field) && even(&field),
                "V0 satisfies the variant and field filters",
            ),
            T5::V1(field) => ensure(
                field < 1000 && field % 5 == 0,
                "V1 satisfies the strategy filter",
            ),
        },
    )
}

#[test]
fn t6_test() -> TestResult {
    ensure_property(
        &any::<T6>(),
        "variant-level filters hold under container params",
        |v| match v {
            T6::V0 { field } => ensure(
                rem3(&field) && even(&field),
                "V0 satisfies the variant and field filters",
            ),
            T6::V1(field) => ensure(
                field < 100 && field % 5 == 0,
                "V1 satisfies the params strategy filter",
            ),
        },
    )
}

#[test]
fn t7_test() -> TestResult {
    ensure_property(&any::<T7>(), "repeated field filters accumulate", |v| {
        ensure(
            even(&v.foo) && rem3(&v.foo),
            "both accumulated filters hold",
        )
    })
}

#[test]
fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<T0>();
    assert_arbitrary::<T1>();
    assert_arbitrary::<T2>();
    assert_arbitrary::<T3>();
    assert_arbitrary::<T4>();
    assert_arbitrary::<T5>();
    assert_arbitrary::<T6>();
    assert_arbitrary::<T7>();
}
