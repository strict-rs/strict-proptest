// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Coverage for the `#[proptest(strategy = ...)]` modifier and its
//! `strategy(...)` and `strategy(fn)` spellings.
//!
//! The derived types attach custom `Strategy` expressions to named-struct
//! fields, tuple-struct fields, and enum-variant fields, and each property
//! confirms the generated value falls in the range the strategy produces.

#[cfg(test)]
mod tests {
  use proptest::prelude::Arbitrary;
  use proptest::prelude::Strategy;
  use proptest::prelude::any;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  /// The generated value is retained on either assertion outcome.
  type Checked<T> = PropertyResult<T, T, PredicateFailure<T>>;

  fn make_strategy(start: usize) -> impl Strategy<Value = usize> {
    (start..100).prop_map(|x| x.saturating_mul(2))
  }

  fn make_strategy2() -> impl Strategy<Value = usize> {
    make_strategy(88)
  }

  #[derive(Debug, Arbitrary)]
  struct T0 {
    #[proptest(strategy = "make_strategy(0)")]
    foo: usize,
    #[proptest(strategy("make_strategy(11)"))]
    bar: usize,
    #[proptest(strategy(make_strategy2))]
    baz: usize,
  }

  #[derive(Debug, Arbitrary)]
  struct T1(
    #[proptest(strategy = "make_strategy(22)")] usize,
    #[proptest(strategy("make_strategy(33)"))] usize,
    #[proptest(strategy(make_strategy2))] usize,
  );

  #[derive(Debug, Arbitrary)]
  enum T2 {
    V0(#[proptest(strategy("make_strategy(44)"))] usize),
    V1 {
      #[proptest(strategy = "make_strategy(55)")]
      field: usize,
    },
    V2(#[proptest(strategy = "make_strategy(66)")] usize),
    V3 {
      #[proptest(strategy("make_strategy(77)"))]
      field: usize,
    },
    V4(#[proptest(strategy(make_strategy2))] usize),
    V5 {
      #[proptest(strategy(make_strategy2))]
      field: usize,
    },
  }

  const fn is_consistent(start: usize, produced: usize) -> bool {
    produced.is_multiple_of(2) && produced < 200 && produced >= start.saturating_mul(2)
  }

  #[test]
  fn t0_test() -> Checked<T0> {
    ensure_property(&any::<T0>(), "every strategy spelling drives its named struct field", |generated| {
      ensure_that(generated, "each named field doubles a value from its start range", |sample| {
        is_consistent(0, sample.foo) && is_consistent(11, sample.bar) && is_consistent(88, sample.baz)
      })
    })
  }

  #[test]
  fn t1_test() -> Checked<T1> {
    ensure_property(&any::<T1>(), "every strategy spelling drives its tuple field", |generated| {
      ensure_that(generated, "each tuple field doubles a value from its start range", |sample| {
        is_consistent(22, sample.0) && is_consistent(33, sample.1) && is_consistent(88, sample.2)
      })
    })
  }

  #[test]
  fn t2_test() -> Checked<T2> {
    ensure_property(&any::<T2>(), "every strategy spelling drives its enum variant field", |generated| {
      ensure_that(
        generated,
        "each variant doubles a value from its start range",
        |sample| match *sample {
          T2::V0(field) => is_consistent(44, field),
          T2::V1 {
            field,
          } => is_consistent(55, field),
          T2::V2(field) => is_consistent(66, field),
          T2::V3 {
            field,
          } => is_consistent(77, field),
          T2::V4(payload) => is_consistent(88, payload),
          T2::V5 {
            field,
          } => is_consistent(88, field),
        },
      )
    })
  }

  #[test]
  fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<T0>();
    assert_arbitrary::<T1>();
    assert_arbitrary::<T2>();
  }
}
