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

use core::char::from_u32;
use core::ops::RangeInclusive;

use rand::Rng;
use rand::RngExt as _;

use crate::num;
use crate::std_facade::Cow;
use crate::std_facade::vec;
#[cfg(test)]
use crate::strategy::CheckStrategySanityOptions;
use crate::strategy::NewTree;
use crate::strategy::Strategy;
use crate::strategy::ValueTree;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::test_runner::TestRunner;

/// An inclusive char range from fst to snd.
type CharRange = RangeInclusive<char>;

/// A default set of characters to consider as "special" during character
/// generation.
///
/// Most of the characters here were chosen specifically because they are
/// difficult to handle in particular contexts.
pub const DEFAULT_SPECIAL_CHARS: &[char] = &[
  // Things to give shell scripts and filesystem logic difficulties
  '/', '\\', '$', '.', '*', '{', '\'', '"', '`', ':', // Characters with special significance in URLs and elsewhere
  '?', '%', '=', '&', '<', // Interesting ASCII control characters
  // NUL, HT,   CR,   LF,   VT      ESC     DEL
  '\x00', '\t', '\r', '\n', '\x0B', '\x1B', '\x7F',
  // ¥ both to test simple Unicode handling and because it has interesting
  // properties on MS Shift-JIS systems.
  '\u{a5}',  // No non-Unicode encoding has both ¥ and Ѩ
  '\u{468}', // In UTF-8, Ⱥ increases in length from 2 to 3 bytes when lowercased
  '\u{23a}', // More Unicode edge-cases: BOM, replacement character, RTL override, and non-BMP
  '\u{FEFF}', '\u{FFFD}', '\u{202E}', '\u{1f574}',
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
pub fn select_char(rnd: &mut impl Rng, special: &[char], preferred: &[CharRange], ranges: &[CharRange]) -> char {
  let (base, offset) = select_range_index(rnd, special, preferred, ranges);
  let fallback = ranges.first().map_or('a', |range| *range.start());
  char_from_range_index(base, offset, fallback)
}

/// Convert a selected `(base, offset)` pair into a scalar value, falling back
/// to a known-valid range endpoint if the numeric point lands in a gap such as
/// the surrogate range.
fn char_from_range_index(base: u32, offset: u32, fallback: char) -> char {
  from_u32(base.saturating_add(offset)).unwrap_or(fallback)
}

/// Chooses a character as `(range base, offset within range)`, applying the
/// same special/preferred/range biases documented on `select_char`.
///
/// Returning the decomposition rather than the finished `char` lets
/// `CharStrategy` pick a convenient shrink target on the same side of the
/// range base.
fn select_range_index(rnd: &mut impl Rng, special: &[char], preferred: &[CharRange], ranges: &[CharRange]) -> (u32, u32) {
  fn in_range(ranges: &[CharRange], ch: char) -> Option<(u32, u32)> {
    ranges
      .iter()
      .find(|range| ch >= *range.start() && ch <= *range.end())
      .map(|range| (u32::from(*range.start()), u32::from(ch).saturating_sub(u32::from(*range.start()))))
  }

  // An empty `ranges` list cannot generate anything; degrade to the
  // canonical ASCII simplification target instead of panicking.
  let Some(first_range) = ranges.first() else {
    return (u32::from('a'), 0);
  };

  if !special.is_empty() && rnd.random() {
    let picked = special
      .get(rnd.random_range(0..special.len()))
      .and_then(|&special_char| in_range(ranges, special_char));
    if let Some(ret) = picked {
      return ret;
    }
  }

  if !preferred.is_empty() && rnd.random() {
    let selected = preferred
      .get(rnd.random_range(0..preferred.len()))
      .and_then(|range| from_u32(rnd.random_range(u32::from(*range.start())..u32::from(*range.end()).saturating_add(1))));
    if let Some(ret) = selected.and_then(|ch| in_range(ranges, ch)) {
      return ret;
    }
  }

  for _ in 0..65_536 {
    let Some(range) = ranges.get(rnd.random_range(0..ranges.len())) else {
      continue;
    };
    if let Some(ch) = from_u32(rnd.random_range(u32::from(*range.start())..u32::from(*range.end()).saturating_add(1))) {
      return (u32::from(*range.start()), u32::from(ch).saturating_sub(u32::from(*range.start())));
    }
  }

  // Give up and return a character we at least know is valid.
  (u32::from(*first_range.start()), 0)
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
  /// Characters given a biased chance of selection (see `select_char`).
  special:   Cow<'a, [char]>,
  /// Ranges sampled preferentially before falling back to `ranges`.
  preferred: Cow<'a, [CharRange]>,
  /// The complete set of ranges any generated character must fall within.
  ranges:    Cow<'a, [CharRange]>,
}

impl<'a> CharStrategy<'a> {
  /// Construct a new `CharStrategy` with the parameters it will pass to the
  /// function underlying `select_char()`.
  ///
  /// All arguments as per `select_char()`.
  #[allow(
    clippy::single_call_fn,
    reason = "assemble a CharStrategy from explicit special, preferred, and full range pools"
  )]
  pub const fn new(special: Cow<'a, [char]>, preferred: Cow<'a, [CharRange]>, ranges: Cow<'a, [CharRange]>) -> Self {
    CharStrategy {
      special,
      preferred,
      ranges,
    }
  }

  /// Same as `CharStrategy::new()` but using `Cow::Borrowed` for all parts.
  pub const fn new_borrowed(special: &'a [char], preferred: &'a [CharRange], ranges: &'a [CharRange]) -> Self {
    CharStrategy::new(Cow::Borrowed(special), Cow::Borrowed(preferred), Cow::Borrowed(ranges))
  }
}

