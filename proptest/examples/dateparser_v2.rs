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
//! An `is_ascii` guard rejects non-ASCII input before decoding, but a lingering
//! one-digit month slice remains, and the round-trip oracle property
//! `parses_date_back_to_original` catches it: proptest shrinks to the minimal
//! failing `y = 0, m = 10, d = 1`. Fails by design; the bug is marked `// !`.

#[cfg(feature = "strict-test")]
use proptest::strict::ensure_property;
#[cfg(feature = "strict-test")]
use proptest::test_runner::PropertyResult;
#[cfg(feature = "strict-test")]
use strict_test_support::PredicateFailure;
#[cfg(feature = "strict-test")]
use strict_test_support::ensure_that;

#[cfg(feature = "strict-test")]
/// Shared field decoding and complete property observations.
pub mod dateparser {
  pub mod tutorial;
}
#[cfg(feature = "strict-test")]
use dateparser::tutorial;
#[cfg(feature = "strict-test")]
use tutorial::Date;
#[cfg(feature = "strict-test")]
use tutorial::ParsedDate;

#[cfg(feature = "strict-test")]
/// Source components, their rendered input, and the parser's complete response.
type RoundTrip = (Date, String, Option<Date>);
#[cfg(feature = "strict-test")]
/// Native observations from each independent tutorial property.
type ExampleOutcome = (
  tutorial::Fixtures,
  tutorial::ParserRun,
  PropertyResult<String, ParsedDate, PredicateFailure<ParsedDate>>,
  PropertyResult<Date, RoundTrip, PredicateFailure<RoundTrip>>,
);

#[cfg(feature = "strict-test")]
/// Parse a date in `YYYY-MM-DD` form using the tutorial's second buggy parser.
fn parse_date(input: &str) -> Option<Date> {
  if 10 != input.len() {
    return None;
  }

  // NEW: Ignore non-ASCII strings so we don't need to deal with Unicode.
  if !input.is_ascii() {
    return None;
  }

  tutorial::parse_fields(input)
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
  ensure_that(
    (
      tutorial::fixed_examples(parse_date),
      tutorial::doesnt_crash(parse_date),
      parses_all_valid_dates(),
      parses_date_back_to_original(),
    ),
    "fixed inputs, arbitrary input, digit-shaped dates, and round trips satisfy their contracts",
    |observed| observed.0.iter().all(|fixture| fixture.1 == fixture.2) && observed.1.is_ok() && observed.2.is_ok() && observed.3.is_ok(),
  )
  .map(drop)
  .map_err(Box::new)
}

#[cfg(not(feature = "strict-test"))]
fn main() {}
