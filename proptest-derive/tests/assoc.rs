// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for `#[derive(Arbitrary)]` on fields typed as
//! associated-type projections.
//!
//! Derives `Arbitrary` for structs whose field types are associated-type
//! projections in every spelling (`<TypeA as Func>::Out`, `Tyvar::OutB`,
//! `<Tyvar as FuncB>::OutB`, and nested projections), including such
//! projections wrapped in `Vec`. The derive must still infer the correct
//! `Arbitrary` bounds, and the generated projected fields carry their
//! pinned values.

use proptest::prelude::{Arbitrary, any};
use proptest::strict::{TestResult, ensure_property};
use proptest_derive::Arbitrary;
use strict_test_support::ensure_eq;

trait Func {
    type Out;
}
trait FuncA {
    type OutA: FuncB;
}
trait FuncB {
    type OutB;
}

#[derive(Debug)]
struct TypeA;

#[derive(Debug)]
struct TypeB;

#[derive(Debug, Arbitrary)]
struct OutTy {
    #[proptest(value = "42")]
    val: usize,
}

impl Func for TypeA {
    type Out = OutTy;
}
impl FuncA for TypeA {
    type OutA = TypeB;
}
impl FuncB for TypeB {
    type OutB = OutTy;
}

#[derive(Debug, Arbitrary)]
struct T0 {
    field: <TypeA as Func>::Out,
}

#[derive(Debug, Arbitrary)]
struct T1 {
    _field: Vec<u8>,
}

#[derive(Debug, Arbitrary)]
struct T2 {
    _field: Vec<Vec<u8>>,
}

#[derive(Debug, Arbitrary)]
struct T3 {
    field: Vec<<TypeA as Func>::Out>,
}

#[derive(Debug, Arbitrary)]
struct T4<Tyvar: FuncB> {
    field: Tyvar::OutB,
}

#[derive(Arbitrary)]
struct T5<Tyvar: FuncB> {
    field: <Tyvar>::OutB,
}

#[derive(Arbitrary)]
struct T6<Tyvar: FuncB> {
    field: <Tyvar as FuncB>::OutB,
}

#[derive(Arbitrary)]
struct T7<Tyvar: FuncA> {
    field: <Tyvar::OutA as FuncB>::OutB,
}

#[derive(Arbitrary)]
struct T8<Tyvar: FuncA> {
    field: <<Tyvar>::OutA as FuncB>::OutB,
}

#[derive(Arbitrary)]
struct T9<Tyvar: FuncA> {
    field: <<Tyvar as FuncA>::OutA as FuncB>::OutB,
}

#[derive(Debug, Arbitrary)]
struct T10<Tyvar: FuncB> {
    field: Vec<Tyvar::OutB>,
}

#[derive(Arbitrary)]
struct T11<Tyvar: FuncB> {
    field: Vec<<Tyvar>::OutB>,
}

#[derive(Arbitrary)]
struct T12<Tyvar: FuncB> {
    field: Vec<<Tyvar as FuncB>::OutB>,
}

#[derive(Arbitrary)]
struct T13<Tyvar: FuncA> {
    field: Vec<<Tyvar::OutA as FuncB>::OutB>,
}

#[derive(Arbitrary)]
struct T14<Tyvar: FuncA> {
    field: Vec<<<Tyvar>::OutA as FuncB>::OutB>,
}

#[derive(Arbitrary)]
struct T15<Tyvar: FuncA> {
    field: Vec<<<Tyvar as FuncA>::OutA as FuncB>::OutB>,
}

macro_rules! debug {
    ($trait: path, $ty: ident) => {
        impl<T: $trait> ::std::fmt::Debug for $ty<T> {
            fn fmt(
                &self,
                fmt: &mut ::std::fmt::Formatter<'_>,
            ) -> Result<(), ::std::fmt::Error> {
                fmt.debug_struct(stringify!($ty))
                    .field("field", &"<redacted>")
                    .finish()
            }
        }
    };
}

debug!(FuncB, T5);
debug!(FuncB, T6);
debug!(FuncA, T7);
debug!(FuncA, T8);
debug!(FuncA, T9);

debug!(FuncB, T11);
debug!(FuncB, T12);
debug!(FuncA, T13);
debug!(FuncA, T14);
debug!(FuncA, T15);

