//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for generating `std::Option` values.

use core::cmp::Ordering;
use core::error::Error;
use core::fmt;
use core::marker::PhantomData;

use crate::std_facade::Rc;
use crate::strategy::LazyValueTree;
use crate::strategy::NewTree;
use crate::strategy::Strategy;
use crate::strategy::TupleUnion;
use crate::strategy::TupleUnionActive2;
use crate::strategy::TupleUnionValueTree;
use crate::strategy::ValueTree;
use crate::strategy::WeightedStrategy;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::float_to_weight;
use crate::strategy::statics;
use crate::test_runner::TestRunner;

//==============================================================================
// Probability
//==============================================================================

/// Creates a `Probability` from some value that is convertible into it.
///
/// Floating-point inputs are normalized into the inclusive `0.0..=1.0` range;
/// use [`Probability::try_new`] when out-of-range input should be reported as
/// a typed error.
#[allow(
  clippy::single_call_fn,
  reason = "public ergonomic constructor names probability normalization at call sites"
)]
pub fn prob(from: impl Into<Probability>) -> Probability {
  from.into()
}

impl Default for Probability {
  /// The default probability is 0.5, or 50% chance.
  fn default() -> Self {
    prob(0.5)
  }
}

impl Probability {
  /// Normalize a raw floating-point probability into the valid range.
  #[allow(
    clippy::single_call_fn,
    reason = "name the legacy Probability clamp rule shared by its constructor documentation"
  )]
  fn normalize(probability: f64) -> f64 {
    match probability.partial_cmp(&0.0) {
      None => 0.5,
      Some(Ordering::Less) => 0.0,
      Some(Ordering::Equal | Ordering::Greater) => match probability.partial_cmp(&1.0) {
        Some(Ordering::Greater) => 1.0,
        Some(Ordering::Less | Ordering::Equal) => probability,
        None => 0.5,
      },
    }
  }

  /// Creates a `Probability` from a `f64`, normalizing out-of-range input
  /// into the inclusive `0.0..=1.0` range.
  #[allow(
    clippy::single_call_fn,
    reason = "normalize a raw f64 into the 0.0..=1.0 range for the legacy Probability constructor"
  )]
  #[must_use]
  pub fn new(probability: f64) -> Self {
    Self(Self::normalize(probability))
  }

  /// Creates a `Probability` from a `f64` without normalization.
  ///
  /// ## Errors
  ///
  /// Returns [`ProbabilityError`] when `probability` is outside the
  /// inclusive `0.0..=1.0` range or is `NaN`.
  pub fn try_new(probability: f64) -> Result<Self, ProbabilityError> {
    let at_least_zero = matches!(probability.partial_cmp(&0.0), Some(Ordering::Equal | Ordering::Greater));
    let at_most_one = matches!(probability.partial_cmp(&1.0), Some(Ordering::Less | Ordering::Equal));

    if at_least_zero && at_most_one {
      Ok(Self(probability))
    } else {
      Err(ProbabilityError {
        probability,
      })
    }
  }

  // Don't rely on these existing internally:

  /// Merges self together with some other argument producing a product
  /// type expected by some implementations of `A: Arbitrary` in
  /// `A::Parameters`. This can be more ergonomic to work with and may
  /// help type inference.
  pub const fn with<X>(self, and: X) -> product_type![Self, X] {
    product_pack![self, and]
  }

  /// Merges self together with some other argument generated with a
  /// default value producing a product type expected by some
  /// implementations of `A: Arbitrary` in `A::Parameters`.
  /// This can be more ergonomic to work with and may help type inference.
  #[must_use]
  pub fn lift<X: Default>(self) -> product_type![Self, X] {
    self.with(Default::default())
  }
}

impl From<f64> for Probability {
  /// Creates a `Probability` from a `f64`, normalizing out-of-range input
  /// into the inclusive `0.0..=1.0` range.
  fn from(probability: f64) -> Self {
    Self::new(probability)
  }
}

impl From<Probability> for f64 {
  fn from(probability: Probability) -> Self {
    probability.0
  }
}

/// A probability in the range `[0.0, 1.0]` with a default of `0.5`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Probability(f64);

/// Invalid floating-point input for [`Probability::try_new`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbabilityError {
  /// The raw probability value that failed validation.
  probability: f64,
}

impl fmt::Display for ProbabilityError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      formatter,
      "probability {} is outside the inclusive 0.0..=1.0 range",
      self.probability
    )
  }
}

impl Error for ProbabilityError {}

//==============================================================================
// Strategies for Option
//==============================================================================

mapfn! {
    [] fn WrapSome[<T : fmt::Debug>](inner: T) -> Option<T> {
        Some(inner)
    }
}

/// Strategy (and its own `ValueTree`) that always produces `None`.
///
/// It forms the `None` arm of the `TupleUnion` behind `OptionStrategy` and
/// carries no inner value, so it never simplifies or complicates.
#[must_use = "strategies do nothing unless used"]
struct NoneStrategy<T>(PhantomData<T>);
impl<T> Clone for NoneStrategy<T> {
  fn clone(&self) -> Self {
    Self(PhantomData)
  }
}
impl<T> fmt::Debug for NoneStrategy<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "NoneStrategy")
  }
}
impl<T: fmt::Debug> Strategy for NoneStrategy<T> {
  type Tree = Self;
  type Value = Option<T>;

