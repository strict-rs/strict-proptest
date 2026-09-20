// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for `#[derive(Arbitrary)]` on fields typed as
//! associated-type projections.
//!
//! Derives `Arbitrary` for structs whose field types are associated-type
//! projections in every spelling (`<TypeA as Func>::Out`, `Tyvar::OutB`,
//! `<Tyvar as FuncB>::OutB`, and nested projections), including such
//! projections wrapped in `Vec`. The derive must still infer the correct
//! `Arbitrary` bounds, and the generated projected fields carry their
//! pinned values.

#[cfg(test)]
mod tests {
  mod properties;
  mod support;

  use core::convert::Infallible;

  use properties::Checked;
  use properties::check_generated;
  use proptest::prelude::any;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use support::assert_arbitrary;

  trait Func {
    type Out;
  }
  trait FuncA {
    type OutA: FuncB;
  }
  trait FuncB {
    type OutB;
  }

  #[derive(Debug)]
  struct TypeA;

  #[derive(Debug)]
  struct TypeB;

  #[derive(Debug, Arbitrary)]
  struct OutTy {
    #[proptest(value = "42")]
    val: usize,
  }

  impl Func for TypeA {
    type Out = OutTy;
  }
  impl FuncA for TypeA {
    type OutA = TypeB;
  }
  impl FuncB for TypeB {
    type OutB = OutTy;
  }

  #[derive(Debug, Arbitrary)]
  struct T0 {
    field: <TypeA as Func>::Out,
  }

  #[derive(Debug, Arbitrary)]
  struct T1 {
    _field: Vec<u8>,
  }

  #[derive(Debug, Arbitrary)]
  struct T2 {
    _field: Vec<Vec<u8>>,
  }

  #[derive(Debug, Arbitrary)]
  struct T3 {
    field: Vec<<TypeA as Func>::Out>,
  }

  #[derive(Debug, Arbitrary)]
  struct T4<Tyvar: FuncB> {
    field: Tyvar::OutB,
  }

  #[derive(Arbitrary)]
  struct T5<Tyvar: FuncB> {
    field: <Tyvar>::OutB,
  }

  #[derive(Arbitrary)]
  struct T6<Tyvar: FuncB> {
    field: <Tyvar as FuncB>::OutB,
  }

  #[derive(Arbitrary)]
  struct T7<Tyvar: FuncA> {
    field: <Tyvar::OutA as FuncB>::OutB,
  }

  #[derive(Arbitrary)]
  struct T8<Tyvar: FuncA> {
    field: <<Tyvar>::OutA as FuncB>::OutB,
  }

  #[derive(Arbitrary)]
  struct T9<Tyvar: FuncA> {
    field: <<Tyvar as FuncA>::OutA as FuncB>::OutB,
  }

  #[derive(Debug, Arbitrary)]
  struct T10<Tyvar: FuncB> {
    field: Vec<Tyvar::OutB>,
  }

  #[derive(Arbitrary)]
  struct T11<Tyvar: FuncB> {
    field: Vec<<Tyvar>::OutB>,
  }

  #[derive(Arbitrary)]
  struct T12<Tyvar: FuncB> {
    field: Vec<<Tyvar as FuncB>::OutB>,
  }

  #[derive(Arbitrary)]
  struct T13<Tyvar: FuncA> {
    field: Vec<<Tyvar::OutA as FuncB>::OutB>,
  }

  #[derive(Arbitrary)]
  struct T14<Tyvar: FuncA> {
    field: Vec<<<Tyvar>::OutA as FuncB>::OutB>,
  }

  #[derive(Arbitrary)]
  struct T15<Tyvar: FuncA> {
    field: Vec<<<Tyvar as FuncA>::OutA as FuncB>::OutB>,
  }

  macro_rules! debug {
    ($trait:path, $ty:ident) => {
      impl<T: $trait> ::std::fmt::Debug for $ty<T> {
        fn fmt(&self, fmt: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
          fmt.debug_struct(stringify!($ty)).field("field", &"<redacted>").finish()
        }
      }
    };
  }

