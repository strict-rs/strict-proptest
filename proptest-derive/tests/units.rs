// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for `#[derive(Arbitrary)]` on degenerate
//! empty shapes.
//!
//! Derives `Arbitrary` for the unit struct `T0;` and enums whose sole variant
//! is an idiomatic unit form. Each impl must resolve even though there is
//! nothing to generate.

#[cfg(test)]
mod tests {
  mod support;

  use proptest_derive::Arbitrary;
  use support::assert_arbitrary;

  #[derive(Debug, Arbitrary)]
  struct T0;

  #[derive(Debug, Arbitrary)]
  enum T3 {
    V0,
  }

  #[derive(Debug, Arbitrary)]
  enum T4 {
    V1,
  }

  #[derive(Debug, Arbitrary)]
  enum T5 {
    V2,
  }

  assert_arbitrary!(T0, T3, T4, T5,);
}
