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

use proptest::prelude::*;

/// Parse a date in `YYYY-MM-DD` form using the tutorial's second buggy parser.
fn parse_date(input: &str) -> Option<(u32, u32, u32)> {
    if 10 != input.len() {
        return None;
    }

    // NEW: Ignore non-ASCII strings so we don't need to deal with Unicode.
    if !input.is_ascii() {
        return None;
    }

    if "-" != &input[4..5] || "-" != &input[7..8] {
        return None;
    }

    let year = &input[0..4];
    let month = &input[6..7]; // !
    let day = &input[8..10];

    year.parse::<u32>().ok().and_then(|y| {
        month.parse::<u32>().ok().and_then(|month_num| {
            day.parse::<u32>()
                .ok()
                .map(|day_num| (y, month_num, day_num))
        })
    })
}

// NB We omit #[test] on these functions so that main() can call them.
proptest! {
    fn doesnt_crash(input in "\\PC*") {
        let _parsed = parse_date(&input);
    }

    fn parses_all_valid_dates(input in "[0-9]{4}-[0-9]{2}-[0-9]{2}") {
        prop_assert!(parse_date(&input).is_some());
    }

    fn parses_date_back_to_original(y in 0u32..10_000,
                                    month in 1u32..13, day in 1u32..32) {
        let (y2, m2, d2) = parse_date(
            &format!("{:04}-{:02}-{:02}", y, month, day)).unwrap();
        prop_assert_eq!((y, month, day), (y2, m2, d2));
    }
}

fn main() {
    assert_eq!(None, parse_date("2017-06-1"));
    assert_eq!(None, parse_date("2017-06-170"));
    assert_eq!(None, parse_date("2017006-17"));
    assert_eq!(None, parse_date("2017-06017"));
    assert_eq!(Some((2017, 6, 17)), parse_date("2017-06-17"));

    doesnt_crash();
    parses_all_valid_dates();
    parses_date_back_to_original();
}
