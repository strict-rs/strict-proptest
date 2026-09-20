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

#[cfg(feature = "strict-test")]
use core::convert::Infallible;
use core::ops::Range;

#[cfg(feature = "strict-test")]
use proptest::strict::ensure_property;
#[cfg(feature = "strict-test")]
use proptest::test_runner::PropertyResult;
#[cfg(feature = "strict-test")]
use strict_test_support::PredicateFailure;
#[cfg(feature = "strict-test")]
use strict_test_support::ensure_that;

/// The parser's native date components.
type Date = (u32, u32, u32);
#[cfg(feature = "strict-test")]
/// Generated input and the parser's complete response.
type ParsedDate = (String, Option<Date>);
#[cfg(feature = "strict-test")]
/// Every completed crash-resistance evaluation remains available.
type ParserRun = PropertyResult<String, ParsedDate, Infallible>;
#[cfg(feature = "strict-test")]
/// A fixed input, its observed response, and its expected response.
type Fixture = (&'static str, Option<Date>, Option<Date>);
#[cfg(feature = "strict-test")]
/// Fixed examples and the complete randomized parser run.
type ParserExamples = ([Fixture; 5], ParserRun);

/// Slice the tutorial parser's fixed byte range, preserving the v1 lesson
/// that non-ASCII input can miss a byte-indexed parser boundary.
fn tutorial_byte_slice(input: &str, range: Range<usize>) -> Option<&str> {
  input.get(range)
}

/// Parse a date in `YYYY-MM-DD` form using the tutorial's first buggy parser.
fn parse_date(input: &str) -> Option<Date> {
  if 10 != input.len() {
    return None;
  }
  // !
  if "-" != tutorial_byte_slice(input, 4..5)? || "-" != tutorial_byte_slice(input, 7..8)? {
    return None;
  }

  let year = tutorial_byte_slice(input, 0..4)?;
  let month = tutorial_byte_slice(input, 6..7)?; // !
  let day = tutorial_byte_slice(input, 8..10)?;

  year.parse::<u32>().ok().and_then(|y| {
    month
      .parse::<u32>()
      .ok()
      .and_then(|month_num| day.parse::<u32>().ok().map(|day_num| (y, month_num, day_num)))
  })
}

#[cfg(feature = "strict-test")]
/// Run the crash-resistance property over arbitrary generated strings.
#[allow(
  clippy::single_call_fn,
  reason = "name the tutorial crash-resistance property that the example main runs"
)]
fn doesnt_crash() -> ParserRun {
  ensure_property(
    &"\\PC*",
    "the tutorial parser handles arbitrary strings without panicking",
    |input| {
      let parsed = parse_date(&input);
      Ok((input, parsed))
    },
  )
}

#[cfg(feature = "strict-test")]
fn main() -> Result<(), Box<PredicateFailure<ParserExamples>>> {
  let fixtures = [
    ("2017-06-1", None),
    ("2017-06-170", None),
    ("2017006-17", None),
    ("2017-06017", None),
    ("2017-06-17", Some((2017, 6, 17))),
  ]
  .map(|(input, expected)| (input, parse_date(input), expected));
  ensure_that(
    (fixtures, doesnt_crash()),
    "fixed dates parse as expected and arbitrary input does not crash",
    |subject| subject.0.iter().all(|fixture| fixture.1 == fixture.2) && subject.1.is_ok(),
  )
  .map(drop)
  .map_err(Box::new)
}

#[cfg(not(feature = "strict-test"))]
fn main() {}
