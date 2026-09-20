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
/// Fixed examples and the complete randomized parser run.
type ParserExamples = (tutorial::Fixtures, tutorial::ParserRun);

#[cfg(feature = "strict-test")]
/// Parse a date in `YYYY-MM-DD` form using the tutorial's first buggy parser.
fn parse_date(input: &str) -> Option<Date> {
  if 10 != input.len() {
    return None;
  }
  tutorial::parse_fields(input)
}

#[cfg(feature = "strict-test")]
fn main() -> Result<(), Box<PredicateFailure<ParserExamples>>> {
  ensure_that(
    (tutorial::fixed_examples(parse_date), tutorial::doesnt_crash(parse_date)),
    "fixed dates parse as expected and arbitrary input does not crash",
    |subject| subject.0.iter().all(|fixture| fixture.1 == fixture.2) && subject.1.is_ok(),
  )
  .map(drop)
  .map_err(Box::new)
}

#[cfg(not(feature = "strict-test"))]
fn main() {}