  fn new_tree(&self, _: &mut TestRunner) -> NewTree<Self> {
    Ok(Self(PhantomData))
  }
}
impl<T: fmt::Debug> ValueTree for NoneStrategy<T> {
  type Value = Option<T>;

  fn current(&self) -> Option<T> {
    None
  }
  fn simplify(&mut self) -> bool {
    false
  }
  fn complicate(&mut self) -> bool {
    false
  }
}

opaque_strategy_wrapper! {
    /// Strategy which generates `Option` values whose inner `Some` values are
    /// generated by another strategy.
    ///
    /// Constructed by other functions in this module.
    #[derive(Clone)]
    pub struct OptionStrategy[<T>][where T : Strategy]
        (TupleUnion<(WeightedStrategy<NoneStrategy<T::Value>>,
                     WeightedStrategy<statics::Map<T, WrapSome>>)>)
        -> OptionValueTree<T>;
    /// `ValueTree` type corresponding to `OptionStrategy`.
    pub struct OptionValueTree[<T>][where T : Strategy]
        (TupleUnionValueTree<(
            Option<LazyValueTree<NoneStrategy<T::Value>>>,
            Option<LazyValueTree<statics::Map<T, WrapSome>>>,
        ), TupleUnionActive2<
            NoneStrategy<T::Value>,
            <statics::Map<T, WrapSome> as Strategy>::Tree,
        >>)
        -> Option<T::Value>;
}

// XXX Unclear why this is necessary; #[derive(Debug)] *should* generate
// exactly this, but for some reason it adds a `T::Value : Debug` constraint as
// well.
impl<T: Strategy + fmt::Debug> fmt::Debug for OptionStrategy<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "OptionStrategy({:?})", self.0)
  }
}

impl<T: Strategy> Clone for OptionValueTree<T>
where
  T::Tree: Clone,
{
  fn clone(&self) -> Self {
    Self(self.0.clone())
  }
}

impl<T: Strategy> fmt::Debug for OptionValueTree<T>
where
  T::Tree: fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "OptionValueTree({:?})", self.0)
  }
}

/// Return a strategy producing `Optional` values wrapping values from the
/// given delegate strategy.
///
/// `Some` values shrink to `None`.
///
/// `Some` and `None` are each chosen with 50% probability.
pub fn of<T: Strategy>(strategy: T) -> OptionStrategy<T> {
  weighted(Probability::default(), strategy)
}

/// Return a strategy producing `Optional` values wrapping values from the
/// given delegate strategy.
///
/// `Some` values shrink to `None`.
///
/// `Some` is chosen with a probability given by `probability_of_some`, which
/// must be between 0.0 and 1.0, both exclusive.
pub fn weighted<T: Strategy>(probability_of_some: impl Into<Probability>, strategy: T) -> OptionStrategy<T> {
  let prob = probability_of_some.into().into();
  let (weight_some, weight_none) = float_to_weight(prob);

  OptionStrategy(TupleUnion::new((
    (weight_none, Rc::new(NoneStrategy(PhantomData))),
    (weight_some, Rc::new(statics::Map::new(strategy, WrapSome))),
  )))
}

#[cfg(test)]
mod test {
  use core::ops::Range;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Vec;
  use crate::strategy::Just;
  use crate::test_runner::Reason;

  /// Native generation outcomes underlying the sampled Some frequency.
  type OptionSamples = Vec<NewTree<OptionStrategy<Just<i32>>>>;

  fn sample_options(strategy: &OptionStrategy<Just<i32>>) -> OptionSamples {
    let mut runner = TestRunner::deterministic();
    (0..1000).map(|_| strategy.new_tree(&mut runner)).collect()
  }

  fn has_some_count(samples: &OptionSamples, expected: Range<usize>) -> bool {
    samples.iter().all(Result::is_ok)
      && expected.contains(
        &samples
          .iter()
          .filter(|sample| sample.as_ref().is_ok_and(|tree| tree.current().is_some()))
          .count(),
      )
  }

  #[test]
  fn probability_defaults_to_0p5() -> Result<(), PredicateFailure<OptionSamples>> {
    ensure_that(
      sample_options(&of(Just(42_i32))),
      "roughly half of the samples are Some",
      |samples| has_some_count(samples, 451..550),
    )
    .map(drop)
  }

  #[test]
  fn probability_handled_correctly() -> Result<(), PredicateFailure<[OptionSamples; 2]>> {
    ensure_that(
      [
        sample_options(&weighted(0.9, Just(42_i32))),
        sample_options(&weighted(0.1, Just(42_i32))),
      ],
      "high and low Some weights produce the expected frequencies",
      |subjects| {
        subjects
          .iter()
          .zip([801..950, 51..150])
          .all(|(sample, bounds)| has_some_count(sample, bounds))
      },
    )
    .map(drop)
  }

  #[test]
  fn test_sanity() -> Result<(), Reason> {
    check_strategy_sanity(of(0_i32..1000_i32), None)
  }
}