#[test]
fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<T0>();
    assert_arbitrary::<T1>();
    assert_arbitrary::<T2>();
    assert_arbitrary::<T3>();

    assert_arbitrary::<T4<TypeB>>();
    assert_arbitrary::<T5<TypeB>>();
    assert_arbitrary::<T6<TypeB>>();
    assert_arbitrary::<T7<TypeA>>();
    assert_arbitrary::<T8<TypeA>>();
    assert_arbitrary::<T9<TypeA>>();

    assert_arbitrary::<T10<TypeB>>();
    assert_arbitrary::<T11<TypeB>>();
    assert_arbitrary::<T12<TypeB>>();
    assert_arbitrary::<T13<TypeA>>();
    assert_arbitrary::<T14<TypeA>>();
    assert_arbitrary::<T15<TypeA>>();
}

/// Every element of an associated-type collection field carries the pinned
/// value.
fn ensure_all_42<'a, I>(items: I) -> TestResult
where
    I: IntoIterator<Item = &'a OutTy>,
{
    for element in items {
        ensure_eq(
            &element.val,
            &42,
            "every generated element is pinned to 42",
        )?;
    }
    Ok(())
}

#[test]
fn t0_field_val_42() -> TestResult {
    ensure_property(&any::<T0>(), "a projected field generates", |sample| {
        ensure_eq(&sample.field.val, &42, "the projected field is pinned")
    })
}

#[test]
fn t1_no_panic() -> TestResult {
    ensure_property(&any::<T1>(), "a projected field generates", |_| Ok(()))
}

#[test]
fn t2_no_panic() -> TestResult {
    ensure_property(&any::<T2>(), "a projected field generates", |_| Ok(()))
}

#[test]
fn t3_all_42() -> TestResult {
    ensure_property(
        &any::<T3>(),
        "a projected collection field generates",
        |sample| ensure_all_42(sample.field.iter()),
    )
}

#[test]
fn t4_field_val_42() -> TestResult {
    ensure_property(
        &any::<T4<TypeB>>(),
        "a projected field generates",
        |sample| {
            ensure_eq(&sample.field.val, &42, "the projected field is pinned")
        },
    )
}

#[test]
fn t5_field_val_42() -> TestResult {
    ensure_property(
        &any::<T5<TypeB>>(),
        "a projected field generates",
        |sample| {
            ensure_eq(&sample.field.val, &42, "the projected field is pinned")
        },
    )
}

#[test]
fn t6_field_val_42() -> TestResult {
    ensure_property(
        &any::<T6<TypeB>>(),
        "a projected field generates",
        |sample| {
            ensure_eq(&sample.field.val, &42, "the projected field is pinned")
        },
    )
}

#[test]
fn t7_field_val_42() -> TestResult {
    ensure_property(
        &any::<T7<TypeA>>(),
        "a projected field generates",
        |sample| {
            ensure_eq(&sample.field.val, &42, "the projected field is pinned")
        },
    )
}

#[test]
fn t8_field_val_42() -> TestResult {
    ensure_property(
        &any::<T8<TypeA>>(),
        "a projected field generates",
        |sample| {
            ensure_eq(&sample.field.val, &42, "the projected field is pinned")
        },
    )
}

#[test]
fn t9_field_val_42() -> TestResult {
    ensure_property(
        &any::<T9<TypeA>>(),
        "a projected field generates",
        |sample| {
            ensure_eq(&sample.field.val, &42, "the projected field is pinned")
        },
    )
}

#[test]
fn t10_all_42() -> TestResult {
    ensure_property(
        &any::<T10<TypeB>>(),
        "a projected collection field generates",
        |sample| ensure_all_42(sample.field.iter()),
    )
}

#[test]
fn t11_all_42() -> TestResult {
    ensure_property(
        &any::<T11<TypeB>>(),
        "a projected collection field generates",
        |sample| ensure_all_42(sample.field.iter()),
    )
}

#[test]
fn t12_all_42() -> TestResult {
    ensure_property(
        &any::<T12<TypeB>>(),
        "a projected collection field generates",
        |sample| ensure_all_42(sample.field.iter()),
    )
}

#[test]
fn t13_all_42() -> TestResult {
    ensure_property(
        &any::<T13<TypeA>>(),
        "a projected collection field generates",
        |sample| ensure_all_42(sample.field.iter()),
    )
}

#[test]
fn t14_all_42() -> TestResult {
    ensure_property(
        &any::<T14<TypeA>>(),
        "a projected collection field generates",
        |sample| ensure_all_42(sample.field.iter()),
    )
}

#[test]
fn t15_all_42() -> TestResult {
    ensure_property(
        &any::<T15<TypeA>>(),
        "a projected collection field generates",
        |sample| ensure_all_42(sample.field.iter()),
    )
}
