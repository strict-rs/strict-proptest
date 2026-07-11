// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-time lint contract for generated `#[derive(Arbitrary)]` impls.

#![deny(warnings, unsafe_code)]

use proptest::arbitrary::Arbitrary as ProptestArbitrary;
use proptest_derive::Arbitrary;
use std::{hint::black_box, vec::IntoIter};

#[derive(Debug, Arbitrary)]
struct LintCleanUnit;

#[derive(Debug, Arbitrary)]
struct LintCleanProjection<T: Iterator> {
    _first: T::Item,
    _second: T::Item,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_arbitrary<T: ProptestArbitrary>() {}

    #[test]
    fn generated_impls_resolve_under_rustc_lint_denies() {
        assert_arbitrary::<LintCleanUnit>();
        assert_arbitrary::<LintCleanProjection<IntoIter<u8>>>();

        let unit = LintCleanUnit;
        let _unit: LintCleanUnit = black_box(unit);

        let value = LintCleanProjection::<IntoIter<u8>> {
            _first: 1,
            _second: 2,
        };
        let LintCleanProjection {
            _first: first,
            _second: second,
        } = value;
        let _fields: (u8, u8) = black_box((first, second));
    }
}
