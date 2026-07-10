// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Coverage for the `#[proptest(value = ...)]` and `#[proptest(value(...))]`
//! modifiers that pin a field or variant to a constant.
//!
//! The derived types cover value expressions written as string literals, bare
//! integer literals, arithmetic expressions, and `fn`-path calls, applied to
//! struct fields, whole enum variants, and variant fields; each property
//! checks the generated value equals the pinned constant.

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

/// Shares its name with the local binding the generated `Value` constructor
/// introduces (`let value_fn: fn() -> _ = || <expr>; value_fn`). Because a
/// `let` target is not in scope inside its own initializer, `value_fn()` in the
/// pinned expression must resolve to this module-level fn, not the local.
const fn value_fn() -> usize {
    7788
}

#[derive(Debug, Arbitrary)]
struct ValueFnCollision {
    #[proptest(value = "value_fn()")]
    field: usize,
    #[proptest(value = "value_fn() + 1")]
    plus_one: usize,
}

#[test]
fn t0_fixed_fields() -> TestResult {
    ensure_property(
        &any::<T0>(),
        "every value spelling pins its struct field",
        |sample| {
            ensure_eq(&sample.field, &42, "string-literal value")?;
            ensure_eq(&sample.bar, &24, "call-form string value")?;
            ensure_eq(&sample.baz, &48, "expression value")?;
            ensure_eq(&sample.quux, &1337, "bare integer value")?;
            ensure_eq(&sample.wibble, &7331, "call-form integer value")?;
            ensure_eq(&sample.wobble, &7, "arithmetic expression value")
        },
    )
}

#[test]
fn t1_field_always_24() -> TestResult {
    ensure_property(
        &any::<T1>(),
        "a tuple-struct value pins its field",
        |sample| ensure_eq(&sample.0, &24, "the tuple field carries the value"),
    )
}

#[test]
fn t2_v1_always_1337() -> TestResult {
    ensure_property(
        &any::<T2>(),
        "a struct-variant value pins its field",
        |sample| {
            if let T2::V1 { field } = sample {
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
    ensure_property(
        &any::<T3>(),
        "a tuple-variant value pins its field",
        |sample| {
            if let T3::V1(field) = sample {
                ensure_eq(
                    &field,
                    &7331,
                    "the variant field carries the value",
                )?;
            }
            Ok(())
        },
    )
}

#[test]
fn t4_v1_always_1337() -> TestResult {
    ensure_property(
        &any::<T4>(),
        "a field-level value inside a struct variant pins the field",
        |sample| {
            if let T4::V1 { field } = sample {
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
        |sample| {
            if let T5::V1(field) = sample {
                ensure_eq(&field, &9, "the variant field carries the value")?;
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
        |sample| {
            ensure_eq(
                &sample.alpha,
                &"alpha".to_string(),
                "the value field is pinned",
            )?;
            ensure(sample.beta < 100, "the strategy field stays in range")
        },
    )
}

#[test]
fn call_fun_always_42() -> TestResult {
    ensure_property(
        &any::<CallFun>(),
        "fn-path value spellings call the function",
        |sample| {
            ensure_eq(&sample.foo, &42, "the string call-form value")?;
            ensure_eq(&sample.bar, &42, "the bare fn-path value")
        },
    )
}

#[test]
fn value_fn_name_collision_resolves_to_user_fn() -> TestResult {
    ensure_property(
        &any::<ValueFnCollision>(),
        "a `value_fn`-named user fn wins over the generated binding of the \
         same name",
        |sample| {
            ensure_eq(
                &sample.field,
                &7788,
                "the pinned expression calls the user's value_fn, not the \
                 generated local",
            )?;
            ensure_eq(
                &sample.plus_one,
                &7789,
                "a second collision site resolves to the user's value_fn too",
            )
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
    assert_arbitrary::<ValueFnCollision>();
}