  debug!(FuncB, T5);
  debug!(FuncB, T6);
  debug!(FuncA, T7);
  debug!(FuncA, T8);
  debug!(FuncA, T9);

  debug!(FuncB, T11);
  debug!(FuncB, T12);
  debug!(FuncA, T13);
  debug!(FuncA, T14);
  debug!(FuncA, T15);

  assert_arbitrary!(
    T0,
    T1,
    T2,
    T3,
    T4<TypeB>,
    T5<TypeB>,
    T6<TypeB>,
    T7<TypeA>,
    T8<TypeA>,
    T9<TypeA>,
    T10<TypeB>,
    T11<TypeB>,
    T12<TypeB>,
    T13<TypeA>,
    T14<TypeA>,
    T15<TypeA>,
  );

  /// Context shared by every projected-field property.
  const PROPERTY_CONTEXT: &str = "associated-type fields generate pinned values";
  /// Assertion context shared by each pinned projected field.
  const ASSERTION_CONTEXT: &str = "every projected value is pinned to 42";

  #[test]
  fn t0_field_val_42() -> Checked<T0> {
    check_generated(&any::<T0>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| sample.field.val == 42)
  }

  #[test]
  fn t1_no_panic() -> PropertyResult<T1, T1, Infallible> {
    ensure_property(&any::<T1>(), "a projected field generates", Ok)
  }

  #[test]
  fn t2_no_panic() -> PropertyResult<T2, T2, Infallible> {
    ensure_property(&any::<T2>(), "a projected field generates", Ok)
  }

  #[test]
  fn t3_all_42() -> Checked<T3> {
    check_generated(&any::<T3>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.iter().all(|element| element.val == 42)
    })
  }

  #[test]
  fn t4_field_val_42() -> Checked<T4<TypeB>> {
    check_generated(&any::<T4<TypeB>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.val == 42
    })
  }

  #[test]
  fn t5_field_val_42() -> Checked<T5<TypeB>> {
    check_generated(&any::<T5<TypeB>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.val == 42
    })
  }

  #[test]
  fn t6_field_val_42() -> Checked<T6<TypeB>> {
    check_generated(&any::<T6<TypeB>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.val == 42
    })
  }

  #[test]
  fn t7_field_val_42() -> Checked<T7<TypeA>> {
    check_generated(&any::<T7<TypeA>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.val == 42
    })
  }

  #[test]
  fn t8_field_val_42() -> Checked<T8<TypeA>> {
    check_generated(&any::<T8<TypeA>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.val == 42
    })
  }

  #[test]
  fn t9_field_val_42() -> Checked<T9<TypeA>> {
    check_generated(&any::<T9<TypeA>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.val == 42
    })
  }

  #[test]
  fn t10_all_42() -> Checked<T10<TypeB>> {
    check_generated(&any::<T10<TypeB>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.iter().all(|element| element.val == 42)
    })
  }

  #[test]
  fn t11_all_42() -> Checked<T11<TypeB>> {
    check_generated(&any::<T11<TypeB>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.iter().all(|element| element.val == 42)
    })
  }

  #[test]
  fn t12_all_42() -> Checked<T12<TypeB>> {
    check_generated(&any::<T12<TypeB>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.iter().all(|element| element.val == 42)
    })
  }

  #[test]
  fn t13_all_42() -> Checked<T13<TypeA>> {
    check_generated(&any::<T13<TypeA>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.iter().all(|element| element.val == 42)
    })
  }

  #[test]
  fn t14_all_42() -> Checked<T14<TypeA>> {
    check_generated(&any::<T14<TypeA>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.iter().all(|element| element.val == 42)
    })
  }

  #[test]
  fn t15_all_42() -> Checked<T15<TypeA>> {
    check_generated(&any::<T15<TypeA>>(), PROPERTY_CONTEXT, ASSERTION_CONTEXT, |sample| {
      sample.field.iter().all(|element| element.val == 42)
    })
  }
}
