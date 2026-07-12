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

use core::ops::Range;

#[cfg(feature = "strict-test")]
use proptest::strict::TestFailure;
#[cfg(feature = "strict-test")]
use proptest::strict::ensure_property;
#[cfg(feature = "strict-test")]
use strict_test_support::ensure;
#[cfg(feature = "strict-test")]
use strict_test_support::ensure_some;

/// Return one of the parser's fixed ASCII byte ranges.
fn ascii_slice(input: &str, range: Range<usize>) -> Option<&str> {
  input.get(range)
}

/// Parse a date in `YYYY-MM-DD` form using the tutorial's second buggy parser.
fn parse_date(input: &str) -> Option<(u32, u32, u32)> {
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
/// Run the property that every digit-shaped date is accepted.
#[allow(
  clippy::single_call_fn,
  reason = "name the tutorial digit-shaped-date acceptance property that the example main runs"
)]
fn parses_all_valid_dates() -> Result<(), TestFailure> {
  ensure_property(&"[0-9]{4}-[0-9]{2}-[0-9]{2}", "all digit-shaped dates parse", |input| {
    ensure(parse_date(&input).is_some(), "the parser accepts the generated date")
  })
}

#[cfg(feature = "strict-test")]
/// Run the round-trip property from generated date components.
#[allow(
  clippy::single_call_fn,
  reason = "name the tutorial round-trip oracle property that the example main runs"
)]
fn parses_date_back_to_original() -> Result<(), TestFailure> {
  ensure_property(
    &(0_u32..10_000, 1_u32..13, 1_u32..32),
    "formatted dates parse back to their source components",
    |(y, month, day)| {
      let formatted = format!("{y:04}-{month:02}-{day:02}");
      let (parsed_year, parsed_month, parsed_day) = ensure_some(parse_date(&formatted), "the formatted date parses")?;
      ensure(
        (y, month, day) == (parsed_year, parsed_month, parsed_day),
        "the parsed date matches the generated components",
      )
    },
  )
}

#[cfg(feature = "strict-test")]
fn main() -> Result<(), TestFailure> {
  ensure(parse_date("2017-06-1").is_none(), "short dates are rejected")?;
  ensure(parse_date("2017-06-170").is_none(), "long dates are rejected")?;
  ensure(parse_date("2017006-17").is_none(), "missing separators are rejected")?;
  ensure(parse_date("2017-06017").is_none(), "misplaced separators are rejected")?;
  ensure(Some((2017, 6, 17)) == parse_date("2017-06-17"), "well-formed dates are parsed")?;

  doesnt_crash()?;
  parses_all_valid_dates()?;
  parses_date_back_to_original()
}

#[cfg(not(feature = "strict-test"))]
fn main() {}
