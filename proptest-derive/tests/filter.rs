// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Coverage for the `#[proptest(filter(...))]` modifier at container, variant,
//! and field level across every spelling.
//!
//! The derived types apply filters written as closure strings, `fn` paths, and
//! `filter = "..."` forms, stack multiple filters on one field, and combine
//! filtering with `strategy`, `value`, and `params`; each property confirms
//! the generated value satisfies every filter that applies to it.

#[cfg(test)]
mod tests {
  use proptest::prelude::*;
  use proptest::strict::TestResult;
  use proptest::strict::ensure_property;
  use proptest_derive::Arbitrary;
  use strict_test_support::ensure;

  trait FilterArithmetic {
    fn filter_is_even(&self) -> bool;

    fn filter_is_rem3(&self) -> bool;
  }

  impl FilterArithmetic for usize {
    fn filter_is_even(&self) -> bool {
      self.is_multiple_of(2)
    }

    fn filter_is_rem3(&self) -> bool {
      self.is_multiple_of(3)
    }
  }

  fn even<T: FilterArithmetic + ?Sized>(x: &T) -> bool {
    x.filter_is_even()
  }

  fn rem3<T: FilterArithmetic + ?Sized>(x: &T) -> bool {
    x.filter_is_rem3()
  }

  #[derive(Copy, Clone)]
  struct Param(usize);

  impl Default for Param {
    fn default() -> Self {
      Self(100)
    }
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(filter("|x| x.foo.rem_euclid(3) == 0"))]
  struct T0 {
    #[proptest(no_params, filter(even))]
    foo:    usize,
    #[proptest(filter("|x| x.rem_euclid(2) == 1"))]
    bar:    usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x.rem_euclid(2) == 1")]
    baz:    usize,
    #[proptest(value = "42", filter(even))]
    quux:   usize,
    #[proptest(params(Param), strategy("0..=params.0"), filter("|x| *x > 2"))]
    wibble: usize,
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(params(Param))]
  #[proptest(filter("|x| x.foo.rem_euclid(3) == 0"))]
  struct T1 {
    #[proptest(filter(even))]
    foo:    usize,
    #[proptest(filter("|x| x.rem_euclid(2) == 1"))]
    bar:    usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x.rem_euclid(2) == 1")]
    baz:    usize,
    #[proptest(value = "42", filter(even))]
    quux:   usize,
    #[proptest(strategy("0..=params.0"), filter("|x| *x > 2"))]
    wibble: usize,
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(filter("|x| x.0.rem_euclid(3) == 0"))]
  struct T2(
    #[proptest(no_params, filter(even))] usize,
    #[proptest(filter("|x| x.rem_euclid(2) == 1"))] usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x.rem_euclid(2) == 1")] usize,
    #[proptest(value = "42", filter(even))] usize,
    #[proptest(params(Param), strategy("0..=params.0"), filter("|x| *x > 2"))] usize,
  );

  #[derive(Debug, Arbitrary)]
  #[proptest(filter("|x| x.0.rem_euclid(3) == 0"))]
  struct T3(
    #[proptest(no_params, filter(even))] usize,
    #[proptest(filter("|x| x.rem_euclid(2) == 1"))] usize,
    #[proptest(strategy = "0..100usize", filter = "|x| x.rem_euclid(2) == 1")] usize,
    #[proptest(value = "42", filter(even))] usize,
    #[proptest(params(Param), strategy("0..=params.0"), filter("|x| *x > 2"))] usize,
  );

