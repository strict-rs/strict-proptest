// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Coverage for the `#[proptest(value = ...)]` and `#[proptest(value(...))]`
//! modifiers that pin a field or variant to a constant.
//!
//! The derived types cover value expressions written as string literals, bare
//! integer literals, arithmetic expressions, and `fn`-path calls, applied to
//! struct fields, whole enum variants, and variant fields; each property
//! checks the generated value equals the pinned constant.

#[cfg(test)]
mod tests {
  use proptest::prelude::Arbitrary;
  use proptest::prelude::any;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  /// Each assertion retains the complete generated value.
  type Checked<T> = PropertyResult<T, T, PredicateFailure<T>>;

  #[derive(Debug, Arbitrary)]
  struct T0 {
    #[proptest(value = "42")]
    field:  usize,
    #[proptest(value("24"))]
    bar:    usize,
    #[proptest(value = "24 + 24usize")]
    baz:    usize,
    #[proptest(value = 1337)]
    quux:   usize,
    #[proptest(value(7331))]
    wibble: usize,
    #[proptest(value("2 * 2 + 9usize.div_euclid(3)"))]
    wobble: usize,
  }

  #[derive(Debug, Arbitrary)]
  struct T1(#[proptest(value = "24")] usize);

  #[derive(Debug, Arbitrary)]
  enum T2 {
    V0,
    #[proptest(value = "T2::V1 { field: 1337 }")]
    V1 {
      field: usize,
    },
  }

  #[derive(Debug, Arbitrary)]
  enum T3 {
    V0,
    #[proptest(value = "T3::V1(7331)")]
    V1(usize),
  }

  #[derive(Debug, Arbitrary)]
  enum T4 {
    V0,
    V1 {
      #[proptest(value = "6")]
      field: usize,
    },
  }

  #[derive(Debug, Arbitrary)]
  enum T5 {
    V0,
    V1(#[proptest(value = "9")] usize),
  }

  #[derive(Debug, Arbitrary)]
  struct T6 {
    #[proptest(value = "\"alpha\".to_owned()")]
    alpha: String,
    #[proptest(strategy = "0..100usize")]
    beta:  usize,
  }

  const fn foo() -> usize {
    42
  }

  #[derive(Debug, Arbitrary)]
  struct CallFun {
    #[proptest(value = "foo()")]
    foo: usize,

    #[proptest(value(foo))]
    bar: usize,
  }

  /// Shares its name with the local binding the generated `Value` constructor
  /// introduces (`let value_fn: fn() -> _ = || <expr>; value_fn`). Because a
  /// `let` target is not in scope inside its own initializer, `value_fn()` in the
  /// pinned expression must resolve to this module-level fn, not the local.
  const fn value_fn() -> usize {
    7788
  }

  #[derive(Debug, Arbitrary)]
  struct ValueFnCollision {
    #[proptest(value = "value_fn()")]
    field:    usize,
    #[proptest(value = "value_fn().saturating_add(1)")]
    plus_one: usize,
  }

  #[test]
  fn t0_fixed_fields() -> Checked<T0> {
    ensure_property(&any::<T0>(), "every value spelling pins its struct field", |generated| {
      ensure_that(generated, "every value spelling pins its struct field", |sample| {
        (sample.field, sample.bar, sample.baz, sample.quux, sample.wibble, sample.wobble) == (42, 24, 48, 1337, 7331, 7)
      })
    })
  }

  #[test]
  fn t1_field_always_24() -> Checked<T1> {
    ensure_property(&any::<T1>(), "a tuple-struct value pins its field", |generated| {
      ensure_that(generated, "a tuple-struct value pins its field", |sample| sample.0 == 24)
    })
  }

  #[test]
  fn t2_v1_always_1337() -> Checked<T2> {
    ensure_property(&any::<T2>(), "a struct-variant value pins its field", |generated| {
      ensure_that(generated, "a struct-variant value pins its field", |sample| match *sample {
        T2::V0 => true,
        T2::V1 {
          field,
        } => field == 1337,
      })
    })
  }

  #[test]
  fn t3_v1_always_7331() -> Checked<T3> {
    ensure_property(&any::<T3>(), "a tuple-variant value pins its field", |generated| {
      ensure_that(generated, "a tuple-variant value pins its field", |sample| match *sample {
        T3::V0 => true,
        T3::V1(field) => field == 7331,
      })
    })
  }

  #[test]
  fn t4_v1_always_1337() -> Checked<T4> {
    ensure_property(
      &any::<T4>(),
      "a field-level value inside a struct variant pins the field",
      |generated| {
        ensure_that(
          generated,
          "a field-level value inside a struct variant pins the field",
          |sample| match *sample {
            T4::V0 => true,
            T4::V1 {
              field,
            } => field == 6,
          },
        )
      },
    )
  }

  #[test]
  fn t5_v1_always_7331() -> Checked<T5> {
    ensure_property(
      &any::<T5>(),
      "a field-level value inside a tuple variant pins the field",
      |generated| {
        ensure_that(
          generated,
          "a field-level value inside a tuple variant pins the field",
          |sample| match *sample {
            T5::V0 => true,
            T5::V1(field) => field == 9,
          },
        )
      },
    )
  }

  #[test]
  fn t6_alpha_beta() -> Checked<T6> {
    ensure_property(&any::<T6>(), "value and strategy fields coexist on one struct", |generated| {
      ensure_that(generated, "value and strategy fields coexist on one struct", |sample| {
        sample.alpha == "alpha" && sample.beta < 100
      })
    })
  }

  #[test]
  fn call_fun_always_42() -> Checked<CallFun> {
    ensure_property(&any::<CallFun>(), "fn-path value spellings call the function", |generated| {
      ensure_that(generated, "fn-path value spellings call the function", |sample| {
        (sample.foo, sample.bar) == (42, 42)
      })
    })
  }

  #[test]
  fn value_fn_name_collision_resolves_to_user_fn() -> Checked<ValueFnCollision> {
    ensure_property(
      &any::<ValueFnCollision>(),
      "both value_fn expressions resolve to the user function",
      |generated| {
        ensure_that(generated, "both value_fn expressions resolve to the user function", |sample| {
          (sample.field, sample.plus_one) == (7788, 7789)
        })
      },
    )
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
    assert_arbitrary::<CallFun>();
    assert_arbitrary::<ValueFnCollision>();
  }
}
