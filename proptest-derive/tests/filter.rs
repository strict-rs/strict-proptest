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
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  /// Every filter is checked against the complete generated value.
  type Checked<T> = PropertyResult<T, T, PredicateFailure<T>>;

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

  /// Shared field, container, strategy, value, and parameter expectations.
  fn satisfies_composed_filters(foo: usize, bar: usize, baz: usize, quux: usize, wibble: usize) -> bool {
    even(&foo) && rem3(&foo) && !even(&bar) && !even(&baz) && baz < 100 && even(&quux) && quux == 42 && wibble > 2 && wibble <= 100
  }

  #[test]
  fn t0_test() -> Checked<T0> {
    ensure_property(&any::<T0>(), "every filter spelling holds on a named struct", |generated| {
      ensure_that(generated, "every filter spelling holds on a named struct", |sample| {
        satisfies_composed_filters(sample.foo, sample.bar, sample.baz, sample.quux, sample.wibble)
      })
    })
  }

  #[test]
  fn t1_test() -> Checked<T1> {
    ensure_property(&any::<T1>(), "every filter spelling holds under container params", |generated| {
      ensure_that(generated, "every filter spelling holds under container params", |sample| {
        satisfies_composed_filters(sample.foo, sample.bar, sample.baz, sample.quux, sample.wibble)
      })
    })
  }

  #[test]
  fn t2_test() -> Checked<T2> {
    ensure_property(&any::<T2>(), "every filter spelling holds on a tuple struct", |generated| {
      ensure_that(generated, "every filter spelling holds on a tuple struct", |sample| {
        satisfies_composed_filters(sample.0, sample.1, sample.2, sample.3, sample.4)
      })
    })
  }

  #[test]
  fn t3_test() -> Checked<T3> {
    ensure_property(&any::<T3>(), "the duplicate tuple-struct spelling holds", |generated| {
      ensure_that(generated, "the duplicate tuple-struct spelling holds", |sample| {
        satisfies_composed_filters(sample.0, sample.1, sample.2, sample.3, sample.4)
      })
    })
  }

  #[test]
  fn t4_test() -> Checked<T4> {
    ensure_property(&any::<T4>(), "only V0 with an even field survives the filters", |generated| {
      ensure_that(
        generated,
        "only V0 with an even field survives the filters",
        |sample| match *sample {
          T4::V0 {
            field,
          } => even(&field),
          T4::V1 => false,
        },
      )
    })
  }

  #[test]
  fn t5_test() -> Checked<T5> {
    ensure_property(
      &any::<T5>(),
      "variant and field filters compose with the explicit strategy",
      |generated| {
        ensure_that(
          generated,
          "variant and field filters compose with the explicit strategy",
          |sample| match *sample {
            T5::V0 {
              field,
            } => rem3(&field) && even(&field),
            T5::V1(field) => field < 1000 && field.is_multiple_of(5),
          },
        )
      },
    )
  }

  #[test]
  fn t6_test() -> Checked<T6> {
    ensure_property(
      &any::<T6>(),
      "variant and field filters compose with the params strategy",
      |generated| {
        ensure_that(
          generated,
          "variant and field filters compose with the params strategy",
          |sample| match *sample {
            T6::V0 {
              field,
            } => rem3(&field) && even(&field),
            T6::V1(field) => field < 100 && field.is_multiple_of(5),
          },
        )
      },
    )
  }

  #[test]
  fn t7_test() -> Checked<T7> {
    ensure_property(&any::<T7>(), "both accumulated field filters hold", |generated| {
      ensure_that(generated, "both accumulated field filters hold", |sample| {
        even(&sample.foo) && rem3(&sample.foo)
      })
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
