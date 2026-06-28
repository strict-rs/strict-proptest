// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use proptest::prelude::{
    Arbitrary, any_with, prop_assert, prop_assert_eq, proptest,
};
use proptest::strategy::Just;
use proptest_derive::Arbitrary;

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

proptest! {
    #[test]
    fn foo_value_constructor_sets_payload(value: Foo) {
        prop_assert_eq!(value.payload(), (1, 1));
    }

    #[test]
    fn a_custom_strategy_sets_c_payload(value in any_with::<A>(0usize)) {
        if let Some(payload) = value.payload() {
            prop_assert_eq!(payload, 1);
        }
    }

    #[test]
    fn bobby_attributes_keep_payloads_reachable(value: Bobby) {
        match &value {
            Bobby::B(_) => {
                let _ = value.payload();
            }
            Bobby::C(_) | Bobby::D(_) | Bobby::E(_) | Bobby::F(_) => {
                prop_assert_eq!(value.payload(), 1);
            }
        }
    }

    #[test]
    fn quux_attributes_keep_payloads_reachable(value: Quux) {
        match &value {
            Quux::B(_) | Quux::C(_, _) => {
                let _ = value.payload_score();
            }
            Quux::D(_, _) => {
                prop_assert_eq!(value.payload_score(), 3);
            }
            Quux::E(_) => {
                prop_assert_eq!(value.payload_score(), 1337);
            }
            Quux::F { _foo } => {
                prop_assert!((10..20).contains(_foo));
            }
        }
    }
}

#[test]
fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<Foo>();
    assert_arbitrary::<A>();
    assert_arbitrary::<Bobby>();
    assert_arbitrary::<Quux>();
}
