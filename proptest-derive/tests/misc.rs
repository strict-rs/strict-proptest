// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use proptest::prelude::{Arbitrary, any, any_with};
use proptest::strategy::Just;
use proptest::strict::{TestResult, ensure_property};
use proptest_derive::Arbitrary;
use strict_test_support::{ensure, ensure_eq};

// TODO: An idea.
/*
#[derive(Debug, Arbitrary)]
#[proptest(with = "Foo::ctor(1337, :usize:.other_fn(:f64:, #0..7#))")]
struct Foo {
    //..
}
*/

#[derive(Default)]
struct Complex;

#[derive(Debug, Arbitrary)]
#[proptest(params(Complex))]
enum Foo {
    #[proptest(value = "Foo::F0(1, 1)")]
    F0(usize, u8),
}

impl Foo {
    fn payload(&self) -> (usize, u8) {
        match self {
            Self::F0(left, right) => (*left, *right),
        }
    }
}

#[derive(Clone, Debug, Arbitrary)]
#[proptest(params = "usize")]
enum A {
    B,
    #[proptest(strategy = "Just(A::C(1))")]
    C(usize),
}

impl A {
    fn payload(&self) -> Option<usize> {
        match self {
            Self::B => None,
            Self::C(value) => Some(*value),
        }
    }
}

#[derive(Clone, Debug, Arbitrary)]
enum Bobby {
    #[proptest(no_params)]
    B(usize),
    #[proptest(no_params, value = "Bobby::C(1)")]
    C(usize),
    #[proptest(no_params, strategy = "Just(Bobby::D(1))")]
    D(usize),
    #[proptest(params(Complex), value = "Bobby::E(1)")]
    E(usize),
    #[proptest(params(Complex), strategy = "Just(Bobby::F(1))")]
    F(usize),
}

impl Bobby {
    fn payload(&self) -> usize {
        match self {
            Self::B(value)
            | Self::C(value)
            | Self::D(value)
            | Self::E(value)
            | Self::F(value) => *value,
        }
    }
}

#[derive(Clone, Debug, Arbitrary)]
enum Quux {
    B(#[proptest(no_params)] usize),
    C(usize, String),
    #[proptest(value = "Quux::D(2, \"a\".into())")]
    D(usize, String),
    #[proptest(strategy = "Just(Quux::E(1337))")]
    E(u32),
    F {
        #[proptest(strategy = "10usize..20usize")]
        _foo: usize,
    },
}

impl Quux {
    fn payload_score(&self) -> usize {
        match self {
            Self::B(value) => *value,
            Self::C(value, text) | Self::D(value, text) => *value + text.len(),
            Self::E(value) => {
                if *value == 1337 {
                    1337
                } else {
                    0
                }
            }
            Self::F { _foo: value } => *value,
        }
    }
}

#[test]
fn foo_value_constructor_sets_payload() -> TestResult {
    ensure_property(
        &any::<Foo>(),
        "a variant value constructor pins the payload",
        |value| {
            let (left, right) = value.payload();
            ensure_eq(&left, &1, "the left payload is pinned")?;
            ensure_eq(&right, &1, "the right payload is pinned")
        },
    )
}

#[test]
fn a_custom_strategy_sets_c_payload() -> TestResult {
    ensure_property(
        &any_with::<A>(0usize),
        "a variant strategy pins the C payload",
        |value| {
            if let Some(payload) = value.payload() {
                ensure_eq(&payload, &1, "the strategy-built payload is one")?;
            }
            Ok(())
        },
    )
}

#[test]
fn bobby_attributes_keep_payloads_reachable() -> TestResult {
    ensure_property(
        &any::<Bobby>(),
        "per-variant params spellings keep payloads reachable",
        |value| match &value {
            Bobby::B(_) => {
                let _ = value.payload();
                Ok(())
            }
            Bobby::C(_) | Bobby::D(_) | Bobby::E(_) | Bobby::F(_) => {
                ensure_eq(&value.payload(), &1, "the pinned payload is one")
            }
        },
    )
}

#[test]
fn quux_attributes_keep_payloads_reachable() -> TestResult {
    ensure_property(
        &any::<Quux>(),
        "mixed variant attributes keep payload scores reachable",
        |value| match &value {
            Quux::B(_) | Quux::C(_, _) => {
                let _ = value.payload_score();
                Ok(())
            }
            Quux::D(_, _) => ensure_eq(
                &value.payload_score(),
                &3,
                "the value variant scores three",
            ),
            Quux::E(_) => ensure_eq(
                &value.payload_score(),
                &1337,
                "the strategy variant scores 1337",
            ),
            Quux::F { _foo } => ensure(
                (10..20).contains(_foo),
                "the range strategy stays in bounds",
            ),
        },
    )
}

#[test]
fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<Foo>();
    assert_arbitrary::<A>();
    assert_arbitrary::<Bobby>();
    assert_arbitrary::<Quux>();
}
