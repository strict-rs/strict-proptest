//-
// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Consumer-side runtime coverage for the `#[property_test]` attribute
//! macro: the generated wrappers run through the strict runner and hand
//! back `proptest::strict::TestResult` instead of panicking.

#![cfg(feature = "attr-macro")]

#[cfg(test)]
mod tests {
    use proptest::strict::TestResult;
    use strict_test_support::{
        ensure, ensure_contains, ensure_eq, ensure_some,
    };

    /// Regression for <https://github.com/proptest-rs/proptest/issues/601>
    ///
    /// `mut` must survive both on a plain ident argument and on an ident
    /// nested inside a tuple-destructuring pattern when the macro rewrites
    /// the signature into a generated params struct.
    #[proptest::property_test]
    fn attr_macro_does_not_clobber_mutability(
        mut x: i32,
        (mut y, _z): (i32, i32),
    ) -> TestResult {
        x = x.saturating_sub(x);
        y = y.saturating_sub(y);
        ensure_eq(&x, &y, "reassigned mut bindings agree after zeroing")
    }

    /// Falsifying fixture for the negative-polarity check below. `#[ignore]`
    /// keeps the harness from running it as a failing test; the wrapper is
    /// still an ordinary `fn() -> TestResult`, so the test after it calls it
    /// directly.
    #[ignore = "falsifying fixture: exercised by the test below via a direct call"]
    #[proptest::property_test]
    fn falsifying_wrapper_surfaces_test_failure(x: i32) -> TestResult {
        ensure(x < 1, "input stays below one")
    }

    /// The generated wrapper propagates a falsified property as an `Err`
    /// carrying the shrunk minimal counterexample — the call returning at all
    /// proves no panic path is involved.
    #[test]
    fn generated_wrapper_returns_test_failure_instead_of_panicking()
    -> TestResult {
        let failure = ensure_some(
            falsifying_wrapper_surfaces_test_failure().err(),
            "a falsifying property must surface as Err",
        )?;
        let rendered = failure.to_string();
        ensure_contains(
            &rendered,
            "property falsified",
            "the verdict names the falsified family",
        )?;
        ensure_contains(
            &rendered,
            "minimal failing input:",
            "the verdict carries the engine's shrink report",
        )?;
        ensure_contains(
            &rendered,
            "x: 1",
            "shrinking converges to the minimal counterexample",
        )
    }
}
