//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for generating strings and byte strings from regular
//! expressions.

use core::fmt;
use core::marker::PhantomData;
use core::mem::take;
use core::ops::RangeInclusive;
use std::error::Error as StdError;

use regex_syntax::Error as ParseError;
use regex_syntax::ParserBuilder;
use regex_syntax::hir::Hir;
use regex_syntax::hir::HirKind;
use regex_syntax::hir::Repetition;
use regex_syntax::hir::{
  self,
};

use crate::bool;
use crate::char;
use crate::collection::SizeRange;
use crate::collection::size_range;
use crate::collection::vec;
use crate::std_facade::Box;
use crate::std_facade::Cow;
use crate::std_facade::String;
use crate::std_facade::ToOwned as _;
use crate::std_facade::Vec;
use crate::std_facade::format;
use crate::std_facade::vec;
use crate::strategy::Just;
use crate::strategy::NewTree;
use crate::strategy::SBoxedStrategy;
use crate::strategy::Strategy;
use crate::strategy::Union;
use crate::strategy::UnionBuildError;
use crate::strategy::ValueTree;
use crate::test_runner::Reason;
use crate::test_runner::TestRunner;

/// Wraps the regex that forms the `Strategy` for `String` so that a sensible
/// `Default` can be given. The default is a string of non-control characters.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StringParam(&'static str);

impl From<StringParam> for &'static str {
  fn from(x: StringParam) -> Self {
    x.0
  }
}

impl From<&'static str> for StringParam {
  fn from(x: &'static str) -> Self {
    Self(x)
  }
}

impl Default for StringParam {
  fn default() -> Self {
    Self("\\PC*")
  }
}

/// Error returned when preparing a regular expression for string or byte
/// generation.
#[derive(Debug)]
pub enum RegexStrategyError {
  /// The string passed as the regex was not syntactically valid.
  RegexSyntax(Box<ParseError>),
  /// The regex was syntactically valid, but contains elements not
  /// supported by proptest.
  UnsupportedRegex(&'static str),
}

impl fmt::Display for RegexStrategyError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match *self {
      Self::RegexSyntax(ref err) => write!(f, "{err}"),
      Self::UnsupportedRegex(message) => write!(f, "{message}"),
    }
  }
}

impl StdError for RegexStrategyError {
  fn source(&self) -> Option<&(dyn StdError + 'static)> {
    match *self {
      Self::RegexSyntax(ref err) => Some(err.as_ref()),
      Self::UnsupportedRegex(_) => None,
    }
  }
}

impl From<ParseError> for RegexStrategyError {
  fn from(err: ParseError) -> Self {
    Self::RegexSyntax(Box::new(err))
  }
}

/// Internal counterpart to the public [`RegexStrategyError`], used while
/// walking a regex HIR.
///
/// It boxes the large `regex_syntax` parse error so intermediate `Result`s
/// stay small, then converts into [`RegexStrategyError`] at the module
/// boundary.
#[derive(Debug)]
enum InternalError {
  /// The regex was not syntactically valid; wraps the boxed parse error.
  RegexSyntax(Box<ParseError>),
  /// The regex parsed but uses a construct proptest cannot generate.
  UnsupportedRegex(&'static str),
}

impl From<ParseError> for InternalError {
  fn from(err: ParseError) -> Self {
    Self::RegexSyntax(Box::new(err))
  }
}

impl From<InternalError> for RegexStrategyError {
  fn from(err: InternalError) -> Self {
    match err {
      InternalError::RegexSyntax(regex_error) => Self::RegexSyntax(regex_error),
      InternalError::UnsupportedRegex(message) => Self::UnsupportedRegex(message),
    }
  }
}

opaque_strategy_wrapper! {
    /// Strategy which generates values (i.e., `String` or `Vec<u8>`) matching
    /// a regular expression.
    ///
    /// Created by various functions in this module.
    #[derive(Debug)]
    pub struct RegexGeneratorStrategy[<T>][where T : fmt::Debug]
        (SBoxedStrategy<T>) -> RegexGeneratorValueTree<T>;
    /// `ValueTree` corresponding to `RegexGeneratorStrategy`.
    pub struct RegexGeneratorValueTree[<T>][where T : fmt::Debug]
        (Box<dyn ValueTree<Value = T>>) -> T;
}

impl<T: fmt::Debug> fmt::Debug for RegexGeneratorValueTree<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // The wrapped `Box<dyn ValueTree>` has no `Debug`, so omit the field.
    f.debug_struct("RegexGeneratorValueTree").finish_non_exhaustive()
  }
}

