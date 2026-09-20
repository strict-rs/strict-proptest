// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Shared compile-time contracts for the derive integration fixtures.

/// Require every fixture's concrete `Arbitrary` implementation in a named test.
macro_rules! assert_arbitrary {
  ($($fixture:ty),+ $(,)?) => {
    #[test]
    fn asserting_arbitrary() {
      use proptest::prelude::Arbitrary;

      fn assert_arbitrary<T: Arbitrary>() {}

      $(assert_arbitrary::<$fixture>();)+
    }
  };
}

pub(super) use assert_arbitrary;
