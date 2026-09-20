// Copyright 2019 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Baseline `#[derive(Arbitrary)]` coverage for plain named-field structs
//! with no `#[proptest(...)]` modifiers.
//!
//! The derived types range from a single field up to twenty fields over
//! the primitive and standard types, and `asserting_arbitrary` checks
//! each derived type resolves an `Arbitrary` impl with the correct bounds.

#[cfg(test)]
mod tests {
  mod support;

  use proptest_derive::Arbitrary;
  use support::assert_arbitrary;

  #[derive(Debug, Arbitrary)]
  struct T1 {
    _f1: u8,
  }

  /// Build each fixture with its predecessor's complete field prefix plus the new fields.
  macro_rules! named_struct_prefixes {
    (@fields { $($fields:tt)* };) => {};
    (@fields { $($fields:tt)* }; $name:ident { $($additional:tt)* } $($remaining:tt)*) => {
      #[derive(Debug, Arbitrary)]
      struct $name {
        $($fields)*
        $($additional)*
      }
      named_struct_prefixes!(@fields { $($fields)* $($additional)* }; $($remaining)*);
    };
    ($($cases:tt)*) => {
      named_struct_prefixes!(@fields {}; $($cases)*);
    };
  }

  named_struct_prefixes! {
    T10 {
      _f1: char,
      _f2: String,
      _f3: u8,
      _f4: u16,
      _f5: u32,
      _f6: u64,
      _f7: u128,
      _f8: f32,
      _f9: f64,
      _f10: bool,
    }
    T11 { _f11: char, }
    T13 { _f12: String, _f13: u8, }
    T19 {
      _f14: u16,
      _f15: u32,
      _f16: u64,
      _f17: u128,
      _f18: f32,
      _f19: f64,
    }
    T20 { _f20: bool, }
  }

  assert_arbitrary!(T1, T10, T11, T13, T19, T20,);
}
