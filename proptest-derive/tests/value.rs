// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use proptest::prelude::{Arbitrary, any};
use proptest::strict::{TestResult, ensure_property};
use proptest_derive::Arbitrary;
use strict_test_support::{ensure, ensure_eq};

#[derive(Debug, Arbitrary)]
struct T0 {
    #[proptest(value = "42")]
    field: usize,
    #[proptest(value("24"))]
    bar: usize,
    #[proptest(value = "24 + 24usize")]
    baz: usize,
    #[proptest(value = 1337)]
    quux: usize,
    #[proptest(value(7331))]
    wibble: usize,
    #[proptest(value("2 * 2 + 9usize / 3"))]
    wobble: usize,
}

#[derive(Debug, Arbitrary)]
struct T1(#[proptest(value = "24")] usize);

#[derive(Debug, Arbitrary)]
enum T2 {
    V0,
    #[proptest(value = "T2::V1 { field: 1337 }")]
    V1 {
        field: usize,
    },
}

#[derive(Debug, Arbitrary)]
enum T3 {
    V0,
    #[proptest(value = "T3::V1(7331)")]
    V1(usize),
}

#[derive(Debug, Arbitrary)]
enum T4 {
    V0,
    V1 {
        #[proptest(value = "6")]
        field: usize,
    },
}

#[derive(Debug, Arbitrary)]
enum T5 {
    V0,
    V1(#[proptest(value = "9")] usize),
}

#[derive(Debug, Arbitrary)]
struct T6 {
    #[proptest(value = "\"alpha\".to_string()")]
    alpha: String,
    #[proptest(strategy = "0..100usize")]
    beta: usize,
}

fn foo() -> usize {
    42
}

#[derive(Debug, Arbitrary)]
struct CallFun {
    #[proptest(value = "foo()")]
    foo: usize,

    #[proptest(value(foo))]
    bar: usize,
}

#[test]
fn t0_fixed_fields() -> TestResult {
    ensure_property(
        &any::<T0>(),
        "every value spelling pins its struct field",
        |v| {
            ensure_eq(&v.field, &42, "string-literal value")?;
            ensure_eq(&v.bar, &24, "call-form string value")?;
            ensure_eq(&v.baz, &48, "expression value")?;
            ensure_eq(&v.quux, &1337, "bare integer value")?;
            ensure_eq(&v.wibble, &7331, "call-form integer value")?;
            ensure_eq(&v.wobble, &7, "arithmetic expression value")
        },
    )
}

#[test]
fn t1_field_always_24() -> TestResult {
    ensure_property(&any::<T1>(), "a tuple-struct value pins its field", |v| {
        ensure_eq(&v.0, &24, "the tuple field carries the value")
    })
}

#[test]
fn t2_v1_always_1337() -> TestResult {
    ensure_property(
        &any::<T2>(),
        "a struct-variant value pins its field",
        |v| {
            if let T2::V1 { field } = v {
                ensure_eq(
                    &field,
                    &1337,
                    "the variant field carries the value",
                )?;
            }
            Ok(())
        },
    )
}

#[test]
fn t3_v1_always_7331() -> TestResult {
    ensure_property(&any::<T3>(), "a tuple-variant value pins its field", |v| {
        if let T3::V1(v) = v {
            ensure_eq(&v, &7331, "the variant field carries the value")?;
        }
        Ok(())
    })
}

#[test]
fn t4_v1_always_1337() -> TestResult {
    ensure_property(
        &any::<T4>(),
        "a field-level value inside a struct variant pins the field",
        |v| {
            if let T4::V1 { field } = v {
                ensure_eq(&field, &6, "the variant field carries the value")?;
            }
            Ok(())
        },
    )
}

#[test]
fn t5_v1_always_7331() -> TestResult {
    ensure_property(
        &any::<T5>(),
        "a field-level value inside a tuple variant pins the field",
        |v| {
            if let T5::V1(v) = v {
                ensure_eq(&v, &9, "the variant field carries the value")?;
            }
            Ok(())
        },
    )
}

#[test]
fn t6_alpha_beta() -> TestResult {
    ensure_property(
        &any::<T6>(),
        "value and strategy fields coexist on one struct",
        |v| {
            ensure_eq(
                &v.alpha,
                &"alpha".to_string(),
                "the value field is pinned",
            )?;
            ensure(v.beta < 100, "the strategy field stays in range")
        },
    )
}

#[test]
fn call_fun_always_42() -> TestResult {
    ensure_property(
        &any::<CallFun>(),
        "fn-path value spellings call the function",
        |v| {
            ensure_eq(&v.foo, &42, "the string call-form value")?;
            ensure_eq(&v.bar, &42, "the bare fn-path value")
        },
    )
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
    assert_arbitrary::<CallFun>();
}
