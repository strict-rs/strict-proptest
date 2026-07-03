//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for generating `char` values.
//!
//! Unlike most strategies in Proptest, character generation is by default
//! biased to particular values known to be difficult to handle in various
//! circumstances.
//!
//! The main things of interest are `any()` to generate truly arbitrary
//! characters, and `range()` and `ranges()` to select characters from
//! inclusive ranges.

use crate::std_facade::Cow;
use core::ops::RangeInclusive;

use rand::{Rng, RngExt};

use crate::num;
use crate::strategy::*;
use crate::test_runner::*;

/// An inclusive char range from fst to snd.
type CharRange = RangeInclusive<char>;

/// A default set of characters to consider as "special" during character
/// generation.
///
/// Most of the characters here were chosen specifically because they are
/// difficult to handle in particular contexts.
pub const DEFAULT_SPECIAL_CHARS: &[char] = &[
    // Things to give shell scripts and filesystem logic difficulties
    '/', '\\', '$', '.', '*', '{', '\'', '"', '`', ':',
    // Characters with special significance in URLs and elsewhere
    '?', '%', '=', '&', '<',
    // Interesting ASCII control characters
    // NUL, HT,   CR,   LF,   VT      ESC     DEL
    '\x00', '\t', '\r', '\n', '\x0B', '\x1B', '\x7F',
    // ¥ both to test simple Unicode handling and because it has interesting
    // properties on MS Shift-JIS systems.
    '¥', // No non-Unicode encoding has both ¥ and Ѩ
    'Ѩ',
    // In UTF-8, Ⱥ increases in length from 2 to 3 bytes when lowercased
    'Ⱥ',
    // More Unicode edge-cases: BOM, replacement character, RTL override, and non-BMP
    '\u{FEFF}', '\u{FFFD}', '\u{202E}', '🕴',
];

/// A default sequence of ranges used preferentially when generating random
/// characters.
pub const DEFAULT_PREFERRED_RANGES: &[CharRange] = &[
    // ASCII printable
    ' '..='~',
    ' '..='~',
    ' '..='~',
    ' '..='~',
    ' '..='~',
    // Latin-1
    '\u{0040}'..='\u{00ff}',
];

/// Selects a random character the way `CharStrategy` does.
///
/// If `special` is non-empty, there is a 50% chance that a character from this
/// array is chosen randomly, and will be returned if that character falls
/// within `ranges`.
///
/// If `preferred` is non-empty, there is a 50% chance that any generation
/// which gets past the `special` step picks a random element from this list,
/// then a random character from within that range (both endpoints inclusive).
/// That character will be returned if it falls within `ranges`.
///
/// In all other cases, an element is picked randomly from `ranges` and a
/// random character within the range (both endpoints inclusive) is chosen and
/// returned.
///
/// Notice that in all cases, `ranges` completely defines the set of characters
/// that can possibly be defined.
///
/// It is legal for ranges in all cases to contain non-characters.
///
/// Both `preferred` and `ranges` bias selection towards characters in smaller
/// ranges. This is deliberate. `preferred` is usually tuned to select
/// particular characters anyway. `ranges` is usually derived from some
/// external property, and the fact that a range is small often means it is
/// more interesting.
pub fn select_char(
    rnd: &mut impl Rng,
    special: &[char],
    preferred: &[CharRange],
    ranges: &[CharRange],
) -> char {
    let (base, offset) = select_range_index(rnd, special, preferred, ranges);
    ::core::char::from_u32(base + offset).expect("bad character selected")
}

fn select_range_index(
    rnd: &mut impl Rng,
    special: &[char],
    preferred: &[CharRange],
    ranges: &[CharRange],
) -> (u32, u32) {
    fn in_range(ranges: &[CharRange], ch: char) -> Option<(u32, u32)> {
        ranges
            .iter()
            .find(|r| ch >= *r.start() && ch <= *r.end())
            .map(|r| (*r.start() as u32, ch as u32 - *r.start() as u32))
    }

    // An empty `ranges` list cannot generate anything; degrade to the
    // canonical ASCII simplification target instead of panicking.
    let Some(first_range) = ranges.first() else {
        return ('a' as u32, 0);
    };

    if !special.is_empty() && rnd.random() {
        let picked = special
            .get(rnd.random_range(0..special.len()))
            .and_then(|&s| in_range(ranges, s));
        if let Some(ret) = picked {
            return ret;
        }
    }

    if !preferred.is_empty() && rnd.random() {
        let selected = preferred
            .get(rnd.random_range(0..preferred.len()))
            .and_then(|range| {
                ::core::char::from_u32(rnd.random_range(
                    *range.start() as u32..*range.end() as u32 + 1,
                ))
            });
        if let Some(ret) = selected.and_then(|ch| in_range(ranges, ch)) {
            return ret;
        }
    }

    for _ in 0..65_536 {
        let Some(range) = ranges.get(rnd.random_range(0..ranges.len())) else {
            continue;
        };
        if let Some(ch) = ::core::char::from_u32(
            rnd.random_range(*range.start() as u32..*range.end() as u32 + 1),
        ) {
            return (*range.start() as u32, ch as u32 - *range.start() as u32);
        }
    }

    // Give up and return a character we at least know is valid.
    (*first_range.start() as u32, 0)
}