/// The single range spanning every `char`, backing `any()`.
const WHOLE_RANGE: &[CharRange] = &[RangeInclusive::new('\x00', char::MAX)];

/// Creates a `CharStrategy` which picks from literally any character, with the
/// default biases.
#[allow(
  clippy::single_call_fn,
  reason = "the whole-Unicode CharStrategy with default biases that backs char::any"
)]
pub const fn any() -> CharStrategy<'static> {
  CharStrategy {
    special:   Cow::Borrowed(DEFAULT_SPECIAL_CHARS),
    preferred: Cow::Borrowed(DEFAULT_PREFERRED_RANGES),
    ranges:    Cow::Borrowed(WHOLE_RANGE),
  }
}

/// Creates a `CharStrategy` which selects characters within the given
/// endpoints, inclusive, using the default biases.
pub fn range(start: char, end: char) -> CharStrategy<'static> {
  CharStrategy {
    special:   Cow::Borrowed(DEFAULT_SPECIAL_CHARS),
    preferred: Cow::Borrowed(DEFAULT_PREFERRED_RANGES),
    ranges:    Cow::Owned(vec![RangeInclusive::new(start, end)]),
  }
}

/// Creates a `CharStrategy` which selects characters within the given ranges,
/// all inclusive, using the default biases.
#[allow(
  clippy::single_call_fn,
  reason = "a CharStrategy over caller-supplied ranges using the default selection biases"
)]
pub const fn ranges(ranges: Cow<'_, [CharRange]>) -> CharStrategy<'_> {
  CharStrategy {
    special: Cow::Borrowed(DEFAULT_SPECIAL_CHARS),
    preferred: Cow::Borrowed(DEFAULT_PREFERRED_RANGES),
    ranges,
  }
}

/// The `ValueTree` corresponding to `CharStrategy`.
#[derive(Debug, Clone, Copy)]
pub struct CharValueTree {
  /// Binary-search shrinker over the character's `u32` code point.
  value:   num::u32::BinarySearch,
  /// Last valid scalar value produced by `value`.
  current: char,
}

impl Strategy for CharStrategy<'_> {
  type Tree = CharValueTree;
  type Value = char;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    let (base, offset) = select_range_index(runner.rng(), &self.special, &self.preferred, &self.ranges);

    // Select a minimum point more convenient than 0
    let fallback = self.ranges.first().map_or('a', |range| *range.start());
    let selected = char_from_range_index(base, offset, fallback);
    let start = u32::from(selected);
    let latin1_start = u32::from('\u{a1}');
    let bottom = if start >= latin1_start && base < latin1_start {
      latin1_start
    } else if start >= u32::from('a') && base < u32::from('a') {
      u32::from('a')
    } else if start >= u32::from('A') && base < u32::from('A') {
      u32::from('A')
    } else if start >= u32::from('0') && base < u32::from('0') {
      u32::from('0')
    } else if start >= u32::from(' ') && base < u32::from(' ') {
      u32::from(' ')
    } else {
      base
    };

    Ok(CharValueTree {
      value:   num::u32::BinarySearch::new_above(bottom, start),
      current: selected,
    })
  }
}

