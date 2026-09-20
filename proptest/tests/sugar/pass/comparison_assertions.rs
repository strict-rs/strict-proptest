//-
// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Exercise exported comparison macros across the consumer-crate boundary.

use proptest::test_runner::TestCaseError;
use strict_test_support::PredicateFailure;
use strict_test_support::ensure_that;

/// Native macro outcomes, observed evaluation order, and independent expectations.
#[derive(Debug)]
struct Observation {
    /// The macro either continues normally or returns its original failure.
    outcome: Result<(), TestCaseError>,
    /// Operand, format-argument, and continuation evaluations in order.
    events: Vec<&'static str>,
    /// Exact failure diagnostic, including the external invocation location.
    expected_message: Option<String>,
    /// Expected execution order for this scenario.
    expected_events: &'static [&'static str],
}

/// Observe both exported macros without bypassing their public expansion.
macro_rules! observe {
    ($assertion:ident, $left:literal, $right:literal, $expected:expr,
     [$($event:literal),*] $(; $message:literal)?) => {{
        let mut events = Vec::new();
        let outcome = (|| -> Result<(), TestCaseError> {
            proptest::$assertion!(
                {
                    events.push("left");
                    String::from($left)
                },
                {
                    events.push("right");
                    String::from($right)
                },
                $("{detail}", detail = {
                    events.push("message");
                    $message
                },)?
            );
            events.push("continued");
            Ok(())
        })();
        let expected_message: Option<&str> = $expected;
        Observation {
            outcome,
            events,
            expected_message: expected_message.map(|message| format!("{message} at {}:{}", file!(), line!())),
            expected_events: &[$($event),*],
        }
    }};
}

fn main() -> Result<(), Box<PredicateFailure<[Observation; 8]>>> {
    let observations = [
        observe!(prop_assert_eq, "same", "same", None, ["left", "right", "continued"]),
        observe!(prop_assert_ne, "left", "right", None, ["left", "right", "continued"]),
        observe!(prop_assert_eq, "same", "same", None, ["left", "right", "continued"]; "unused detail"),
        observe!(prop_assert_ne, "left", "right", None, ["left", "right", "continued"]; "unused detail"),
        observe!(prop_assert_eq, "left", "right",
            Some("assertion failed: `(left == right)` \n  left: `\"left\"`,\n right: `\"right\"`"),
            ["left", "right"]),
        observe!(prop_assert_ne, "same", "same",
            Some("assertion failed: `(left != right)`\n  left: `\"same\"`,\n right: `\"same\"`"),
            ["left", "right"]),
        observe!(prop_assert_eq, "left", "right",
            Some("assertion failed: `(left == right)` \n  left: `\"left\"`, \n right: `\"right\"`: context"),
            ["left", "right", "message"]; "context"),
        observe!(prop_assert_ne, "same", "same",
            Some("assertion failed: `(left != right)`\n  left: `\"same\"`,\n right: `\"same\"`: context"),
            ["left", "right", "message"]; "context"),
    ];
    ensure_that(observations, "comparison macros preserve evaluation and diagnostics", |cases| {
        cases.iter().all(|case| {
            case.events == case.expected_events
                && match &case.expected_message {
                    None => case.outcome.is_ok(),
                    Some(message) => case.outcome.as_ref().is_err_and(|failure| {
                        matches!(failure, TestCaseError::Fail(reason) if reason.message() == message)
                    }),
                }
        })
    })
    .map(drop)
    .map_err(Box::new)
}
