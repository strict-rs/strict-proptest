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
  mod properties;
  mod support;

  use properties::Checked;
  use properties::check_generated;
  use proptest::prelude::*;
  use proptest_derive::Arbitrary;
  use support::assert_arbitrary;

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

  /// Keep field filters identical while varying the owner of their parameters.
  macro_rules! named_filter_fixture {
    ($(#[$container:meta])* $name:ident, [$($first:tt)*], [$($last:tt)*]) => {
      #[derive(Debug, Arbitrary)]
      $(#[$container])*
      #[proptest(filter("|x| x.foo.rem_euclid(3) == 0"))]
      struct $name {
        #[proptest($($first)* filter(even))]
        foo: usize,
        #[proptest(filter("|x| x.rem_euclid(2) == 1"))]
        bar: usize,
        #[proptest(strategy = "0..100usize", filter = "|x| x.rem_euclid(2) == 1")]
        baz: usize,
        #[proptest(value = "42", filter(even))]
        quux: usize,
        #[proptest($($last)* strategy("0..=params.0"), filter("|x| *x > 2"))]
        wibble: usize,
      }
    };
  }

  named_filter_fixture!(T0, [no_params,], [params(Param),]);
  named_filter_fixture!(
    #[proptest(params(Param))]
    T1,
    [],
    []
  );

  /// Instantiate each tuple fixture with the same composed attribute spellings.
  macro_rules! tuple_filter_fixtures {
    ($($name:ident),+) => {
      $(
        #[derive(Debug, Arbitrary)]
        #[proptest(filter("|x| x.0.rem_euclid(3) == 0"))]
        struct $name(
          #[proptest(no_params, filter(even))] usize,
          #[proptest(filter("|x| x.rem_euclid(2) == 1"))] usize,
          #[proptest(strategy = "0..100usize", filter = "|x| x.rem_euclid(2) == 1")] usize,
          #[proptest(value = "42", filter(even))] usize,
          #[proptest(params(Param), strategy("0..=params.0"), filter("|x| *x > 2"))] usize,
        );
      )+
    };
  }

  tuple_filter_fixtures!(T2, T3);

  /// Keep named predicate functions while sharing rejection of other variants.
  macro_rules! variant_filter {
    ($(#[$attribute:meta])* [$($qualifier:tt)*] $name:ident($candidate:ident: $fixture:ty) {
      $pattern:pat => $predicate:expr
    }) => {
      $(#[$attribute])*
      $($qualifier)* fn $name($candidate: &$fixture) -> bool {
        if let $pattern = *$candidate {
          $predicate
        } else {
          false
        }
      }
    };
  }

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

  variant_filter! {
    #[allow(
      clippy::single_call_fn,
      reason = "T5::V0 filter predicate accepting fields divisible by three"
    )]
    [] t5_v0_rem_3(candidate: T5) {
      T5::V0 { field } => rem3(&field)
    }
  }

  variant_filter! {
    #[allow(
      clippy::single_call_fn,
      reason = "T5::V1 filter predicate accepting fields divisible by five"
    )]
    [const] t5_v1_rem_5(candidate: T5) {
      T5::V1(field) => field.is_multiple_of(5)
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

  variant_filter! {
    #[allow(
      clippy::single_call_fn,
      reason = "T6::V0 filter predicate accepting fields divisible by three"
    )]
    [] t6_v0_rem_3(candidate: T6) {
      T6::V0 { field } => rem3(&field)
    }
  }

  variant_filter! {
    #[allow(
      clippy::single_call_fn,
      reason = "T6::V1 filter predicate accepting fields divisible by five"
    )]
    [const] t6_v1_rem_5(candidate: T6) {
      T6::V1(field) => field.is_multiple_of(5)
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
    let context = "every filter spelling holds on a named struct";
    check_generated(&any::<T0>(), context, context, |sample: &T0| {
      satisfies_composed_filters(sample.foo, sample.bar, sample.baz, sample.quux, sample.wibble)
    })
  }

  #[test]
  fn t1_test() -> Checked<T1> {
    let context = "every filter spelling holds under container params";
    check_generated(&any::<T1>(), context, context, |sample: &T1| {
      satisfies_composed_filters(sample.foo, sample.bar, sample.baz, sample.quux, sample.wibble)
    })
  }

  #[test]
  fn t2_test() -> Checked<T2> {
    let context = "every filter spelling holds on a tuple struct";
    check_generated(&any::<T2>(), context, context, |sample: &T2| {
      satisfies_composed_filters(sample.0, sample.1, sample.2, sample.3, sample.4)
    })
  }

  #[test]
  fn t3_test() -> Checked<T3> {
    let context = "the duplicate tuple-struct spelling holds";
    check_generated(&any::<T3>(), context, context, |sample: &T3| {
      satisfies_composed_filters(sample.0, sample.1, sample.2, sample.3, sample.4)
    })
  }

  #[test]
  fn t4_test() -> Checked<T4> {
    let context = "only V0 with an even field survives the filters";
    check_generated(&any::<T4>(), context, context, |sample| match *sample {
      T4::V0 {
        field,
      } => even(&field),
      T4::V1 => false,
    })
  }

  /// Check the same variant and field filters against each strategy's own bound.
  macro_rules! variant_filter_property {
    ($test:ident, $fixture:ident, $upper:expr, $context:literal) => {
      #[test]
      fn $test() -> Checked<$fixture> {
        check_generated(&any::<$fixture>(), $context, $context, |sample| match *sample {
          $fixture::V0 {
            field,
          } => rem3(&field) && even(&field),
          $fixture::V1(field) => field < $upper && field.is_multiple_of(5),
        })
      }
    };
  }

  variant_filter_property!(t5_test, T5, 1000, "variant and field filters compose with the explicit strategy");
  variant_filter_property!(t6_test, T6, 100, "variant and field filters compose with the params strategy");

  #[test]
  fn t7_test() -> Checked<T7> {
    let context = "both accumulated field filters hold";
    check_generated(&any::<T7>(), context, context, |sample: &T7| even(&sample.foo) && rem3(&sample.foo))
  }

  assert_arbitrary!(T0, T1, T2, T3, T4, T5, T6, T7,);
}
