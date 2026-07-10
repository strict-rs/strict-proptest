// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Grab-bag coverage for `#[proptest(...)]` combinations not exercised by the
//! per-modifier test files.
//!
//! The derived enums mix container-level `params` with variant-level
//! `value`, `strategy`, `no_params`, and `params` attributes on each variant,
//! and each property confirms the payloads stay reachable and pinned.

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
enum Custom {
    Unit,
    #[proptest(strategy = "Just(Custom::Fixed(1))")]
    Fixed(usize),
}

impl Custom {
    fn payload(&self) -> Option<usize> {
        match self {
            Self::Unit => None,
            Self::Fixed(payload) => Some(*payload),
        }
    }
}

#[derive(Clone, Debug, Arbitrary)]
enum Bobby {
    #[proptest(no_params)]
    Defaulted(usize),
    #[proptest(no_params, value = "Bobby::Valued(1)")]
    Valued(usize),
    #[proptest(no_params, strategy = "Just(Bobby::Strategized(1))")]
    Strategized(usize),
    #[proptest(params(Complex), value = "Bobby::ParamValued(1)")]
    ParamValued(usize),
    #[proptest(params(Complex), strategy = "Just(Bobby::ParamStrategized(1))")]
    ParamStrategized(usize),
}

impl Bobby {
    fn payload(&self) -> usize {
        match self {
            Self::Defaulted(payload)
            | Self::Valued(payload)
            | Self::Strategized(payload)
            | Self::ParamValued(payload)
            | Self::ParamStrategized(payload) => *payload,
        }
    }
}

#[derive(Clone, Debug, Arbitrary)]
enum Quux {
    Bare(#[proptest(no_params)] usize),
    Pair(usize, String),
    #[proptest(value = "Quux::PinnedPair(2, \"a\".into())")]
    PinnedPair(usize, String),
    #[proptest(strategy = "Just(Quux::PinnedWord(1337))")]
    PinnedWord(u32),
    Braced {
        #[proptest(strategy = "10usize..20usize")]
        _foo: usize,
    },
}

impl Quux {
    fn payload_score(&self) -> usize {
        match self {
            Self::Bare(payload) => *payload,
            Self::Pair(payload, text) | Self::PinnedPair(payload, text) => {
                *payload + text.len()
            }
            Self::PinnedWord(payload) => {
                if *payload == 1337 {
                    1337
                } else {
                    0
                }
            }
            Self::Braced { _foo: payload } => *payload,
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
        &any_with::<Custom>(0usize),
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
            Bobby::Defaulted(_) => {
                let _ = value.payload();
                Ok(())
            }
            Bobby::Valued(_)
            | Bobby::Strategized(_)
            | Bobby::ParamValued(_)
            | Bobby::ParamStrategized(_) => {
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
            Quux::Bare(_) | Quux::Pair(_, _) => {
                let _ = value.payload_score();
                Ok(())
            }
            Quux::PinnedPair(_, _) => ensure_eq(
                &value.payload_score(),
                &3,
                "the value variant scores three",
            ),
            Quux::PinnedWord(_) => ensure_eq(
                &value.payload_score(),
                &1337,
                "the strategy variant scores 1337",
            ),
            Quux::Braced { _foo } => ensure(
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
    assert_arbitrary::<Custom>();
    assert_arbitrary::<Bobby>();
    assert_arbitrary::<Quux>();
}
