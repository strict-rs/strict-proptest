// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Named property cases for derived-value integration fixtures.

/// Declare separately runnable properties with concrete subjects and native outcomes.
macro_rules! derived_properties {
  ($(
    $name:ident(
      $subject:ty, $strategy:expr, $property_context:literal,
      $assertion_context:literal, $predicate:expr $(,)?
    );
  )+) => {
    $(
      #[test]
      fn $name() -> $crate::tests::properties::Checked<$subject> {
        $crate::tests::properties::check_generated(
          &$strategy,
          $property_context,
          $assertion_context,
          $predicate,
        )
      }
    )+
  };
}

pub(super) use derived_properties;