/// Strategy for generating `char`s.
///
/// Character selection is more sophisticated than integer selection. Naïve
/// selection (particularly in the larger context of generating strings) would
/// result in starting inputs like `ꂡ螧轎ቶᢹ糦狥芹ᘆ㶏曊ᒀ踔虙ჲ` and "simplified"
/// inputs consisting mostly of control characters. It also has difficulty
/// locating edge cases, since the vast majority of code points (such as the
/// enormous CJK regions) don't cause problems for anything with even basic
/// Unicode support.
///
/// Instead, character selection is always based on explicit ranges, and is
/// designed to bias to specifically chosen characters and character ranges to
/// produce inputs that are both more useful and easier for humans to
/// understand. There are also hard-wired simplification targets based on ASCII
/// instead of simply simplifying towards NUL to avoid problematic inputs being
/// reduced to a bunch of NUL characters.
///
/// Shrinking never crosses ranges. If you have a complex range like `[A-Za-z]`
/// and the starting point `x` is chosen, it will not shrink to the first `A-Z`
/// group, but rather simply to `a`.
///
/// The usual way to get instances of this class is with the module-level `ANY`
/// constant or `range` function. Directly constructing a `CharStrategy` is
/// only necessary for complex ranges or to override the default biases.
#[derive(Debug, Clone)]
#[must_use = "strategies do nothing unless used"]
pub struct CharStrategy<'a> {
    special: Cow<'a, [char]>,
    preferred: Cow<'a, [CharRange]>,
    ranges: Cow<'a, [CharRange]>,
}

impl<'a> CharStrategy<'a> {
    /// Construct a new `CharStrategy` with the parameters it will pass to the
    /// function underlying `select_char()`.
    ///
    /// All arguments as per `select_char()`.
    pub fn new(
        special: Cow<'a, [char]>,
        preferred: Cow<'a, [CharRange]>,
        ranges: Cow<'a, [CharRange]>,
    ) -> Self {
        CharStrategy {
            special,
            preferred,
            ranges,
        }
    }

    /// Same as `CharStrategy::new()` but using `Cow::Borrowed` for all parts.
    pub fn new_borrowed(
        special: &'a [char],
        preferred: &'a [CharRange],
        ranges: &'a [CharRange],
    ) -> Self {
        CharStrategy::new(
            Cow::Borrowed(special),
            Cow::Borrowed(preferred),
            Cow::Borrowed(ranges),
        )
    }
}

const WHOLE_RANGE: &[CharRange] = &[RangeInclusive::new('\x00', char::MAX)];

/// Creates a `CharStrategy` which picks from literally any character, with the
/// default biases.
pub fn any() -> CharStrategy<'static> {
    CharStrategy {
        special: Cow::Borrowed(DEFAULT_SPECIAL_CHARS),
        preferred: Cow::Borrowed(DEFAULT_PREFERRED_RANGES),
        ranges: Cow::Borrowed(WHOLE_RANGE),
    }
}

/// Creates a `CharStrategy` which selects characters within the given
/// endpoints, inclusive, using the default biases.
pub fn range(start: char, end: char) -> CharStrategy<'static> {
    CharStrategy {
        special: Cow::Borrowed(DEFAULT_SPECIAL_CHARS),
        preferred: Cow::Borrowed(DEFAULT_PREFERRED_RANGES),
        ranges: Cow::Owned(vec![RangeInclusive::new(start, end)]),
    }
}

/// Creates a `CharStrategy` which selects characters within the given ranges,
/// all inclusive, using the default biases.
pub fn ranges(ranges: Cow<[CharRange]>) -> CharStrategy {
    CharStrategy {
        special: Cow::Borrowed(DEFAULT_SPECIAL_CHARS),
        preferred: Cow::Borrowed(DEFAULT_PREFERRED_RANGES),
        ranges,
    }
}

/// The `ValueTree` corresponding to `CharStrategy`.
#[derive(Debug, Clone, Copy)]
pub struct CharValueTree {
    value: num::u32::BinarySearch,
}

impl<'a> Strategy for CharStrategy<'a> {
    type Tree = CharValueTree;
    type Value = char;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        let (base, offset) = select_range_index(
            runner.rng(),
            &self.special,
            &self.preferred,
            &self.ranges,
        );

        // Select a minimum point more convenient than 0
        let start = base + offset;
        let bottom = if start >= '¡' as u32 && base < '¡' as u32 {
            '¡' as u32
        } else if start >= 'a' as u32 && base < 'a' as u32 {
            'a' as u32
        } else if start >= 'A' as u32 && base < 'A' as u32 {
            'A' as u32
        } else if start >= '0' as u32 && base < '0' as u32 {
            '0' as u32
        } else if start >= ' ' as u32 && base < ' ' as u32 {
            ' ' as u32
        } else {
            base
        };