impl CharValueTree {
  /// Advances the shrinker off any `u32` that is not a valid `char`.
  ///
  /// A simplify/complicate step can land in the surrogate gap; this
  /// complicates until the numeric shrinker reaches a scalar value again.
  fn reposition(&mut self) -> bool {
    for _ in 0..65_536 {
      if let Some(current) = from_u32(self.value.current()) {
        self.current = current;
        return true;
      }
      if !self.value.complicate() {
        return false;
      }
    }
    false
  }
}

impl ValueTree for CharValueTree {
  type Value = char;

  fn current(&self) -> char {
    self.current
  }

  fn simplify(&mut self) -> bool {
    if self.value.simplify() {
      self.reposition()
    } else {
      false
    }
  }

  fn complicate(&mut self) -> bool {
    if self.value.complicate() {
      self.reposition()
    } else {
      false
    }
  }
}

#[cfg(test)]
mod test {
  use core::slice;
  #[cfg(feature = "strict-test")]
  use std::boxed::Box;
  #[cfg(feature = "strict-test")]
  use std::char::from_u32 as std_from_u32;
  #[cfg(feature = "strict-test")]
  use std::cmp::max;
  #[cfg(feature = "strict-test")]
  use std::cmp::min;
  use std::vec::Vec;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  #[cfg(feature = "strict-test")]
  use crate::collection;
  use crate::strategy::trace_shrink_steps;
  #[cfg(feature = "strict-test")]
  use crate::strict::ensure_property;
  #[cfg(feature = "strict-test")]
  use crate::test_runner::PropertyResult;
  use crate::test_runner::Reason;
  #[cfg(feature = "strict-test")]
  use crate::test_runner::test_runner_without_persistence;