/// A regex strategy placeholder for `StrategyFromRegex` inputs that cannot be
/// parsed into a concrete generator.
#[derive(Clone, Debug)]
struct RegexErrorStrategy<T> {
  /// The generation error reported whenever the strategy is used.
  reason: Reason,
  /// Retain the generated value type without imposing ownership bounds on it.
  marker: PhantomData<fn() -> T>,
}

impl<T: fmt::Debug> Strategy for RegexErrorStrategy<T> {
  type Tree = Box<dyn ValueTree<Value = T>>;
  type Value = T;

  fn new_tree(&self, _runner: &mut TestRunner) -> NewTree<Self> {
    Err(self.reason.clone())
  }
}

/// Convert a regex parse/build error into the generation failure vocabulary.
fn regex_parse_reason(regex: &str, error: &RegexStrategyError) -> Reason {
  format!("invalid regex strategy `{regex}`: {error}").into()
}

/// Build a `RegexGeneratorStrategy` that reports `error` at generation time.
fn regex_error_strategy<T: fmt::Debug + 'static>(regex: &str, error: &RegexStrategyError) -> RegexGeneratorStrategy<T> {
  RegexGeneratorStrategy(
    RegexErrorStrategy {
      reason: regex_parse_reason(regex, error),
      marker: PhantomData,
    }
    .sboxed(),
  )
}

impl Strategy for str {
  type Tree = RegexGeneratorValueTree<String>;
  type Value = String;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    string_regex(self)
      .map_err(|error| regex_parse_reason(self, &error))?
      .new_tree(runner)
  }
}

/// Result of building a public regex strategy: the strategy, or a public
/// `RegexStrategyError` explaining why the regex was rejected.
type ParseResult<T> = Result<RegexGeneratorStrategy<T>, RegexStrategyError>;
/// Like `ParseResult`, but carrying the crate-internal `InternalError` used
/// while the regex HIR is being walked.
type InternalParseResult<T> = Result<RegexGeneratorStrategy<T>, InternalError>;

#[doc(hidden)]
/// A type which knows how to produce a `Strategy` from a regular expression
/// generating the type.
///
/// This trait exists for the benefit of `#[proptest(regex = "...")]`.
/// It is semver exempt, so use at your own risk.
/// If you found a use for the trait beyond `Vec<u8>` and `String`,
/// please file an issue at https://github.com/proptest-rs/proptest.
pub trait StrategyFromRegex: Sized + fmt::Debug {
  type Strategy: Strategy<Value = Self>;

  /// Produce a strategy for `Self` from the `regex`.
  fn from_regex(regex: &str) -> Self::Strategy;
}

impl StrategyFromRegex for String {
  type Strategy = RegexGeneratorStrategy<Self>;

  fn from_regex(regex: &str) -> Self::Strategy {
    string_regex(regex).unwrap_or_else(|error| regex_error_strategy(regex, &error))
  }
}

impl StrategyFromRegex for Vec<u8> {
  type Strategy = RegexGeneratorStrategy<Self>;

  fn from_regex(regex: &str) -> Self::Strategy {
    bytes_regex(regex).unwrap_or_else(|error| regex_error_strategy(regex, &error))
  }
}

