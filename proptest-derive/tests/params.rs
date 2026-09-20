// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Coverage for the `#[proptest(params(...))]` and `#[proptest(no_params)]`
//! modifiers that thread a custom parameter type into a derived strategy.
//!
//! The derived types exercise container-level and field-level params (in both
//! the `params(T)` and `params = "T"` spellings), `no_params` overrides, field
//! strategies that read `params`, and per-field "parallel" params; each test
//! drives generation through `any_with` to supply the parameters.

#[cfg(test)]
mod tests {
  use core::convert::Infallible;

  use proptest::prelude::Arbitrary;
  use proptest::prelude::any_with;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  /// Each assertion retains the complete generated value.
  type Checked<T> = PropertyResult<T, T, PredicateFailure<T>>;

  struct ComplexType {
    max: u64,
  }

  impl Default for ComplexType {
    fn default() -> Self {
      Self {
        max: 10
      }
    }
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(params(ComplexType))]
  struct TopHasParams {
    _string: usize,
    #[proptest(strategy = "0..params.max")]
    int:     u64,
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(no_params)]
  struct TopNoParams {
    _stuff: usize,
  }

  #[derive(Debug, Arbitrary)]
  struct InnerNoParams {
    string: String,
    #[proptest(no_params)]
    has:    TopHasParams,
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(params(u64))]
  struct Tpis {
    #[proptest(strategy = "\"a+\"")]
    string: String,
    #[proptest(strategy = "3..=params")]
    int:    u64,
  }

  #[derive(Debug, Arbitrary)]
  struct Parallel {
    #[proptest(params = "&'static str", strategy = "params")]
    string: String,
    #[proptest(params(u8), strategy = "0i64..i64::from(params)")]
    int:    i64,
  }

  #[derive(Debug, Arbitrary)]
  struct Parallel2 {
    #[proptest(params("&'static str"), strategy = "params")]
    string: String,
    #[proptest(params("u8"), strategy = "0i64..i64::from(params)")]
    int:    i64,
  }

  const MAX: ComplexType = ComplexType {
    max: 5
  };

  #[test]
  fn top_no_params() -> PropertyResult<TopNoParams, TopNoParams, Infallible> {
    ensure_property(
      &any_with::<TopNoParams>(()),
      "a no_params container generates under unit params",
      Ok,
    )
  }

  #[test]
  fn top_has_params() -> Checked<TopHasParams> {
    ensure_property(
      &any_with::<TopHasParams>(MAX),
      "container params thread into the field strategy",
      |generated| {
        ensure_that(generated, "container params thread into the field strategy", |sample| {
          sample.int < 5
        })
      },
    )
  }

  #[test]
  fn inner_params() -> Checked<InnerNoParams> {
    ensure_property(
      &any_with::<InnerNoParams>("\\s+".into()),
      "an inner no_params field keeps its defaults and the outer string uses its regex",
      |generated| {
        ensure_that(
          generated,
          "an inner no_params field keeps its defaults and the outer string uses its regex",
          |sample| sample.has.int < 10 && sample.string.trim().is_empty(),
        )
      },
    )
  }

  #[test]
  fn top_param_inner_strat() -> Checked<Tpis> {
    ensure_property(
      &any_with::<Tpis>(6),
      "container params reach the range field and the string keeps its own strategy",
      |generated| {
        ensure_that(
          generated,
          "container params reach the range field and the string keeps its own strategy",
          |sample| (3..=6).contains(&sample.int) && sample.string.chars().all(|character| character == 'a'),
        )
      },
    )
  }

  #[test]
  fn parallel_params() -> Checked<Parallel> {
    check_parallel_params("parallel per-field params drive each field", |sample: &Parallel| {
      (&sample.string, sample.int)
    })
  }

  #[test]
  fn parallel_params2() -> Checked<Parallel2> {
    check_parallel_params("string-spelled parallel params drive each field", |sample: &Parallel2| {
      (&sample.string, sample.int)
    })
  }

  /// Both parameter spellings preserve the generated subject while checking its fields.
  fn check_parallel_params<T>(context: &'static str, fields: impl Fn(&T) -> (&str, i64)) -> Checked<T>
  where
    T: Arbitrary<Parameters = (&'static str, u8)>,
  {
    ensure_property(&any_with::<T>(("[0-9]", 3)), context, |generated| {
      ensure_that(generated, context, |sample| {
        let (string, int) = fields(sample);
        (0..3).contains(&int) && string.chars().next().is_some_and(|character| character.is_ascii_digit())
      })
    })
  }

  #[test]
  fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<TopHasParams>();
    assert_arbitrary::<TopNoParams>();
    assert_arbitrary::<InnerNoParams>();
    assert_arbitrary::<Tpis>();
    assert_arbitrary::<Parallel>();
    assert_arbitrary::<Parallel2>();
  }
}
