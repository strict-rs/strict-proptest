// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for `#[proptest(value = ...)]` combined with
//! `#[proptest(params = ...)]`.
//!
//! Derives `Arbitrary` for structs and enum variants whose `value`
//! expression reads the supplied `params`, then drives each type with
//! `any_with` to confirm the generated field equals the value computed
//! from the parameter (halving, doubling, squaring, subtraction, and a
//! `fn` call).

#[cfg(test)]
mod tests {
  mod cases;
  mod properties;
  mod support;

  use cases::derived_properties;
  use proptest::prelude::*;
  use proptest_derive::Arbitrary;
  use support::assert_arbitrary;

  #[derive(Debug, Arbitrary)]
  enum T0 {
    #[proptest(params = "u8", value = "T0::V0(params.div_euclid(2))")]
    V0(u8),
  }

  #[derive(Debug, Arbitrary)]
  enum T1 {
    #[proptest(params = "u8", value = "T1::V0 { field: params.saturating_mul(2) }")]
    V0 { field: u8 },
  }

  #[derive(Debug, Arbitrary)]
  enum T2 {
    V0(#[proptest(params = "u8", value = "params.is_power_of_two()")] bool),
  }

  #[derive(Debug, Arbitrary)]
  enum T3 {
    V0 {
      #[proptest(params = "u8", value = "params.saturating_mul(params)")]
      field: u8,
    },
  }

  #[derive(Debug, Arbitrary)]
  struct T4 {
    #[proptest(params = "u8", value = "params.saturating_sub(3)")]
    field: u8,
  }

  #[allow(
    clippy::single_call_fn,
    reason = "fn-path value expression that adds one to the params argument for T5"
  )]
  const fn add(x: u8) -> u8 {
    x.saturating_add(1)
  }

  #[derive(Debug, Arbitrary)]
  struct T5(#[proptest(params = "u8", value = "add(params)")] u8);

  assert_arbitrary!(T0, T1, T2, T3, T4, T5,);

  derived_properties! {
    t0_test(
      T0, any_with::<T0>(4),
      "the value expression halves the param",
      "the value expression halves the param",
      |sample| matches!(sample, T0::V0(2)),
    );
    t1_test(
      T1, any_with::<T1>(4),
      "the value expression doubles the param",
      "the value expression doubles the param",
      |sample| {
        matches!(sample, T1::V0 {
          field: 8
        })
      },
    );
    t2_test_true(
      T2, any_with::<T2>(4),
      "the power-of-two check holds for four",
      "the power-of-two check holds for four",
      |sample| matches!(sample, T2::V0(true)),
    );
    t2_test_false(
      T2, any_with::<T2>(10),
      "the power-of-two check fails for ten",
      "the power-of-two check fails for ten",
      |sample| matches!(sample, T2::V0(false)),
    );
    t3_test(
      T3, any_with::<T3>(4),
      "the value expression squares the param",
      "the value expression squares the param",
      |sample| {
        matches!(sample, T3::V0 {
          field: 16
        })
      },
    );
    t4_test(
      T4, any_with::<T4>(4),
      "the value expression subtracts three",
      "the value expression subtracts three",
      |sample| sample.field == 1,
    );
    t5_test(
      T5, any_with::<T5>(4),
      "the fn-call value adds one",
      "the fn-call value adds one",
      |sample| sample.0 == 5,
    );
  }
}