/// Creates a strategy which generates strings matching the given regular
/// expression.
///
/// If you don't need error handling and aren't limited by setup time, it is
/// also possible to directly use a `&str` as a strategy with the same effect.
///
/// # Errors
///
/// Returns `RegexStrategyError::RegexSyntax` if `regex` is not valid regex syntax, or
/// `RegexStrategyError::UnsupportedRegex` if it parses but uses a construct proptest
/// cannot generate (for example an anchor or look-around).
pub fn string_regex(regex: &str) -> ParseResult<String> {
  string_regex_inner(regex).map_err(RegexStrategyError::from)
}

/// Parse `regex` into an HIR and build the string strategy from it,
/// reporting failures as the crate-internal `InternalError`.
#[allow(
  clippy::single_call_fn,
  reason = "parse a regex into an HIR and build the string strategy behind string_regex"
)]
fn string_regex_inner(regex: &str) -> InternalParseResult<String> {
  let hir = ParserBuilder::new().build().parse(regex)?;
  string_regex_parsed_inner(&hir)
}

/// Like `string_regex()`, but allows providing a pre-parsed expression.
///
/// # Errors
///
/// Returns `RegexStrategyError::UnsupportedRegex` if `expr` uses a construct proptest
/// cannot generate, such as an anchor or look-around. A pre-parsed `expr`
/// cannot carry a syntax error, so `RegexStrategyError::RegexSyntax` is never returned.
pub fn string_regex_parsed(expr: &Hir) -> ParseResult<String> {
  string_regex_parsed_inner(expr).map_err(RegexStrategyError::from)
}

/// Build a `String` strategy from an already-parsed regex HIR by generating
/// the matching bytes and decoding them as UTF-8.
fn string_regex_parsed_inner(expr: &Hir) -> InternalParseResult<String> {
  bytes_regex_parsed_inner(expr)
    .map(|bytes_strategy| {
      bytes_strategy
        .prop_filter_map("regex bytes must decode as UTF-8", |bytes| String::from_utf8(bytes).ok())
        .sboxed()
    })
    .map(RegexGeneratorStrategy)
}

/// Creates a strategy which generates byte strings matching the given regular
/// expression.
///
/// By default, the byte strings generated by this strategy _will_ be valid
/// UTF-8.  If you wish to generate byte strings that aren't (necessarily)
/// valid UTF-8, wrap your regex (or some subsection of it) in `(?-u: ... )`.
/// You may want to turn on the `s` flag as well (`(?s-u: ... )`) so that `.`
/// will generate newline characters (byte value `0x0A`).  See the
/// [`regex` crate's documentation](https://docs.rs/regex/*/regex/#opt-out-of-unicode-support)
/// for more information.
///
/// # Errors
///
/// Returns `RegexStrategyError::RegexSyntax` if `regex` is not valid regex syntax, or
/// `RegexStrategyError::UnsupportedRegex` if it parses but uses a construct proptest
/// cannot generate (for example an anchor or look-around).
#[allow(
  clippy::single_call_fn,
  reason = "public entry point converting a byte-regex source into the crate's typed ParseResult"
)]
pub fn bytes_regex(regex: &str) -> ParseResult<Vec<u8>> {
  bytes_regex_inner(regex).map_err(RegexStrategyError::from)
}

/// Parse `regex` into an HIR (with UTF-8 requirements relaxed so byte
/// classes are allowed) and build the byte-string strategy from it,
/// reporting failures as the crate-internal `InternalError`.
#[allow(
  clippy::single_call_fn,
  reason = "parse a byte-regex source with UTF-8 relaxed and build the byte-string strategy"
)]
fn bytes_regex_inner(regex: &str) -> InternalParseResult<Vec<u8>> {
  let hir = ParserBuilder::new().utf8(false).build().parse(regex)?;
  bytes_regex_parsed_inner(&hir)
}

