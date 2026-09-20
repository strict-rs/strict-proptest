//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Tutorial date-parser property test at its second stage.
//!
//! Backs the second half of the Proptest Book's `getting-started` chapter.
//! An `is_ascii` guard fixes the crash from `dateparser_v1`, but a lingering
//! one-digit month slice remains, and the round-trip oracle property
//! `parses_date_back_to_original` catches it: proptest shrinks to the minimal
//! failing `y = 0, m = 10, d = 1`. Fails by design; the bug is marked `// !`.

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
/// Source components, their rendered input, and the parser's complete response.
type RoundTrip = (Date, String, Option<Date>);
#[cfg(feature = "strict-test")]
/// A fixed input, its observed response, and its expected response.
type Fixture = (&'static str, Option<Date>, Option<Date>);
#[cfg(feature = "strict-test")]
/// Native observations from each independent tutorial property.
type ExampleOutcome = (
  [Fixture; 5],
  PropertyResult<String, ParsedDate, Infallible>,
  PropertyResult<String, ParsedDate, PredicateFailure<ParsedDate>>,
  PropertyResult<Date, RoundTrip, PredicateFailure<RoundTrip>>,
);

/// Return one of the parser's fixed ASCII byte ranges.
fn ascii_slice(input: &str, range: Range<usize>) -> Option<&str> {
  input.get(range)
}

/// Parse a date in `YYYY-MM-DD` form using the tutorial's second buggy parser.
fn parse_date(input: &str) -> Option<Date> {
  if 10 != input.len() {
    return None;
  }

  // NEW: Ignore non-ASCII strings so we don't need to deal with Unicode.
  if !input.is_ascii() {
    return None;
  }

  if "-" != ascii_slice(input, 4..5)? || "-" != ascii_slice(input, 7..8)? {
    return None;
  }

  let year = ascii_slice(input, 0..4)?;
  let month = ascii_slice(input, 6..7)?; // !
  let day = ascii_slice(input, 8..10)?;

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
fn doesnt_crash() -> PropertyResult<String, ParsedDate, Infallible> {
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
/// Run the property that every digit-shaped date is accepted.
#[allow(
  clippy::single_call_fn,
  reason = "name the tutorial digit-shaped-date acceptance property that the example main runs"
)]
fn parses_all_valid_dates() -> PropertyResult<String, ParsedDate, PredicateFailure<ParsedDate>> {
  ensure_property(&"[0-9]{4}-[0-9]{2}-[0-9]{2}", "all digit-shaped dates parse", |input| {
    let parsed = parse_date(&input);
    ensure_that((input, parsed), "the parser accepts the generated date", |observed| {
      observed.1.is_some()
    })
  })
}

#[cfg(feature = "strict-test")]
/// Run the round-trip property from generated date components.
#[allow(
  clippy::single_call_fn,
  reason = "name the tutorial round-trip oracle property that the example main runs"
)]
fn parses_date_back_to_original() -> PropertyResult<Date, RoundTrip, PredicateFailure<RoundTrip>> {
  ensure_property(
    &(0_u32..10_000, 1_u32..13, 1_u32..32),
    "formatted dates parse back to their source components",
    |(y, month, day)| {
      let formatted = format!("{y:04}-{month:02}-{day:02}");
      let parsed = parse_date(&formatted);
      ensure_that(
        ((y, month, day), formatted, parsed),
        "the parsed date matches the generated components",
        |observed| observed.2 == Some(observed.0),
      )
    },
  )
}

#[cfg(feature = "strict-test")]
fn main() -> Result<(), Box<PredicateFailure<ExampleOutcome>>> {
  let fixtures = [
    ("2017-06-1", None),
    ("2017-06-170", None),
    ("2017006-17", None),
    ("2017-06017", None),
    ("2017-06-17", Some((2017, 6, 17))),
  ]
  .map(|(input, expected)| (input, parse_date(input), expected));
  ensure_that(
    (fixtures, doesnt_crash(), parses_all_valid_dates(), parses_date_back_to_original()),
    "fixed inputs, arbitrary input, digit-shaped dates, and round trips satisfy their contracts",
    |observed| observed.0.iter().all(|fixture| fixture.1 == fixture.2) && observed.1.is_ok() && observed.2.is_ok() && observed.3.is_ok(),
  )
  .map(drop)
  .map_err(Box::new)
}

#[cfg(not(feature = "strict-test"))]
fn main() {}