  #[allow(
    clippy::single_call_fn,
    reason = "test predicate keeping only the T4::V0 container variant"
  )]
  const fn is_v0(candidate: &T4) -> bool {
    matches!(candidate, T4::V0 { .. })
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(filter(is_v0))]
  enum T4 {
    V0 {
      #[proptest(filter(even))]
      field: usize,
    },
    V1,
  }

  #[allow(
    clippy::single_call_fn,
    reason = "T5::V0 filter predicate accepting fields divisible by three"
  )]
  fn t5_v0_rem_3(candidate: &T5) -> bool {
    if let T5::V0 {
      field,
    } = *candidate
    {
      rem3(&field)
    } else {
      false
    }
  }

  #[allow(
    clippy::single_call_fn,
    reason = "T5::V1 filter predicate accepting fields divisible by five"
  )]
  const fn t5_v1_rem_5(candidate: &T5) -> bool {
    if let T5::V1(field) = *candidate {
      field.is_multiple_of(5)
    } else {
      false
    }
  }

  #[derive(Debug, Arbitrary)]
  enum T5 {
    #[proptest(filter(t5_v0_rem_3))]
    V0 {
      #[proptest(filter(even))]
      field: usize,
    },
    #[proptest(strategy("(0..1000usize).prop_map(T5::V1)"), filter(t5_v1_rem_5))]
    V1(usize),
  }

  #[allow(
    clippy::single_call_fn,
    reason = "T6::V0 filter predicate accepting fields divisible by three"
  )]
  fn t6_v0_rem_3(candidate: &T6) -> bool {
    if let T6::V0 {
      field,
    } = *candidate
    {
      rem3(&field)
    } else {
      false
    }
  }

  #[allow(
    clippy::single_call_fn,
    reason = "T6::V1 filter predicate accepting fields divisible by five"
  )]
  const fn t6_v1_rem_5(candidate: &T6) -> bool {
    if let T6::V1(field) = *candidate {
      field.is_multiple_of(5)
    } else {
      false
    }
  }

  #[derive(Debug, Arbitrary)]
  #[proptest(params(Param))]
  enum T6 {
    #[proptest(filter(t6_v0_rem_3))]
    V0 {
      #[proptest(filter(even))]
      field: usize,
    },
    #[proptest(strategy("(0..params.0).prop_map(T6::V1)"), filter(t6_v1_rem_5))]
    V1(usize),
  }

  #[derive(Debug, Arbitrary)]
  struct T7 {
    #[proptest(filter(even), filter(rem3))]
    foo: usize,
  }

  #[test]
  fn t0_test() -> TestResult {
    ensure_property(&any::<T0>(), "every filter spelling holds on a named struct", |sample| {
      ensure(even(&sample.foo) && rem3(&sample.foo), "field and container filters compose")?;
      ensure(!even(&sample.bar), "the closure-string filter holds")?;
      ensure(!even(&sample.baz) && sample.baz < 100, "the filter composes with a strategy")?;
      ensure(even(&sample.quux) && sample.quux == 42, "the filter composes with a value")?;
      ensure(
        sample.wibble > 2 && sample.wibble <= 100,
        "the filter composes with params and a strategy",
      )
    })
  }

  #[test]
  fn t1_test() -> TestResult {
    ensure_property(&any::<T1>(), "every filter spelling holds under container params", |sample| {
      ensure(
        even(&sample.foo) && sample.foo.rem_euclid(3) == 0,
        "field and container filters compose",
      )?;
      ensure(!even(&sample.bar), "the closure-string filter holds")?;
      ensure(!even(&sample.baz) && sample.baz < 100, "the filter composes with a strategy")?;
      ensure(even(&sample.quux) && sample.quux == 42, "the filter composes with a value")?;
      ensure(
        sample.wibble > 2 && sample.wibble <= 100,
        "the filter composes with the params strategy",
      )
    })
  }

  #[test]
  fn t2_test() -> TestResult {
    ensure_property(&any::<T2>(), "every filter spelling holds on a tuple struct", |sample| {
      ensure(
        even(&sample.0) && sample.0.rem_euclid(3) == 0,
        "field and container filters compose",
      )?;
      ensure(!even(&sample.1), "the closure-string filter holds")?;
      ensure(!even(&sample.2) && sample.2 < 100, "the filter composes with a strategy")?;
      ensure(even(&sample.3) && sample.3 == 42, "the filter composes with a value")?;
      ensure(sample.4 > 2 && sample.4 <= 100, "the filter composes with params and a strategy")
    })
  }

  #[test]
  fn t3_test() -> TestResult {
    ensure_property(&any::<T3>(), "the duplicate tuple-struct spelling holds", |sample| {
      ensure(
        even(&sample.0) && sample.0.rem_euclid(3) == 0,
        "field and container filters compose",
      )?;
      ensure(!even(&sample.1), "the closure-string filter holds")?;
      ensure(!even(&sample.2) && sample.2 < 100, "the filter composes with a strategy")?;
      ensure(even(&sample.3) && sample.3 == 42, "the filter composes with a value")?;
      ensure(sample.4 > 2 && sample.4 <= 100, "the filter composes with params and a strategy")
    })
  }

  #[test]
  fn t4_test() -> TestResult {
    ensure_property(&any::<T4>(), "a container fn-filter keeps only the matching variant", |sample| {
      ensure(
        if let T4::V0 {
          field,
        } = sample
        {
          even(&field)
        } else {
          false
        },
        "only V0 with an even field survives the filters",
      )
    })
  }

  #[test]
  fn t5_test() -> TestResult {
    ensure_property(&any::<T5>(), "variant-level filters hold per variant", |sample| match sample {
      T5::V0 {
        field,
      } => ensure(rem3(&field) && even(&field), "V0 satisfies the variant and field filters"),
      T5::V1(field) => ensure(field < 1000 && field.rem_euclid(5) == 0, "V1 satisfies the strategy filter"),
    })
  }

  #[test]
  fn t6_test() -> TestResult {
    ensure_property(
      &any::<T6>(),
      "variant-level filters hold under container params",
      |sample| match sample {
        T6::V0 {
          field,
        } => ensure(rem3(&field) && even(&field), "V0 satisfies the variant and field filters"),
        T6::V1(field) => ensure(field < 100 && field.rem_euclid(5) == 0, "V1 satisfies the params strategy filter"),
      },
    )
  }

  #[test]
  fn t7_test() -> TestResult {
    ensure_property(&any::<T7>(), "repeated field filters accumulate", |sample| {
      ensure(even(&sample.foo) && rem3(&sample.foo), "both accumulated filters hold")
    })
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
    assert_arbitrary::<T7>();
  }
}