/// Like `bytes_regex()`, but allows providing a pre-parsed expression.
///
/// # Errors
///
/// Returns `RegexStrategyError::UnsupportedRegex` if `expr` uses a construct proptest
/// cannot generate, such as an anchor or look-around. A pre-parsed `expr`
/// cannot carry a syntax error, so `RegexStrategyError::RegexSyntax` is never returned.
pub fn bytes_regex_parsed(expr: &Hir) -> ParseResult<Vec<u8>> {
  bytes_regex_parsed_inner(expr).map_err(RegexStrategyError::from)
}

/// Recursively translate a regex HIR node into a boxed byte-string
/// strategy, threading the crate-internal `InternalError` for unsupported
/// constructs.
fn bytes_regex_parsed_inner(expr: &Hir) -> InternalParseResult<Vec<u8>> {
  match *expr.kind() {
    HirKind::Empty => Ok(Just(vec![]).sboxed()),

    HirKind::Literal(ref lit) => Ok(Just(lit.0.to_vec()).sboxed()),

    HirKind::Class(ref regex_class) => Ok(match *regex_class {
      hir::Class::Unicode(ref unicode_class) => unicode_class_strategy(unicode_class).prop_map(to_bytes).sboxed(),
      hir::Class::Bytes(ref byte_class) => {
        let subs = byte_class.iter().map(|range| range.start()..=range.end());
        Union::new(subs).prop_map(|byte| vec![byte]).sboxed()
      }
    }),

    HirKind::Repetition(ref rep) => Ok(
      vec(bytes_regex_parsed_inner(&rep.sub)?, to_range(rep)?)
        .prop_map(|parts| parts.concat())
        .sboxed(),
    ),

    HirKind::Capture(ref capture) => bytes_regex_parsed_inner(&capture.sub).map(|captured| captured.0),

    HirKind::Concat(ref subexpressions) => {
      let mut concat_iter = ConcatIter {
        iter: subexpressions.iter(),
        buf:  vec![],
        next: None,
      };
      let ext = |(mut lhs, rhs): (Vec<_>, _)| {
        lhs.extend(rhs);
        lhs
      };
      Ok(
        concat_iter
          .try_fold(None, |accum, rhs| -> Result<_, InternalError> {
            let rhs_strategy = rhs?;
            Ok(match accum {
              None => Some(rhs_strategy.sboxed()),
              Some(accum_strategy) => Some((accum_strategy, rhs_strategy).prop_map(ext).sboxed()),
            })
          })?
          .unwrap_or_else(|| Just(vec![]).sboxed()),
      )
    }

    HirKind::Alternation(ref subs) => {
      let options = subs.iter().map(bytes_regex_parsed_inner).collect::<Result<Vec<_>, _>>()?;
      Union::try_new_uniform(options)
        .map(Strategy::sboxed)
        .map_err(|error| match error {
          UnionBuildError::Empty => InternalError::UnsupportedRegex("empty alternation"),
          UnionBuildError::InvalidProbability | UnionBuildError::ZeroWeight | UnionBuildError::WeightSumOverflow => {
            InternalError::UnsupportedRegex("invalid alternation")
          }
        })
    }

    HirKind::Look(_) => unsupported("anchors/boundaries not supported for string generation"),
  }
  .map(RegexGeneratorStrategy)
}