  /// Reached character shrinker and every character it produced.
  type CharWalk = (CharValueTree, Vec<char>);
  /// Native character generation outcomes, including the complete shrink walk.
  type CharWalks = Vec<Result<CharWalk, Reason>>;
  /// Native ranges, their strategy, and all observed character shrink walks.
  #[cfg(feature = "strict-test")]
  type RangeWalks = (Vec<(u32, u32)>, CharStrategy<'static>, CharWalks);

  /// Concrete result for a property over native requested character ranges.
  #[cfg(feature = "strict-test")]
  type RangeProperty = PropertyResult<(Vec<(u32, u32)>, Vec<CharRange>), RangeWalks, Box<PredicateFailure<RangeWalks>>>;
  /// Native generated character trees and failures.
  type CharSamples = Vec<NewTree<CharStrategy<'static>>>;

  /// Requested numeric ranges constrain every generated and simplified character.
  #[cfg(feature = "strict-test")]
  #[allow(
    clippy::single_call_fn,
    reason = "range membership is checked independently of property input construction and character sampling"
  )]
  fn range_characters_match(observed: &RangeWalks) -> bool {
    observed.2.iter().all(Result::is_ok)
      && observed
        .2
        .iter()
        .filter_map(|walk| walk.as_ref().ok())
        .flat_map(|reached| &reached.1)
        .all(|ch| {
          let code = u32::from(*ch);
          observed.0.iter().any(|&(lo, hi)| code >= min(lo, hi) && code <= max(lo, hi))
        })
  }

  #[cfg(feature = "strict-test")]
  #[test]
  fn stays_in_range() -> RangeProperty {
    // The non-char pairs are filtered out in the strategy (the legacy
    // test rejected them from inside the test body instead).
    let valid_range_pairs = Strategy::prop_filter_map(
      collection::vec((0..u32::from(char::MAX), 0..u32::from(char::MAX)), 1..5),
      "pair does not describe a char range",
      |pairs| {
        pairs
          .iter()
          .map(|&(lo, hi)| {
            let lower_char = std_from_u32(lo)?;
            let upper_char = std_from_u32(hi)?;
            Some(min(lower_char, upper_char)..=max(lower_char, upper_char))
          })
          .collect::<Option<Vec<CharRange>>>()
          .map(|char_ranges| (pairs, char_ranges))
      },
    );
    ensure_property(
      &valid_range_pairs,
      "generated chars stay within the requested ranges",
      |(input_ranges, char_ranges)| {
        let strategy = ranges(Cow::Owned(char_ranges));
        let mut runner = test_runner_without_persistence();
        let walks = (0..256)
          .map(|_| strategy.new_tree(&mut runner).map(trace_shrink_steps))
          .collect::<CharWalks>();
        ensure_that(
          (input_ranges, strategy, walks),
          "all generated and shrunken characters lie within the input ranges",
          range_characters_match,
        )
        .map_err(Box::new)
      },
    )
  }

  #[test]
  fn applies_desired_bias() -> Result<(), PredicateFailure<CharSamples>> {
    let mut runner = TestRunner::deterministic();
    let samples: Vec<_> = (0..1024).map(|_| any().new_tree(&mut runner)).collect();
    ensure_that(
      samples,
      "ASCII printable characters and the levitating man receive the desired bias",
      |observed| {
        observed.iter().all(Result::is_ok)
          && observed
            .iter()
            .filter(|sample| sample.as_ref().is_ok_and(|tree| (' '..='~').contains(&tree.current())))
            .count()
            >= 256
          && observed
            .iter()
            .any(|sample| sample.as_ref().is_ok_and(|tree| tree.current() == '\u{1f574}'))
      },
    )
    .map(drop)
  }

  #[test]
  fn doesnt_shrink_to_ascii_control() -> Result<(), PredicateFailure<CharWalks>> {
    let mut runner = TestRunner::deterministic();
    let shrink_noncontrol = |tree: CharValueTree| {
      let initial = tree.current();
      if initial <= ' ' {
        (tree, vec![initial])
      } else {
        trace_shrink_steps(tree)
      }
    };
    let keeps_noncontrol = |reached: &CharWalk| {
      reached
        .1
        .first()
        .is_some_and(|first| *first <= ' ' || reached.0.current() >= ' ')
    };
    let is_noncontrol = |first: &char| *first > ' ';
    let starts_noncontrol =
      |walk: &&Result<CharWalk, Reason>| walk.as_ref().is_ok_and(|reached| reached.1.first().is_some_and(is_noncontrol));
    let walks = (0..256)
      .map(|_| any().new_tree(&mut runner))
      .map(|generated| generated.map(shrink_noncontrol))
      .collect::<CharWalks>();
    ensure_that(
      walks,
      "at least 200 non-control characters shrink without entering ASCII controls",
      |observed| {
        observed.iter().all(|walk| walk.as_ref().is_ok_and(keeps_noncontrol)) && observed.iter().filter(starts_noncontrol).count() >= 200
      },
    )
    .map(drop)
  }

  #[test]
  fn test_sanity() -> Result<(), Reason> {
    check_strategy_sanity(
      any(),
      Some(CheckStrategySanityOptions {
        // `simplify()` can itself `complicate()` back to the starting
        // position, so the overly strict complicate-after-simplify check
        // must be disabled.
        strict_complicate_after_simplify: false,
        ..CheckStrategySanityOptions::default()
      }),
    )
  }
  #[test]
  fn select_char_degrades_to_ascii_a_on_empty_ranges() -> Result<(), PredicateFailure<(char, char)>> {
    let mut runner = TestRunner::deterministic();
    let selected = select_char(runner.rng(), &['x'], &[], &[]);
    let allowed_range = 'p'..='t';
    let in_range = select_char(runner.rng(), &[], &[], slice::from_ref(&allowed_range));
    ensure_that(
      (selected, in_range),
      "empty ranges fall back to a while nonempty ranges constrain selection",
      |observed| observed.0 == 'a' && ('p'..='t').contains(&observed.1),
    )
    .map(drop)
  }
}
