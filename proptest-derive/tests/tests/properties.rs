// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Property assertions shared by derive integration fixtures.

use proptest::strategy::Strategy;
use proptest::strict::ensure_property;
use proptest::test_runner::PropertyResult;
use strict_test_support::PredicateFailure;
use strict_test_support::ensure_that;

/// Retain every complete generated subject and its native assertion failure.
pub(super) type Checked<T> = PropertyResult<T, T, PredicateFailure<T>>;

/// Check a derived strategy without erasing its values or either diagnostic context.
pub(super) fn check_generated<S: Strategy>(
  strategy: &S,
  property_context: &'static str,
  assertion_context: &'static str,
  predicate: impl Fn(&S::Value) -> bool,
) -> Checked<S::Value> {
  ensure_property(strategy, property_context, |generated| {
    ensure_that(generated, assertion_context, &predicate)
  })
}
