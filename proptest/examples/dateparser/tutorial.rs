//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Field decoding and typed observations shared by the date-parser tutorial stages.

use core::convert::Infallible;

use proptest::strict::ensure_property;
use proptest::test_runner::PropertyResult;

/// The parser's native date components.
pub(crate) type Date = (u32, u32, u32);
/// Generated input and the parser's complete response.
pub(crate) type ParsedDate = (String, Option<Date>);
/// Every completed crash-resistance evaluation remains available.
pub(crate) type ParserRun = PropertyResult<String, ParsedDate, Infallible>;
/// Fixed inputs, their observed responses, and their expected responses.
pub(crate) type Fixtures = [(&'static str, Option<Date>, Option<Date>); 5];

/// Decode fixed byte ranges after the caller checks the input length.
///
/// Every range remains fallible for non-ASCII input. The single-digit month
/// deliberately preserves the bug exposed by the second stage's oracle.
#[allow(
  clippy::single_call_fn,
  reason = "share the field decoder across separately compiled tutorial stages while keeping admission checks local"
)]
pub(crate) fn parse_fields(input: &str) -> Option<Date> {
  if input.get(4..5)? != "-" || input.get(7..8)? != "-" {
    return None;
  }

  let year = input.get(0..4)?;
  let month = input.get(6..7)?; // ! The correct month range is 5..7.
  let day = input.get(8..10)?;
  Some((year.parse().ok()?, month.parse().ok()?, day.parse().ok()?))
}

/// Observe the fixed parser examples without discarding their inputs or results.
#[allow(
  clippy::single_call_fn,
  reason = "share the complete fixed-case observations across separately compiled date-parser examples"
)]
pub(crate) fn fixed_examples(parser: fn(&str) -> Option<Date>) -> Fixtures {
  [
    ("2017-06-1", None),
    ("2017-06-170", None),
    ("2017006-17", None),
    ("2017-06017", None),
    ("2017-06-17", Some((2017, 6, 17))),
  ]
  .map(|(input, expected)| (input, parser(input), expected))
}

/// Run the crash-resistance property and retain every generated input and response.
#[allow(
  clippy::single_call_fn,
  reason = "name the shared crash-resistance property that each separately compiled example main runs"
)]
pub(crate) fn doesnt_crash(parser: fn(&str) -> Option<Date>) -> ParserRun {
  ensure_property(
    &"\\PC*",
    "the tutorial parser handles arbitrary strings without panicking",
    |input| {
      let parsed = parser(&input);
      Ok((input, parsed))
    },
  )
}

#[cfg(test)]
mod tests {
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::Date;
  use super::parse_fields;

  /// Complete inputs, parser responses, and expected responses for the field contract.
  type FieldCases = Vec<(&'static str, Option<Date>, Option<Date>)>;

  /// Observe valid fields, the seeded month bug, and each rejection boundary.
  #[test]
  fn decodes_fields_without_panicking() -> Result<(), PredicateFailure<FieldCases>> {
    let observations: FieldCases = [
      ("2017-06-17", Some((2017, 6, 17))),
      ("0000-10-01", Some((0, 0, 1))),
      ("2017/06-17", None),
      ("2017-06/17", None),
      ("201x-06-17", None),
      ("2017-0x-17", None),
      ("2017-06-1x", None),
      ("aA\u{0bd7}0\u{3300}0", None),
      ("2017-06-1", None),
    ]
    .into_iter()
    .map(|(input, expected)| (input, parse_fields(input), expected))
    .collect();
    ensure_that(
      observations,
      "date fields preserve their values or reject invalid byte ranges and digits",
      |cases| cases.iter().all(|case| case.1 == case.2),
    )
    .map(drop)
  }
}
