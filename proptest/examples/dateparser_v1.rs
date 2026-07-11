//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Tutorial date-parser property test at its first, buggy stage.
//!
//! Backs the Proptest Book's `getting-started` chapter. A naive `parse_date`
//! byte-slices a length-10 `&str`, and the `doesnt_crash` property feeds it
//! arbitrary strings to demonstrate why the parser needs to treat byte ranges
//! as fallible.

use core::ops::Range;

#[cfg(feature = "strict-test")]
use proptest::strict::{TestFailure, ensure_property};
#[cfg(feature = "strict-test")]
use strict_test_support::ensure;

/// Slice the tutorial parser's fixed byte range, preserving the v1 lesson
/// that non-ASCII input can miss a byte-indexed parser boundary.
fn tutorial_byte_slice(input: &str, range: Range<usize>) -> Option<&str> {
    input.get(range)
}

/// Parse a date in `YYYY-MM-DD` form using the tutorial's first buggy parser.
fn parse_date(input: &str) -> Option<(u32, u32, u32)> {
    if 10 != input.len() {
        return None;
    }
    // !
    if "-" != tutorial_byte_slice(input, 4..5)?
        || "-" != tutorial_byte_slice(input, 7..8)?
    {
        return None;
    }

    let year = tutorial_byte_slice(input, 0..4)?;
    let month = tutorial_byte_slice(input, 6..7)?; // !
    let day = tutorial_byte_slice(input, 8..10)?;

    year.parse::<u32>().ok().and_then(|y| {
        month.parse::<u32>().ok().and_then(|month_num| {
            day.parse::<u32>()
                .ok()
                .map(|day_num| (y, month_num, day_num))
        })
    })
}

#[cfg(feature = "strict-test")]
/// Run the crash-resistance property over arbitrary generated strings.
#[allow(
    clippy::single_call_fn,
    reason = "name the tutorial crash-resistance property that the example main runs"
)]
fn doesnt_crash() -> Result<(), TestFailure> {
    ensure_property(
        &"\\PC*",
        "the tutorial parser handles arbitrary strings without panicking",
        |input| {
            let _parsed = parse_date(&input);
            Ok(())
        },
    )
}

#[cfg(feature = "strict-test")]
fn main() -> Result<(), TestFailure> {
    ensure(
        parse_date("2017-06-1").is_none(),
        "short dates are rejected",
    )?;
    ensure(
        parse_date("2017-06-170").is_none(),
        "long dates are rejected",
    )?;
    ensure(
        parse_date("2017006-17").is_none(),
        "missing separators are rejected",
    )?;
    ensure(
        parse_date("2017-06017").is_none(),
        "misplaced separators are rejected",
    )?;
    ensure(
        Some((2017, 6, 17)) == parse_date("2017-06-17"),
        "well-formed dates are parsed",
    )?;

    doesnt_crash()
}

#[cfg(not(feature = "strict-test"))]
fn main() {}