/// Build a `char` strategy covering a Unicode character class.
///
/// The dot-without-newline class (`\x00-\x09` plus `\x0B-\u{10FFFF}`) is
/// special-cased onto weighted ranges that lift the bias away from the tiny
/// control-character span; any other class maps straight to its ranges.
#[allow(
  clippy::single_call_fn,
  reason = "map a regex HIR Unicode class onto a CharStrategy, special-casing the dot-without-newline class"
)]
fn unicode_class_strategy(class: &hir::ClassUnicode) -> char::CharStrategy<'static> {
  static NONL_RANGES: &[RangeInclusive<char>] = &[
    '\x00'..='\x09',
    // Multiple instances of the latter range to partially make up
    // for the bias of having such a tiny range in the control
    // characters.
    '\x0B'..=char::MAX,
    '\x0B'..=char::MAX,
    '\x0B'..=char::MAX,
    '\x0B'..=char::MAX,
    '\x0B'..=char::MAX,
  ];

  let dotnnl = |x: &hir::ClassUnicodeRange, y: &hir::ClassUnicodeRange| {
    x.start() == '\0' && x.end() == '\x09' && y.start() == '\x0B' && y.end() == '\u{10FFFF}'
  };

  char::ranges(match *class.ranges() {
    [ref x, ref y] if dotnnl(x, y) || dotnnl(y, x) => Cow::Borrowed(NONL_RANGES),
    _ => Cow::Owned(class.iter().map(|range| range.start()..=range.end()).collect()),
  })
}

/// Iterator over the children of a regex concatenation that coalesces
/// adjacent literals into a single node before yielding non-literal
/// children.
///
/// Fusing runs of literals keeps the generated strategy shallow instead of
/// concatenating one strategy per literal byte.
struct ConcatIter<'a, I> {
  /// Bytes of the literal run accumulated so far, flushed as one node.
  buf:  Vec<u8>,
  /// Remaining children of the concatenation still to be visited.
  iter: I,
  /// A non-literal child held back to yield once the pending literal run
  /// has been flushed.
  next: Option<&'a Hir>,
}

/// Take the accumulated literal bytes and yield them as a single `Just`
/// byte-string strategy, emptying the buffer.
fn flush_lit_buf<I>(it: &mut ConcatIter<'_, I>) -> RegexGeneratorStrategy<Vec<u8>> {
  RegexGeneratorStrategy(Just(take(&mut it.buf)).sboxed())
}

impl<'a, I: Iterator<Item = &'a Hir>> Iterator for ConcatIter<'a, I> {
  type Item = InternalParseResult<Vec<u8>>;

  fn next(&mut self) -> Option<Self::Item> {
    // A left-over node, process it first:
    if let Some(next) = self.next.take() {
      return Some(bytes_regex_parsed_inner(next));
    }

    // Accumulate a literal sequence as long as we can:
    while let Some(next) = self.iter.next() {
      // A literal. Accumulate:
      if let HirKind::Literal(ref literal) = *next.kind() {
        self.buf.extend_from_slice(&literal.0);
        continue;
      }
      // Encountered a non-literal without an accumulated literal from
      // before; just yield this node.
      if self.buf.is_empty() {
        return Some(bytes_regex_parsed_inner(next));
      }
      // We've accumulated a literal from before, flush it out.
      // Store this node so we deal with it the next call.
      self.next = Some(next);
      return Some(Ok(flush_lit_buf(self)));
    }

    // Flush out any accumulated literal from before.
    if self.buf.is_empty() {
      self.next.take().map(bytes_regex_parsed_inner)
    } else {
      Some(Ok(flush_lit_buf(self)))
    }
  }
}

/// Translate a regex repetition's `{min,max}` bounds into a `SizeRange`,
/// capping unbounded repeats at a generation-friendly limit.
///
/// Unbounded (`*`, `+`, `{n,}`) repeats become finite ranges so generation
/// terminates; the two `u32::MAX` corner cases are rejected as unsupported.
#[allow(
  clippy::single_call_fn,
  reason = "translate a regex repetition's min and max bounds into a generation-bounded SizeRange"
)]
fn to_range(rep: &Repetition) -> Result<SizeRange, InternalError> {
  Ok(match (rep.min, rep.max) {
    // Zero or one
    (0, Some(1)) => size_range(0..=1),
    // Zero or more
    (0, None) => size_range(0..=32),
    // One or more
    (1, None) => size_range(1..=32),
    // Exact count of u32::MAX
    (u32::MAX, Some(u32::MAX)) => {
      return unsupported("Cannot have repetition of exactly u32::MAX");
    }
    // Exact count
    (min, Some(max)) if min == max => size_range(repetition_bound(min)),
    // At least min
    (min, None) => {
      let max = if min < u32::MAX.div_euclid(2) {
        repetition_bound(min).saturating_mul(2)
      } else {
        repetition_bound(u32::MAX)
      };
      size_range(repetition_bound(min)..max)
    }
    // Bounded range with max of u32::MAX
    (_, Some(u32::MAX)) => {
      return unsupported("Cannot have repetition max of u32::MAX");
    }
    // Bounded range
    (min, Some(max)) => size_range(repetition_bound(min)..repetition_bound(max).saturating_add(1)),
  })
}

