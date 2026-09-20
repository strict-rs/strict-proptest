// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Coverage for the `#[proptest(skip)]` modifier and stable uninhabited
//! variant detection.
//!
//! The derived enums mark some variants `#[proptest(skip)]` and give others an
//! uninhabited `Infallible` payload, and each property confirms only the
//! inhabited, unskipped variants are ever generated.

#[cfg(test)]
mod tests {
  extern crate core as real_core;

  use proptest::prelude::Arbitrary;
  use proptest::prelude::any;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  mod core {
    pub(in crate::tests) mod convert {
      pub(in crate::tests) use super::super::real_core::convert::Infallible;
    }
  }

  #[derive(Debug, Arbitrary)]
  enum Ty1 {
    V1,
    _V2(core::convert::Infallible),
    #[proptest(skip)]
    _V3,
  }

  #[derive(Debug, Arbitrary)]
  enum Ty2 {
    V1,
    V2,
    #[proptest(skip)]
    _V3,
    #[proptest(skip)]
    _V4,
  }

  #[test]
  fn ty1_always_v1() -> PropertyResult<Ty1, Ty1, PredicateFailure<Ty1>> {
    ensure_property(&any::<Ty1>(), "skipped and uninhabited variants never generate", |generated| {
      ensure_that(generated, "only the inhabited, unskipped variant appears", |sample| {
        matches!(sample, Ty1::V1)
      })
    })
  }

  #[test]
  fn ty_always_1_or_2() -> PropertyResult<Ty2, Ty2, PredicateFailure<Ty2>> {
    ensure_property(&any::<Ty2>(), "multiple skipped variants never generate", |generated| {
      ensure_that(generated, "only the unskipped variants appear", |sample| {
        matches!(sample, Ty2::V1 | Ty2::V2)
      })
    })
  }

  #[test]
  fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<Ty1>();
    assert_arbitrary::<Ty2>();
  }
}
