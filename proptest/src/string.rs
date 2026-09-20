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
use regex_syntax::hir;
use regex_syntax::hir::Hir;
use regex_syntax::hir::HirKind;
use regex_syntax::hir::Repetition;

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
  use core::hash::Hash;
  use std::collections::HashSet;
  use std::format;
  use std::time::Duration;
  use std::time::Instant;

  use regex::Regex;
  use regex::bytes::Regex as BytesRegex;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::test_runner::test_runner_without_persistence;

  /// Reached regex trees and every string observed along their shrink walks.
  type Walks<T> = Vec<Result<(RegexGeneratorValueTree<T>, Vec<T>), Reason>>;

  /// Pattern, matcher, strategy, and native generation outcomes for a regex run.
  #[derive(Debug)]
  struct RegexSamples<T: fmt::Debug, R> {
    /// The pattern supplied to both regex implementations.
    pattern:  String,
    /// Native matcher construction result.
    matcher:  Result<R, regex::Error>,
    /// Native generator construction result.
    strategy: ParseResult<T>,
    /// Every reached tree and its generated values.
    walks:    Walks<T>,
  }

  /// Full regex sampling subject together with the requested diversity bounds.
  type BoundedSamples<T, R> = (RegexSamples<T, R>, RangeInclusive<usize>);
  /// Assertion result retaining both regex implementations and all sampled values.
  type CheckedSamples<T, R> = Result<BoundedSamples<T, R>, Box<PredicateFailure<BoundedSamples<T, R>>>>;
  /// A string sample collection and its expected complete diversity set.
  type ExpectedStrings = (RegexSamples<String, Regex>, HashSet<String>);
  /// Complete matching and diversity checks for the string and byte generators.
  type RepresentationChecks = (CheckedSamples<String, Regex>, CheckedSamples<Vec<u8>, BytesRegex>);
  /// Both representations' native sampling outcomes, including either failure.
  type CheckedRepresentations = Result<RepresentationChecks, Box<PredicateFailure<RepresentationChecks>>>;

  /// Observe a bounded simplification walk separately from generation and its wall-clock budget.
  #[allow(
    clippy::single_call_fn,
    reason = "bounded tree traversal is independent of regex construction and the generation time budget"
  )]
  fn shrink_regex<T: fmt::Debug>(mut tree: RegexGeneratorValueTree<T>, limit: usize) -> (RegexGeneratorValueTree<T>, Vec<T>) {
    let mut values = vec![tree.current()];
    for _ in 0..limit {
      if !tree.simplify() {
        break;
      }
      values.push(tree.current());
    }
    (tree, values)
  }

  /// Preserve every generated and simplified value, respecting the existing optional time bound.
  fn sample_regex<T: fmt::Debug>(
    strategy: &ParseResult<T>,
    runner: &mut TestRunner,
    iterations: usize,
    shrink_limit: usize,
    start_time: Option<Instant>,
  ) -> Walks<T> {
    let mut walks = Vec::new();
    let Ok(generator) = strategy.as_ref() else {
      return walks;
    };
    for _ in 0..iterations {
      walks.push(generator.new_tree(runner).map(|tree| shrink_regex(tree, shrink_limit)));
      if start_time.is_some_and(|start| start.elapsed().as_secs() > 10) {
        break;
      }
    }
    walks
  }

  /// Generate string walks together with both native regex construction results.
  fn generate_values_matching_regex(pattern: &str, iterations: usize) -> RegexSamples<String, Regex> {
    let matcher = Regex::new(pattern);
    let strategy = string_regex(pattern);
    let walks = sample_regex(&strategy, &mut TestRunner::deterministic(), iterations, usize::MAX, None);
    RegexSamples {
      pattern: pattern.to_owned(),
      matcher,
      strategy,
      walks,
    }
  }

  impl<T: fmt::Debug, R> RegexSamples<T, R> {
    /// Check every native generation outcome against its constructed matcher.
    fn matches(&self, pattern: fn(&R) -> &str, accepts: impl Fn(&R, &T) -> bool) -> bool {
      let Ok(ref matcher) = self.matcher else {
        return false;
      };
      pattern(matcher) == self.pattern
        && self.strategy.is_ok()
        && self.walks.iter().all(Result::is_ok)
        && self
          .walks
          .iter()
          .filter_map(|walk| walk.as_ref().ok())
          .flat_map(|generation| &generation.1)
          .all(|produced| accepts(matcher, produced))
    }

    /// Borrow all distinct values while the sample report retains their owners.
    fn distinct_values(&self) -> HashSet<&T>
    where
      T: Eq + Hash,
    {
      self
        .walks
        .iter()
        .filter_map(|walk| walk.as_ref().ok())
        .flat_map(|generation| &generation.1)
        .collect()
    }
  }

  /// Validate native matching and diversity evidence shared by both regex representations.
  fn check_samples<T, R>(
    samples: RegexSamples<T, R>,
    bounds: RangeInclusive<usize>,
    pattern: fn(&R) -> &str,
    accepts: impl Fn(&R, &T) -> bool,
  ) -> CheckedSamples<T, R>
  where
    T: fmt::Debug + Eq + Hash,
    R: fmt::Debug,
  {
    ensure_that(
      (samples, bounds),
      "all generated values fully match and have the requested diversity",
      |observed| observed.0.matches(pattern, &accepts) && observed.1.contains(&observed.0.distinct_values().len()),
    )
    .map_err(Box::new)
  }

  /// Check string matches and diversity without discarding any generation outcome.
  fn do_test(pattern: &str, min_distinct: usize, max_distinct: usize, iterations: usize) -> CheckedSamples<String, Regex> {
    check_samples(
      generate_values_matching_regex(pattern, iterations),
      min_distinct..=max_distinct,
      Regex::as_str,
      |matcher, produced| {
        matcher
          .find(produced)
          .is_some_and(|found| found.start() == 0 && found.end() == produced.len())
      },
    )
  }

  /// Check arbitrary-byte regex matches with the same retained native outcomes.
  fn do_test_bytes(pattern: &str, min_distinct: usize, max_distinct: usize, iterations: usize) -> CheckedSamples<Vec<u8>, BytesRegex> {
    let matcher = BytesRegex::new(pattern);
    let strategy = bytes_regex(pattern);
    let walks = sample_regex(&strategy, &mut TestRunner::deterministic(), iterations, usize::MAX, None);
    let samples = RegexSamples {
      pattern: pattern.to_owned(),
      matcher,
      strategy,
      walks,
    };
    check_samples(samples, min_distinct..=max_distinct, BytesRegex::as_str, |compiled, produced| {
      compiled
        .find(produced)
        .is_some_and(|found| found.start() == 0 && found.end() == produced.len())
    })
  }

  /// Check both regex representations while retaining each complete sampling outcome.
  fn check_representations(strings: CheckedSamples<String, Regex>, bytes: CheckedSamples<Vec<u8>, BytesRegex>) -> CheckedRepresentations {
    ensure_that(
      (strings, bytes),
      "every regex representation satisfies its sampling contract",
      |observed| observed.0.is_ok() && observed.1.is_ok(),
    )
    .map_err(Box::new)
  }

  /// Exercise the same pattern and diversity bounds through both public generators.
  fn do_test_both(pattern: &str, min_distinct: usize, max_distinct: usize, iterations: usize) -> CheckedRepresentations {
    check_representations(
      do_test(pattern, min_distinct, max_distinct, iterations),
      do_test_bytes(pattern, min_distinct, max_distinct, iterations),
    )
  }

  #[test]
  fn regex_generator_value_tree_is_debug() -> Result<(), impl fmt::Debug> {
    let result = string_regex("[a-z]+").map(|strategy| {
      let generated = strategy.new_tree(&mut TestRunner::deterministic()).map(|tree| {
        let rendered = format!("{tree:?}");
        (tree, rendered)
      });
      (strategy, generated)
    });
    ensure_that(result, "the native value-tree debug rendering names its type", |observed| {
      let Ok(generated) = observed.as_ref() else {
        return false;
      };
      generated
        .1
        .as_ref()
        .is_ok_and(|rendering| rendering.1.contains("RegexGeneratorValueTree"))
    })
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn test_case_insensitive_produces_all_available_values() -> Result<(), Box<PredicateFailure<ExpectedStrings>>> {
    let expected: HashSet<String> = ["a", "b", "A", "B"].into_iter().map(String::from).collect();
    ensure_that(
      (generate_values_matching_regex("(?i:a|B)", 64), expected),
      "case-insensitive alternation generates every casing",
      |observed| {
        observed.0.matcher.is_ok()
          && observed.0.strategy.is_ok()
          && observed.0.walks.iter().all(Result::is_ok)
          && observed.0.distinct_values() == observed.1.iter().collect()
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn test_literal() -> Result<(), impl fmt::Debug> {
    do_test_both("foo", 1, 1, 8).map(drop)
  }

  #[test]
  fn test_casei_literal() -> Result<(), impl fmt::Debug> {
    do_test("(?i:fOo)", 8, 8, 64).map(drop)
  }

  #[test]
  fn test_alternation() -> Result<(), impl fmt::Debug> {
    do_test_both("foo|bar|baz", 3, 3, 16).map(drop)
  }

  #[test]
  fn test_repetition() -> Result<(), impl fmt::Debug> {
    do_test_both("a{0,8}", 9, 9, 64).map(drop)
  }

  #[test]
  fn test_question() -> Result<(), impl fmt::Debug> {
    do_test_both("a?", 2, 2, 16).map(drop)
  }

  #[test]
  fn test_star() -> Result<(), impl fmt::Debug> {
    do_test_both("a*", 33, 33, 256).map(drop)
  }

  #[test]
  fn test_plus() -> Result<(), impl fmt::Debug> {
    do_test_both("a+", 32, 32, 256).map(drop)
  }

  #[test]
  fn test_n_to_range() -> Result<(), impl fmt::Debug> {
    do_test_both("a{4,}", 4, 4, 64).map(drop)
  }

  #[test]
  fn test_concatenation() -> Result<(), impl fmt::Debug> {
    do_test_both("(foo|bar)(xyzzy|plugh)", 4, 4, 32).map(drop)
  }

  #[test]
  fn test_ascii_class() -> Result<(), impl fmt::Debug> {
    do_test("[[:digit:]]", 10, 10, 256).map(drop)
  }

  #[test]
  fn test_unicode_class() -> Result<(), impl fmt::Debug> {
    do_test("\\p{Greek}", 24, 512, 256).map(drop)
  }

  #[test]
  fn test_dot() -> Result<(), impl fmt::Debug> {
    do_test(".", 200, 65536, 256).map(drop)
  }

  #[test]
  fn test_dot_s() -> Result<(), impl fmt::Debug> {
    check_representations(do_test("(?s).", 200, 65536, 256), do_test_bytes("(?s-u).", 256, 256, 2048)).map(drop)
  }

  #[test]
  fn test_backslash_d_plus() -> Result<(), impl fmt::Debug> {
    do_test("\\d+", 1, 65536, 256).map(drop)
  }

  #[test]
  fn test_non_utf8_byte_strings() -> Result<(), impl fmt::Debug> {
    let result_1 = do_test_bytes(r"(?-u)[\xC0-\xFF]\x20", 64, 64, 512);
    let result_2 = do_test_bytes(r"(?-u)\x20[\x80-\xBF]", 64, 64, 512);
    let result_3 = do_test_bytes(
      r"(?x-u)
  \xed (( ( \xa0\x80 | \xad\xbf | \xae\x80 | \xaf\xbf )
          ( \xed ( \xb0\x80 | \xbf\xbf ) )? )
        | \xb0\x80 | \xbe\x80 | \xbf\xbf )",
      15,
      15,
      120,
    );
    ensure_that(
      (result_1, result_2, result_3),
      "every regex representation satisfies its sampling contract",
      |observed| observed.0.is_ok() && observed.1.is_ok() && observed.2.is_ok(),
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// Preserve the value while statically checking thread-transfer guarantees.
  #[allow(
    clippy::single_call_fn,
    reason = "the identity boundary checks thread-transfer bounds while preserving its subject"
  )]
  fn ensure_send_and_sync<T: Send + Sync>(value: T) -> T {
    value
  }

  #[test]
  fn regex_strategy_is_send_and_sync() -> Result<(), PredicateFailure<ParseResult<String>>> {
    ensure_that(
      ensure_send_and_sync(string_regex(".")),
      "the dot strategy is supported and Send + Sync",
      Result::is_ok,
    )
    .map(drop)
  }

  /// A contributed regex's supported run or its native unsupported-construction outcome.
  enum ConsistencyRun {
    /// Unsupported patterns remain an explicit portability skip.
    Unsupported {
      pattern: String,
      error:   RegexStrategyError,
    },
    /// Supported patterns retain every attempted generation and its elapsed time.
    Supported {
      samples: RegexSamples<String, Regex>,
      elapsed: Duration,
    },
  }

  impl fmt::Debug for ConsistencyRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      match *self {
        Self::Unsupported {
          ref pattern,
          ref error,
        } => formatter
          .debug_struct("Unsupported")
          .field("pattern", pattern)
          .field("error", error)
          .finish(),
        Self::Supported {
          ref samples,
          elapsed,
        } => formatter
          .debug_struct("Supported")
          .field("samples", samples)
          .field("elapsed", &elapsed)
          .finish(),
      }
    }
  }

  macro_rules! consistent {
    ($name:ident, $value:expr) => {
      #[test]
      fn $name() -> Result<(), Box<PredicateFailure<ConsistencyRun>>> {
        test_generates_matching_strings($value).map(drop)
      }
    };
  }

  /// Exercise a contributed pattern under the existing case, shrink, and time budgets.
  fn test_generates_matching_strings(pattern: &str) -> Result<ConsistencyRun, Box<PredicateFailure<ConsistencyRun>>> {
    let start = Instant::now();
    let construction = string_regex(pattern);
    let outcome = match construction {
      Err(error) => ConsistencyRun::Unsupported {
        pattern: pattern.to_owned(),
        error,
      },
      Ok(generator) => {
        let strategy = Ok(generator);
        let matcher = Regex::new(pattern);
        let walks = sample_regex(&strategy, &mut test_runner_without_persistence(), 1000, 1000, Some(start));
        ConsistencyRun::Supported {
          samples: RegexSamples {
            pattern: pattern.to_owned(),
            matcher,
            strategy,
            walks,
          },
          elapsed: start.elapsed(),
        }
      }
    };
    ensure_that(
      outcome,
      "every generated value for a supported contributed regex matches its source pattern",
      |observed| match *observed {
        ConsistencyRun::Unsupported {
          ..
        } => true,
        ConsistencyRun::Supported {
          ref samples, ..
        } => samples.matches(Regex::as_str, |matcher, produced| matcher.is_match(produced)),
      },
    )
    .map_err(Box::new)
  }

  include!("regex-contrib/crates_regex.rs");
}