/// Convert a regex repetition bound to the platform size domain.
fn repetition_bound(bound: u32) -> usize {
  usize::try_from(bound).unwrap_or(usize::MAX)
}

/// Encode a single `char` into its UTF-8 byte sequence.
#[allow(
  clippy::single_call_fn,
  reason = "encode one generated char into its UTF-8 byte sequence for the byte-regex strategy"
)]
fn to_bytes(khar: char) -> Vec<u8> {
  let mut buf = [0_u8; 4];
  khar.encode_utf8(&mut buf).as_bytes().to_owned()
}

/// Build an `Err` reporting a regex construct proptest cannot generate.
const fn unsupported<T>(error: &'static str) -> Result<T, InternalError> {
  Err(InternalError::UnsupportedRegex(error))
}

#[cfg(test)]
mod test {
  use std::collections::HashSet;
  use std::format;

  use regex::Regex;
  use regex::bytes::Regex as BytesRegex;
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_contains;
  use strict_test_support::ensure_ok;
  use strict_test_support::ensure_some;

  use super::*;
  use crate::test_runner::test_runner_without_persistence;

  #[test]
  fn regex_generator_value_tree_is_debug() -> Result<(), TestFailure> {
    let strategy = ensure_ok(string_regex("[a-z]+"), "the pattern is supported")?;
    let mut runner = TestRunner::deterministic();
    let value_tree = ensure_some(strategy.new_tree(&mut runner).ok(), "the string strategy builds a value tree")?;
    let rendered = format!("{value_tree:?}");
    ensure_contains(
      &rendered,
      "RegexGeneratorValueTree",
      "the debug rendering names the value-tree type",
    )
  }

  fn do_test(pattern: &str, min_distinct: usize, max_distinct: usize, iterations: usize) -> Result<(), TestFailure> {
    let generated = generate_values_matching_regex(pattern, iterations)?;
    ensure(
      generated.len() >= min_distinct,
      "generated at least the expected number of distinct strings",
    )?;
    ensure(
      generated.len() <= max_distinct,
      "generated at most the expected number of distinct strings",
    )
  }

  fn do_test_bytes(pattern: &str, min_distinct: usize, max_distinct: usize, iterations: usize) -> Result<(), TestFailure> {
    let generated = generate_byte_values_matching_regex(pattern, iterations)?;
    ensure(
      generated.len() >= min_distinct,
      "generated at least the expected number of distinct byte strings",
    )?;
    ensure(
      generated.len() <= max_distinct,
      "generated at most the expected number of distinct byte strings",
    )
  }

  fn generate_values_matching_regex(pattern: &str, iterations: usize) -> Result<HashSet<String>, TestFailure> {
    let rx = ensure_ok(Regex::new(pattern), "the pattern is valid regex")?;
    let mut generated = HashSet::new();

    let strategy = ensure_ok(string_regex(pattern), "the pattern is supported")?;
    let mut runner = TestRunner::deterministic();
    for _ in 0..iterations {
      let mut value = ensure_some(strategy.new_tree(&mut runner).ok(), "string strategy generates a value tree")?;

      ensure_string_value_matches_and_record(&value, &rx, &mut generated)?;
      while value.simplify() {
        ensure_string_value_matches_and_record(&value, &rx, &mut generated)?;
      }
    }
    Ok(generated)
  }