        Ok(CharValueTree {
            value: num::u32::BinarySearch::new_above(bottom, start),
        })
    }
}

impl CharValueTree {
    fn reposition(&mut self) {
        while ::core::char::from_u32(self.value.current()).is_none() {
            if !self.value.complicate() {
                panic!("Converged to non-char value");
            }
        }
    }
}

impl ValueTree for CharValueTree {
    type Value = char;

    fn current(&self) -> char {
        ::core::char::from_u32(self.value.current())
            .expect("Generated non-char value")
    }

    fn simplify(&mut self) -> bool {
        if self.value.simplify() {
            self.reposition();
            true
        } else {
            false
        }
    }

    fn complicate(&mut self) -> bool {
        if self.value.complicate() {
            self.reposition();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use std::cmp::{max, min};
    use std::vec::Vec;

    use strict_test_support::{TestFailure, ensure, ensure_some};

    use super::*;
    use crate::collection;

    #[test]
    fn stays_in_range() -> Result<(), TestFailure> {
        // The non-char pairs are filtered out in the strategy (the legacy
        // test rejected them from inside the test body instead).
        let valid_range_pairs = Strategy::prop_filter_map(
            collection::vec((0..char::MAX as u32, 0..char::MAX as u32), 1..5),
            "pair does not describe a char range",
            |pairs| {
                pairs
                    .iter()
                    .map(|&(lo, hi)| {
                        ::std::char::from_u32(lo).and_then(|lo| {
                            ::std::char::from_u32(hi)
                                .map(|hi| min(lo, hi)..=max(lo, hi))
                        })
                    })
                    .collect::<Option<Vec<CharRange>>>()
                    .map(|char_ranges| (pairs, char_ranges))
            },
        );
        crate::strict::ensure_property(
            &valid_range_pairs,
            "generated chars stay within the requested ranges",
            |(input_ranges, char_ranges)| {
                let input = ranges(Cow::Owned(char_ranges));
                let mut runner = TestRunner::default();
                for _ in 0..256 {
                    let mut value = ensure_some(
                        input.new_tree(&mut runner).ok(),
                        "char strategy generates a value tree",
                    )?;
                    loop {
                        let ch = value.current() as u32;
                        ensure(
                            input_ranges.iter().any(|&(lo, hi)| {
                                ch >= min(lo, hi) && ch <= max(lo, hi)
                            }),
                            "generated char lies in one of the input ranges",
                        )?;

                        if !value.simplify() {
                            break;
                        }
                    }
                }
                Ok(())
            },
        )
    }

    #[test]
    fn applies_desired_bias() -> Result<(), TestFailure> {
        let mut men_in_business_suits_levitating = 0;
        let mut ascii_printable = 0;
        let mut runner = TestRunner::deterministic();

        for _ in 0..1024 {
            let ch = ensure_some(
                any().new_tree(&mut runner).ok(),
                "char strategy generates a value tree",
            )?
            .current();
            if '🕴' == ch {
                men_in_business_suits_levitating += 1;
            } else if (' '..='~').contains(&ch) {
                ascii_printable += 1;
            }
        }

        ensure(
            ascii_printable >= 256,
            "the bias favors ASCII printable chars",
        )?;
        ensure(
            men_in_business_suits_levitating >= 1,
            "the special-char bias emits the levitating man",
        )
    }

    #[test]
    fn doesnt_shrink_to_ascii_control() -> Result<(), TestFailure> {
        let mut accepted = 0;
        let mut runner = TestRunner::deterministic();

        for _ in 0..256 {
            let mut value = ensure_some(
                any().new_tree(&mut runner).ok(),
                "char strategy generates a value tree",
            )?;

            if value.current() <= ' ' {
                continue;
            }

            while value.simplify() {}

            ensure(
                value.current() >= ' ',
                "shrinking never lands on an ASCII control char",
            )?;
            accepted += 1;
        }

        ensure(accepted >= 200, "enough shrink runs were accepted")
    }

    #[test]
    fn test_sanity() {
        check_strategy_sanity(
            any(),
            Some(CheckStrategySanityOptions {
                // `simplify()` can itself `complicate()` back to the starting
                // position, so the overly strict complicate-after-simplify check
                // must be disabled.
                strict_complicate_after_simplify: false,
                ..CheckStrategySanityOptions::default()
            }),
        );
    }
    #[test]
    fn select_char_degrades_to_ascii_a_on_empty_ranges()
    -> Result<(), TestFailure> {
        let mut runner = crate::test_runner::TestRunner::deterministic();
        let selected = select_char(runner.rng(), &['x'], &[], &[]);
        ensure(
            'a' == selected,
            "an empty range list degrades to the canonical 'a' target",
        )?;
        let in_range = select_char(runner.rng(), &[], &[], &['p'..='t']);
        ensure(
            in_range >= 'p' && in_range <= 't',
            "a non-empty range list still selects from the ranges",
        )
    }
}
