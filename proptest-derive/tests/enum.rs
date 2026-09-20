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
  mod support;

  use proptest::prelude::any;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;
  use support::assert_arbitrary;

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
        Self::Unsigned(payload) => usize::from(payload.count_ones() > 0),
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

  /// Count each byte payload once, with one additional point for a nonzero byte.
  fn byte_payload_score(payloads: &[u8]) -> usize {
    payloads.iter().fold(0_usize, |score, payload| {
      score.saturating_add(1).saturating_add(usize::from(*payload != 0))
    })
  }

  impl OneTwo {
    fn payload_score(&self) -> usize {
      match *self {
        Self::One(payload) => byte_payload_score(&[payload]),
        Self::Two(left, right) => byte_payload_score(&[left, right]),
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
    fn payload_score(&self) -> usize {
      match *self {
        Self::Zero => 0,
        Self::One(payload) => byte_payload_score(&[payload]),
        Self::Two(left, right) => byte_payload_score(&[left, right]),
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
        Self::Second(ref left, ref right) => left.payload_score().saturating_add(right.payload_score()),
      }
    }
  }

  /// Every generated enum is retained alongside the complete property verdict.
  type Payloads = (Alan, SameType, OneTwo, ZeroOneTwo, Nested);

  #[test]
  fn generated_payload_fixtures_are_consumed() -> PropertyResult<Payloads, Payloads, PredicateFailure<Payloads>> {
    ensure_property(
      &(
        any::<Alan>(),
        any::<SameType>(),
        any::<OneTwo>(),
        any::<ZeroOneTwo>(),
        any::<Nested>(),
      ),
      "derived enum payloads stay within their scoring bounds",
      |values| {
        ensure_that(values, "all generated enum payload scores stay bounded", |observed| {
          observed.0.payload_score() <= 6
            && observed.1.payload_score() <= 2
            && (1..=4).contains(&observed.2.payload_score())
            && observed.3.payload_score() <= 4
            && observed.4.payload_score() <= 8
        })
      },
    )
  }

  assert_arbitrary!(
    T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, Alan, SameType,
    OneTwo, ZeroOneTwo, Nested,
  );
}
