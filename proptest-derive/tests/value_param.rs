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
  use proptest::prelude::*;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  /// Each assertion retains the complete generated value.
  type Checked<T> = PropertyResult<T, T, PredicateFailure<T>>;

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

  #[test]
  fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<T0>();
    assert_arbitrary::<T1>();
    assert_arbitrary::<T2>();
    assert_arbitrary::<T3>();
    assert_arbitrary::<T4>();
    assert_arbitrary::<T5>();
  }

  #[test]
  fn t0_test() -> Checked<T0> {
    ensure_property(&any_with::<T0>(4), "the value expression halves the param", |generated| {
      ensure_that(generated, "the value expression halves the param", |sample| {
        matches!(sample, T0::V0(2))
      })
    })
  }

  #[test]
  fn t1_test() -> Checked<T1> {
    ensure_property(&any_with::<T1>(4), "the value expression doubles the param", |generated| {
      ensure_that(generated, "the value expression doubles the param", |sample| {
        matches!(sample, T1::V0 {
          field: 8
        })
      })
    })
  }

  #[test]
  fn t2_test_true() -> Checked<T2> {
    ensure_property(&any_with::<T2>(4), "the power-of-two check holds for four", |generated| {
      ensure_that(generated, "the power-of-two check holds for four", |sample| {
        matches!(sample, T2::V0(true))
      })
    })
  }

  #[test]
  fn t2_test_false() -> Checked<T2> {
    ensure_property(&any_with::<T2>(10), "the power-of-two check fails for ten", |generated| {
      ensure_that(generated, "the power-of-two check fails for ten", |sample| {
        matches!(sample, T2::V0(false))
      })
    })
  }

  #[test]
  fn t3_test() -> Checked<T3> {
    ensure_property(&any_with::<T3>(4), "the value expression squares the param", |generated| {
      ensure_that(generated, "the value expression squares the param", |sample| {
        matches!(sample, T3::V0 {
          field: 16
        })
      })
    })
  }

  #[test]
  fn t4_test() -> Checked<T4> {
    ensure_property(&any_with::<T4>(4), "the value expression subtracts three", |generated| {
      ensure_that(generated, "the value expression subtracts three", |sample| sample.field == 1)
    })
  }

  #[test]
  fn t5_test() -> Checked<T5> {
    ensure_property(&any_with::<T5>(4), "the fn-call value adds one", |generated| {
      ensure_that(generated, "the fn-call value adds one", |sample| sample.0 == 5)
    })
  }
}
