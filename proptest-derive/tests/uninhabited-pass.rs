// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for the passing side of the derive's
//! uninhabited-field detection.
//!
//! Exercises `#[derive(Arbitrary)]` on types whose fields are
//! `core::convert::Infallible`, arrays `[Infallible; N]` with const-expression lengths, and
//! uninhabited types hidden behind a `macro_rules!` call or an
//! associated-type projection that the derive cannot inspect. Generation
//! must drop uninhabited enum variants (leaving only the inhabited one)
//! while still emitting a working `Arbitrary` impl.

#[cfg(test)]
mod tests {
  extern crate core as real_core;

  use proptest::prelude::Arbitrary;
  use proptest::prelude::any;
  use proptest::strict::TestResult;
  use proptest::strict::ensure_property;
  use proptest_derive::Arbitrary;
  use strict_test_support::ensure;

  mod core {
    pub(in crate::tests) mod convert {
      pub(in crate::tests) use super::super::real_core::convert::Infallible;
    }
  }

  // Various arithmetic and basic things.
  #[derive(Debug, Arbitrary)]
  enum Ty1 {
    // Ensure that all of the types below are deemed uninhabited:
    _V2(core::convert::Infallible),
    _V3([core::convert::Infallible; 1]),
    _V4([core::convert::Infallible; 2 - 1]),
    _V5([core::convert::Infallible; 2 * 3]),
    _V6([core::convert::Infallible; 4 - 2]),
    _V7([core::convert::Infallible; 0b10 ^ 0b11]),
    _V8([core::convert::Infallible; 0b11 & 0b01]),
    _V9([core::convert::Infallible; 0b10 | 0b01]),
    _V10([core::convert::Infallible; 0b10 << 1]),
    _V11([core::convert::Infallible; 0b10 >> 1]),
    _V12([core::convert::Infallible; !0 - 18_446_744_073_709_551_614]),
    _V13([core::convert::Infallible; 1 + 2 * (6 - 4)]),
    V1,
  }

  #[test]
  fn ty1_always_v1() -> TestResult {
    ensure_property(&any::<Ty1>(), "every uninhabited-array variant is dropped from generation", |v1| {
      ensure(matches!(v1, Ty1::V1), "only the inhabited variant appears")
    })
  }

  // Can't inspect type macros called as  mac!(uninhabited_type).
  macro_rules! tymac {
    ($ignore:ty) => {
      u8
    };
  }

  #[derive(Debug, Arbitrary)]
  struct TyMac0 {
    _field: tymac!(core::convert::Infallible),
  }

  #[derive(Debug, Arbitrary)]
  struct TyMac1 {
    _baz: tymac!([core::convert::Infallible; 3 + 4]),
  }

  enum _TyMac2 {
    #[deny(dead_code)]
    V0(tymac!((u8, core::convert::Infallible, usize))),
  }

  // Can't inspect projections through associated types:
  trait Fun {
    type Prj;
  }
  impl Fun for core::convert::Infallible {
    type Prj = u8;
  }
  impl Fun for (core::convert::Infallible, usize, core::convert::Infallible) {
    type Prj = u8;
  }

  #[derive(Debug, Arbitrary)]
  enum UsePrj0 {
    V0(<core::convert::Infallible as Fun>::Prj),
  }

  impl UsePrj0 {
    const fn projection(self) -> <core::convert::Infallible as Fun>::Prj {
      let Self::V0(payload) = self;
      payload
    }
  }

  #[derive(Debug, Arbitrary)]
  enum UsePrj1 {
    V0(<(core::convert::Infallible, usize, core::convert::Infallible) as Fun>::Prj),
  }

  impl UsePrj1 {
    const fn projection(self) -> <(core::convert::Infallible, usize, core::convert::Infallible) as Fun>::Prj {
      let Self::V0(payload) = self;
      payload
    }
  }

  #[test]
  fn associated_projection_fields_are_generated() -> TestResult {
    ensure_property(
      &(any::<UsePrj0>(), any::<UsePrj1>()),
      "projection-hidden fields the derive cannot inspect still generate",
      |(prj0, prj1)| {
        let _: u8 = prj0.projection();
        let _: u8 = prj1.projection();
        Ok(())
      },
    )
  }

  #[test]
  fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<Ty1>();
    assert_arbitrary::<TyMac0>();
    assert_arbitrary::<TyMac1>();
    assert_arbitrary::<UsePrj0>();
    assert_arbitrary::<UsePrj1>();
  }
}