  fn ensure_string_value_matches_and_record<V>(value: &V, rx: &Regex, generated: &mut HashSet<String>) -> Result<(), TestFailure>
  where
    V: ValueTree<Value = String>,
  {
    let produced = value.current();
    let ok = rx
      .find(&produced)
      .is_some_and(|matsch| 0 == matsch.start() && produced.len() == matsch.end());
    ensure(ok, "every generated string matches the pattern")?;

    let _was_new = generated.insert(produced);
    Ok(())
  }

  #[allow(
    clippy::single_call_fn,
    reason = "test-only helper collecting every byte string a byte-regex strategy generates while shrinking"
  )]
  fn generate_byte_values_matching_regex(pattern: &str, iterations: usize) -> Result<HashSet<Vec<u8>>, TestFailure> {
    let rx = ensure_ok(BytesRegex::new(pattern), "the pattern is valid byte regex")?;
    let mut generated = HashSet::new();

    let strategy = ensure_ok(bytes_regex(pattern), "the pattern is supported")?;
    let mut runner = TestRunner::deterministic();
    for _ in 0..iterations {
      let mut value = ensure_some(strategy.new_tree(&mut runner).ok(), "byte-string strategy generates a value tree")?;

      ensure_byte_value_matches_and_record(&value, &rx, &mut generated)?;
      while value.simplify() {
        ensure_byte_value_matches_and_record(&value, &rx, &mut generated)?;
      }
    }
    Ok(generated)
  }

  fn ensure_byte_value_matches_and_record<V>(value: &V, rx: &BytesRegex, generated: &mut HashSet<Vec<u8>>) -> Result<(), TestFailure>
  where
    V: ValueTree<Value = Vec<u8>>,
  {
    let produced = value.current();
    let ok = rx
      .find(&produced)
      .is_some_and(|matsch| 0 == matsch.start() && produced.len() == matsch.end());
    ensure(ok, "every generated byte string matches the pattern")?;

    let _was_new = generated.insert(produced);
    Ok(())
  }

  #[test]
  fn test_case_insensitive_produces_all_available_values() -> Result<(), TestFailure> {
    let expected: HashSet<String> = ["a", "b", "A", "B"].into_iter().map(String::from).collect();
    ensure(
      generate_values_matching_regex("(?i:a|B)", 64)? == expected,
      "a case-insensitive alternation generates every casing",
    )
  }

  #[test]
  fn test_literal() -> Result<(), TestFailure> {
    do_test("foo", 1, 1, 8)?;
    do_test_bytes("foo", 1, 1, 8)
  }

  #[test]
  fn test_casei_literal() -> Result<(), TestFailure> {
    do_test("(?i:fOo)", 8, 8, 64)
  }

  #[test]
  fn test_alternation() -> Result<(), TestFailure> {
    do_test("foo|bar|baz", 3, 3, 16)?;
    do_test_bytes("foo|bar|baz", 3, 3, 16)
  }

  #[test]
  fn test_repetition() -> Result<(), TestFailure> {
    do_test("a{0,8}", 9, 9, 64)?;
    do_test_bytes("a{0,8}", 9, 9, 64)
  }

  #[test]
  fn test_question() -> Result<(), TestFailure> {
    do_test("a?", 2, 2, 16)?;
    do_test_bytes("a?", 2, 2, 16)
  }

  #[test]
  fn test_star() -> Result<(), TestFailure> {
    do_test("a*", 33, 33, 256)?;
    do_test_bytes("a*", 33, 33, 256)
  }

  #[test]
  fn test_plus() -> Result<(), TestFailure> {
    do_test("a+", 32, 32, 256)?;
    do_test_bytes("a+", 32, 32, 256)
  }

  #[test]
  fn test_n_to_range() -> Result<(), TestFailure> {
    do_test("a{4,}", 4, 4, 64)?;
    do_test_bytes("a{4,}", 4, 4, 64)
  }

  #[test]
  fn test_concatenation() -> Result<(), TestFailure> {
    do_test("(foo|bar)(xyzzy|plugh)", 4, 4, 32)?;
    do_test_bytes("(foo|bar)(xyzzy|plugh)", 4, 4, 32)
  }

  #[test]
  fn test_ascii_class() -> Result<(), TestFailure> {
    do_test("[[:digit:]]", 10, 10, 256)
  }

  #[test]
  fn test_unicode_class() -> Result<(), TestFailure> {
    do_test("\\p{Greek}", 24, 512, 256)
  }

  #[test]
  fn test_dot() -> Result<(), TestFailure> {
    do_test(".", 200, 65536, 256)
  }

  #[test]
  fn test_dot_s() -> Result<(), TestFailure> {
    do_test("(?s).", 200, 65536, 256)?;
    do_test_bytes("(?s-u).", 256, 256, 2048)
  }

  #[test]
  fn test_backslash_d_plus() -> Result<(), TestFailure> {
    do_test("\\d+", 1, 65536, 256)
  }

  #[test]
  fn test_non_utf8_byte_strings() -> Result<(), TestFailure> {
    do_test_bytes(r"(?-u)[\xC0-\xFF]\x20", 64, 64, 512)?;
    do_test_bytes(r"(?-u)\x20[\x80-\xBF]", 64, 64, 512)?;
    do_test_bytes(
      r"(?x-u)
  \xed (( ( \xa0\x80 | \xad\xbf | \xae\x80 | \xaf\xbf )
          ( \xed ( \xb0\x80 | \xbf\xbf ) )? )
        | \xb0\x80 | \xbe\x80 | \xbf\xbf )",
      15,
      15,
      120,
    )
  }

  #[allow(
    clippy::single_call_fn,
    reason = "test-only assertion that the regex strategy type stays Send and Sync"
  )]
  fn ensure_send_and_sync<T: Send + Sync>(_: T) {}

  #[test]
  fn regex_strategy_is_send_and_sync() -> Result<(), TestFailure> {
    ensure_send_and_sync(ensure_ok(string_regex("."), "the dot pattern is supported")?);
    Ok(())
  }

  macro_rules! consistent {
    ($name:ident, $value:expr) => {
      #[test]
      fn $name() -> Result<(), TestFailure> {
        test_generates_matching_strings($value)
      }
    };
  }

  fn test_generates_matching_strings(pattern: &str) -> Result<(), TestFailure> {
    use std::time;

    let mut runner = test_runner_without_persistence();
    let start = time::Instant::now();

    // If we don't support this regex, just move on quietly
    let Ok(strategy) = string_regex(pattern) else {
      return Ok(());
    };
    let rx = ensure_ok(Regex::new(pattern), "a supported pattern is valid regex")?;

    for _ in 0..1000 {
      let mut val = ensure_some(strategy.new_tree(&mut runner).ok(), "string strategy generates a value tree")?;
      ensure_current_string_matches(&val, &rx)?;

      // No more than 1000 simplify steps to keep test time down
      let mut simplify_steps = 0_u16;
      while simplify_steps < 1000 && val.simplify() {
        simplify_steps = simplify_steps.saturating_add(1);
        ensure_current_string_matches(&val, &rx)?;
      }

      // Quietly stop testing if we've run for >10 s
      if start.elapsed().as_secs() > 10 {
        break;
      }
    }
    Ok(())
  }

  fn ensure_current_string_matches<V>(val: &V, rx: &Regex) -> Result<(), TestFailure>
  where
    V: ValueTree<Value = String>,
  {
    let produced = val.current();
    ensure(rx.is_match(&produced), "every produced string matches the source pattern")
  }

  include!("regex-contrib/crates_regex.rs");
}
