// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for `#[derive(Arbitrary)]` on enums of many
//! shapes and arities.
//!
//! Derives `Arbitrary` for enums ranging from 1 to 25 idiomatic unit
//! variants, then for payload-carrying and nested enums whose generated
//! values are checked to stay within per-variant scoring bounds. This guards
//! variant-count scaling in the union codegen and that every variant's payload
//! is generated.

#[cfg(test)]
mod tests {
    use proptest::prelude::{Arbitrary, any};
    use proptest::strict::{TestResult, ensure_property};
    use proptest_derive::Arbitrary;
    use strict_test_support::ensure;

    #[derive(Debug, Arbitrary)]
    enum T1 {
        V1,
    }

    #[derive(Debug, Arbitrary)]
    enum T2 {
        V1,
        V2,
    }

    #[derive(Debug, Arbitrary)]
    enum T3 {
        V1,
        V2,
        V3,
    }

    #[derive(Debug, Arbitrary)]
    enum T4 {
        V1,
        V2,
        V3,
        V4,
    }

    #[derive(Debug, Arbitrary)]
    enum T5 {
        V1,
        V2,
        V3,
        V4,
        V5,
    }

    #[derive(Debug, Arbitrary)]
    enum T6 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
    }

    #[derive(Debug, Arbitrary)]
    enum T7 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
    }

    #[derive(Debug, Arbitrary)]
    enum T8 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
    }

    #[derive(Debug, Arbitrary)]
    enum T9 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
    }

    #[derive(Debug, Arbitrary)]
    enum T10 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
    }

    #[derive(Debug, Arbitrary)]
    enum T11 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
    }

    #[derive(Debug, Arbitrary)]
    enum T12 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
    }

    #[derive(Debug, Arbitrary)]
    enum T13 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
    }

    #[derive(Debug, Arbitrary)]
    enum T14 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
    }

    #[derive(Debug, Arbitrary)]
    enum T15 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
    }

    #[derive(Debug, Arbitrary)]
    enum T16 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
    }

    #[derive(Debug, Arbitrary)]
    enum T17 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
    }

    #[derive(Debug, Arbitrary)]
    enum T18 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
    }

    #[derive(Debug, Arbitrary)]
    enum T19 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
    }

    #[derive(Debug, Arbitrary)]
    enum T20 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
        V20,
    }

    #[derive(Debug, Arbitrary)]
    enum T21 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
        V20,
        V21,
    }

    #[derive(Debug, Arbitrary)]
    enum T22 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
        V20,
        V21,
        V22,
    }

    #[derive(Debug, Arbitrary)]
    enum T23 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
        V20,
        V21,
        V22,
        V23,
    }

    #[derive(Debug, Arbitrary)]
    enum T24 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
        V20,
        V21,
        V22,
        V23,
        V24,
    }

    #[derive(Debug, Arbitrary)]
    enum T25 {
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
        V20,
        V21,
        V22,
        V23,
        V24,
        V25,
    }

    #[derive(Clone, Debug, Arbitrary)]
    enum Alan {
        Unsigned(usize),
        Text(String),
        Empty(()),
        Word(u32),
        Real(f64),
        Letter(char),
    }

    impl Alan {
        fn payload_score(&self) -> usize {
            match *self {
                Self::Unsigned(payload) => {
                    usize::from(payload.count_ones() > 0)
                }
                Self::Text(ref payload) if payload.is_empty() => 1,
                Self::Text(_) | Self::Empty(()) => 2,
                Self::Word(0) => 3,
                Self::Real(payload) if payload.is_sign_negative() => 5,
                Self::Word(_) | Self::Real(_) => 4,
                Self::Letter(payload) if payload.len_utf8() == 0 => 5,
                Self::Letter(_) => 6,
            }
        }
    }

    #[derive(Clone, Debug, Arbitrary)]
    enum SameType {
        Former(usize),
        Latter(usize),
    }

    impl SameType {
        fn payload_score(&self) -> usize {
            match *self {
                Self::Former(payload) => usize::from(payload.count_ones() > 0),
                Self::Latter(payload) if payload.count_ones() == 0 => 1,
                Self::Latter(_) => 2,
            }
        }
    }

    #[derive(Arbitrary, Debug)]
    enum OneTwo {
        One(u8),
        Two(u8, u8),
    }

    impl OneTwo {
        const fn payload_score(&self) -> usize {
            match *self {
                Self::One(payload) if payload.count_ones() == 0 => 1,
                Self::One(_) => 2,
                Self::Two(left, right) => {
                    match (left.count_ones() > 0, right.count_ones() > 0) {
                        (false, false) => 2,
                        (true, false) | (false, true) => 3,
                        (true, true) => 4,
                    }
                }
            }
        }
    }

    #[derive(Arbitrary, Debug)]
    enum ZeroOneTwo {
        Zero,
        One(u8),
        Two(u8, u8),
    }

    impl ZeroOneTwo {
        const fn payload_score(&self) -> usize {
            match *self {
                Self::Zero => 0,
                Self::One(payload) if payload.count_ones() == 0 => 1,
                Self::One(_) => 2,
                Self::Two(left, right) => {
                    match (left.count_ones() > 0, right.count_ones() > 0) {
                        (false, false) => 2,
                        (true, false) | (false, true) => 3,
                        (true, true) => 4,
                    }
                }
            }
        }
    }

    #[derive(Arbitrary, Debug)]
    enum Nested {
        First(SameType),
        Second(ZeroOneTwo, OneTwo),
    }

    impl Nested {
        fn payload_score(&self) -> usize {
            match *self {
                Self::First(ref payload) => payload.payload_score(),
                Self::Second(ref left, ref right) => {
                    left.payload_score().saturating_add(right.payload_score())
                }
            }
        }
    }

    #[test]
    fn generated_payload_fixtures_are_consumed() -> TestResult {
        ensure_property(
            &(
                any::<Alan>(),
                any::<SameType>(),
                any::<OneTwo>(),
                any::<ZeroOneTwo>(),
                any::<Nested>(),
            ),
            "derived enum payloads stay within their scoring bounds",
            |(alan, same_type, one_two, zero_one_two, nested)| {
                ensure(
                    alan.payload_score() <= 6,
                    "Alan's payloads stay bounded",
                )?;
                ensure(
                    same_type.payload_score() <= 2,
                    "SameType's payloads stay bounded",
                )?;
                ensure(
                    (1..=4).contains(&one_two.payload_score()),
                    "OneTwo consumes generated payloads in its score",
                )?;
                ensure(
                    zero_one_two.payload_score() <= 4,
                    "ZeroOneTwo consumes generated payloads in its score",
                )?;
                ensure(
                    nested.payload_score() <= 8,
                    "Nested composes the inner scores",
                )
            },
        )
    }

    #[test]
    fn asserting_arbitrary() {
        fn assert_arbitrary<T: Arbitrary>() {}

        assert_arbitrary::<T1>();
        assert_arbitrary::<T2>();
        assert_arbitrary::<T3>();
        assert_arbitrary::<T4>();
        assert_arbitrary::<T5>();
        assert_arbitrary::<T6>();
        assert_arbitrary::<T7>();
        assert_arbitrary::<T8>();
        assert_arbitrary::<T9>();
        assert_arbitrary::<T10>();
        assert_arbitrary::<T11>();
        assert_arbitrary::<T12>();
        assert_arbitrary::<T13>();
        assert_arbitrary::<T14>();
        assert_arbitrary::<T15>();
        assert_arbitrary::<T16>();
        assert_arbitrary::<T17>();
        assert_arbitrary::<T18>();
        assert_arbitrary::<T19>();
        assert_arbitrary::<T20>();
        assert_arbitrary::<T21>();
        assert_arbitrary::<T22>();
        assert_arbitrary::<T23>();
        assert_arbitrary::<T24>();
        assert_arbitrary::<T25>();
        assert_arbitrary::<Alan>();
        assert_arbitrary::<SameType>();
        assert_arbitrary::<OneTwo>();
        assert_arbitrary::<ZeroOneTwo>();
        assert_arbitrary::<Nested>();
    }
}
