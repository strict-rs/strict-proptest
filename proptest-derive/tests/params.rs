// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Coverage for the `#[proptest(params(...))]` and `#[proptest(no_params)]`
//! modifiers that thread a custom parameter type into a derived strategy.
//!
//! The derived types exercise container-level and field-level params (in both
//! the `params(T)` and `params = "T"` spellings), `no_params` overrides, field
//! strategies that read `params`, and per-field "parallel" params; each test
//! drives generation through `any_with` to supply the parameters.

use proptest::prelude::{Arbitrary, any_with};
use proptest::strict::{TestResult, ensure_property};
use proptest_derive::Arbitrary;
use strict_test_support::{ensure, ensure_eq, ensure_some};

struct ComplexType {
    max: u64,
}

impl Default for ComplexType {
    fn default() -> Self {
        Self { max: 10 }
    }
}

#[derive(Debug, Arbitrary)]
#[proptest(params(ComplexType))]
struct TopHasParams {
    _string: usize,
    #[proptest(strategy = "0..params.max")]
    int: u64,
}

#[derive(Debug, Arbitrary)]
#[proptest(no_params)]
struct TopNoParams {
    _stuff: usize,
}

#[derive(Debug, Arbitrary)]
struct InnerNoParams {
    string: String,
    #[proptest(no_params)]
    has: TopHasParams,
}

#[derive(Debug, Arbitrary)]
#[proptest(params(u64))]
struct Tpis {
    #[proptest(strategy = "\"a+\"")]
    string: String,
    #[proptest(strategy = "3..=params")]
    int: u64,
}

#[derive(Debug, Arbitrary)]
struct Parallel {
    #[proptest(params = "&'static str", strategy = "params")]
    string: String,
    #[proptest(params(u8), strategy = "0i64..params as i64")]
    int: i64,
}

#[derive(Debug, Arbitrary)]
struct Parallel2 {
    #[proptest(params("&'static str"), strategy = "params")]
    _string: String,
    #[proptest(params("u8"), strategy = "0i64..params as i64")]
    _int: i64,
}

const MAX: ComplexType = ComplexType { max: 5 };

#[test]
fn top_has_params() -> TestResult {
    ensure_property(
        &any_with::<TopHasParams>(MAX),
        "container params thread into the field strategy",
        |sample| ensure(sample.int < 5, "int stays below the params max"),
    )
}

#[test]
fn top_no_params() -> TestResult {
    ensure_property(
        &any_with::<TopNoParams>(()),
        "a no_params container generates under unit params",
        |_| Ok(()),
    )
}

#[test]
fn inner_params() -> TestResult {
    ensure_property(
        &any_with::<InnerNoParams>("\\s+".into()),
        "an inner no_params field keeps its own defaults",
        |inner| {
            ensure(
                inner.has.int < 10,
                "the no_params field keeps its default bound",
            )?;
            ensure(
                inner.string.trim().is_empty(),
                "the outer string obeys the whitespace regex param",
            )
        },
    )
}

#[test]
fn top_param_inner_strat() -> TestResult {
    ensure_property(
        &any_with::<Tpis>(6),
        "container params reach a range field strategy",
        |inner| {
            ensure(inner.int <= 6, "int stays at or below the param")?;
            ensure(inner.int >= 3, "int stays at or above the range start")?;
            ensure_eq(
                &0,
                &inner
                    .string
                    .split("a")
                    .filter(|segment| !segment.is_empty())
                    .count(),
                "the string field is made of a's only",
            )
        },
    )
}

#[test]
fn parallel_params() -> TestResult {
    ensure_property(
        &any_with::<Parallel>(("[0-9]", 3)),
        "parallel per-field params drive each field",
        |inner| {
            ensure(inner.int >= 0, "int stays at or above zero")?;
            ensure(inner.int < 3, "int stays below the u8 param")?;
            let first = ensure_some(
                inner.string.chars().next(),
                "the regex-driven string is non-empty",
            )?;
            ensure(
                first.is_ascii_digit(),
                "the regex-driven string starts with a digit",
            )
        },
    )
}

#[test]
fn parallel_params2() -> TestResult {
    ensure_property(
        &any_with::<Parallel>(("[0-9]", 3)),
        "parallel per-field params drive each field in the string spelling",
        |inner| {
            ensure(inner.int >= 0, "int stays at or above zero")?;
            ensure(inner.int < 3, "int stays below the u8 param")?;
            let first = ensure_some(
                inner.string.chars().next(),
                "the regex-driven string is non-empty",
            )?;
            ensure(
                first.is_ascii_digit(),
                "the regex-driven string starts with a digit",
            )
        },
    )
}

#[test]
fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<TopHasParams>();
    assert_arbitrary::<TopNoParams>();
    assert_arbitrary::<InnerNoParams>();
    assert_arbitrary::<Tpis>();
    assert_arbitrary::<Parallel>();
    assert_arbitrary::<Parallel2>();
}
