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
//! arbitrary strings until proptest finds and shrinks a multi-byte input that
//! panics on a non-char-boundary slice, so this example fails by design.

use proptest::prelude::*;

/// Parse a date in `YYYY-MM-DD` form using the tutorial's first buggy parser.
fn parse_date(input: &str) -> Option<(u32, u32, u32)> {
    if 10 != input.len() {
        return None;
    }
    // !
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
}

fn main() {
    assert_eq!(None, parse_date("2017-06-1"));
    assert_eq!(None, parse_date("2017-06-170"));
    assert_eq!(None, parse_date("2017006-17"));
    assert_eq!(None, parse_date("2017-06017"));
    assert_eq!(Some((2017, 6, 17)), parse_date("2017-06-17"));

    doesnt_crash();
}
