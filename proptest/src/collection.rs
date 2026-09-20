//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for generating `std::collections` of values.

use core::cmp::Ord;
use core::error::Error;
use core::hash::Hash;
use core::ops::Add;
use core::ops::Range;
use core::ops::RangeInclusive;
use core::ops::RangeTo;
use core::ops::RangeToInclusive;

use crate::bits::BitSetLike as _;
use crate::bits::VarBitSet;
use crate::num::sample_uniform_incl;
use crate::std_facade::BTreeMap;
use crate::std_facade::BTreeSet;
use crate::std_facade::BinaryHeap;
#[cfg(feature = "std")]
use crate::std_facade::HashMap;
#[cfg(feature = "std")]
use crate::std_facade::HashSet;
use crate::std_facade::LinkedList;
use crate::std_facade::Vec;
use crate::std_facade::VecDeque;
use crate::std_facade::fmt;
use crate::std_facade::string::ToString as _;
use crate::strategy::NewTree;
use crate::strategy::Strategy;
use crate::strategy::ValueTree;
use crate::strategy::statics;
use crate::test_runner::Config;
use crate::test_runner::Reason;
use crate::test_runner::TestRunner;
use crate::tuple::TupleValueTree;

//==============================================================================
// SizeRange
//==============================================================================

/// The minimum and maximum range/bounds on the size of a collection.
/// The interval must form a subset of `[0, usize::MAX)`.
///
/// A value like `0..=usize::MAX` will still be accepted but will silently
/// truncate the maximum to `usize::MAX - 1`.
///
/// The `Default` is `0..PROPTEST_MAX_DEFAULT_SIZE_RANGE`. The max can be set with
/// the `PROPTEST_MAX_DEFAULT_SIZE_RANGE` env var, which defaults to `100`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SizeRange(Range<usize>);

/// Creates a `SizeRange` from some value that is convertible into it.
pub fn size_range(from: impl Into<SizeRange>) -> SizeRange {
  from.into()
}

impl Default for SizeRange {
  /// Constructs a `SizeRange` equivalent to `size_range(0..PROPTEST_MAX_DEFAULT_SIZE_RANGE)`.
  /// The max can be set with the `PROPTEST_MAX_DEFAULT_SIZE_RANGE` env var, which defaults to
  /// `100`.
  fn default() -> Self {
    size_range(0..Config::default().max_default_size_range)
  }
}

impl SizeRange {
  /// Creates a `SizeBounds` from a `RangeInclusive<usize>`.
  #[must_use]
  pub fn new(range: RangeInclusive<usize>) -> Self {
    range.into()
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

  /// The lower bound of the range (inclusive).
  #[must_use]
  pub const fn start(&self) -> usize {
    self.0.start
  }

  /// Extract the ends `[low, high]` of a `SizeRange`.
  #[must_use]
  pub const fn start_end_incl(&self) -> (usize, usize) {
    (self.start(), self.end_incl())
  }

  /// The upper bound of the range (inclusive).
  #[must_use]
  pub const fn end_incl(&self) -> usize {
    self.0.end.saturating_sub(1)
  }

  /// The upper bound of the range (exclusive).
  #[must_use]
  pub const fn end_excl(&self) -> usize {
    self.0.end
  }

  /// Iterates over every size in the range, from `start` up to `end_excl`.
  pub(crate) fn iter(&self) -> impl Iterator<Item = usize> {
    self.0.clone()
  }

  /// Returns whether the range admits no sizes at all (`start == end`).
  pub(crate) const fn is_empty(&self) -> bool {
    self.start() == self.end_excl()
  }

  /// Validate that this size range is non-empty, naming the violated
  /// invariant as a typed error instead of panicking.
  pub(crate) const fn ensure_nonempty(&self) -> Result<(), EmptySizeRange> {
    if self.is_empty() {
      Err(EmptySizeRange {
        start:    self.start(),
        end_excl: self.end_excl(),
      })
    } else {
      Ok(())
    }
  }
}

/// Error returned by the fallible collection-strategy constructors (and other
/// size-driven strategy constructors) when the requested size range is empty,
/// for example `0..0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptySizeRange {
  /// The (inclusive) lower bound of the offending range.
  start:    usize,
  /// The (exclusive) upper bound of the offending range.
  end_excl: usize,
}

impl fmt::Display for EmptySizeRange {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "Invalid use of empty size range. (hint: did you accidentally write {}..{} where you meant {}..={} somewhere?)",
      self.start, self.end_excl, self.start, self.end_excl
    )
  }
}

impl Error for EmptySizeRange {}

/// Given `(low: usize, high: usize)`,
/// then a size range of `[low..high)` is the result.
impl From<(usize, usize)> for SizeRange {
  fn from((low, high): (usize, usize)) -> Self {
    size_range(low..high)
  }
}

/// Given `exact`, then a size range of `[exact, exact]` is the result.
impl From<usize> for SizeRange {
  fn from(exact: usize) -> Self {
    size_range(exact..=exact)
  }
}

/// Given `..high`, then a size range `[0, high)` is the result.
impl From<RangeTo<usize>> for SizeRange {
  fn from(high: RangeTo<usize>) -> Self {
    size_range(0..high.end)
  }
}

/// Given `low .. high`, then a size range `[low, high)` is the result.
impl From<Range<usize>> for SizeRange {
  fn from(range: Range<usize>) -> Self {
    Self(range)
  }
}

/// Given `low ..= high`, then a size range `[low, high]` is the result.
impl From<RangeInclusive<usize>> for SizeRange {
  fn from(range: RangeInclusive<usize>) -> Self {
    size_range(*range.start()..range.end().saturating_add(1))
  }
}

/// Given `..=high`, then a size range `[0, high]` is the result.
impl From<RangeToInclusive<usize>> for SizeRange {
  fn from(high: RangeToInclusive<usize>) -> Self {
    size_range(0..=high.end)
  }
}

impl From<SizeRange> for Range<usize> {
  fn from(size_range: SizeRange) -> Self {
    size_range.0
  }
}

/// Adds `usize` to both start and end of the bounds, saturating each endpoint
/// at `usize::MAX`.
impl Add<usize> for SizeRange {
  type Output = Self;

  fn add(self, rhs: usize) -> Self::Output {
    let (lower_bound, upper_bound) = self.start_end_incl();
    let start = lower_bound.saturating_add(rhs);
    let end = upper_bound.saturating_add(rhs);
    size_range(start..=end)
  }
}

//==============================================================================
// Strategies
//==============================================================================

/// Strategy to create `Vec`s with a length in a certain range.
///
/// Created by the `vec()` function in the same module.
#[must_use = "strategies do nothing unless used"]
#[derive(Clone, Debug)]
pub struct VecStrategy<T: Strategy> {
  /// Strategy each generated element is drawn from.
  element: T,
  /// Range constraining the generated `Vec`'s length.
  size:    SizeRange,
}

/// Create a strategy to generate `Vec`s containing elements drawn from
/// `element` and with a size range given by `size`.
///
/// To make a `Vec` with a fixed number of elements, each with its own
/// strategy, you can instead make a `Vec` of strategies (boxed if necessary).
pub fn vec<T: Strategy>(element: T, size: impl Into<SizeRange>) -> VecStrategy<T> {
  let size_range = size.into();
  VecStrategy {
    element,
    size: size_range,
  }
}

/// Fallible form of [`vec()`]: returns a typed [`EmptySizeRange`] error instead
/// of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
pub fn try_vec<T: Strategy>(element: T, size: impl Into<SizeRange>) -> Result<VecStrategy<T>, EmptySizeRange> {
  let size_range = size.into();
  size_range.ensure_nonempty()?;
  Ok(VecStrategy {
    element,
    size: size_range,
  })
}

mapfn! {
    [] fn VecToDeque[<T : fmt::Debug>](vec: Vec<T>) -> VecDeque<T> {
        vec.into()
    }
}

opaque_strategy_wrapper! {
    /// Strategy to create `VecDeque`s with a length in a certain range.
    ///
    /// Created by the `vec_deque()` function in the same module.
    #[derive(Clone, Debug)]
    pub struct VecDequeStrategy[<T>][where T : Strategy](
        statics::Map<VecStrategy<T>, VecToDeque>)
        -> VecDequeValueTree<T::Tree>;
    /// `ValueTree` corresponding to `VecDequeStrategy`.
    #[derive(Clone, Debug)]
    pub struct VecDequeValueTree[<T>][where T : ValueTree](
        statics::Map<VecValueTree<T>, VecToDeque>)
        -> VecDeque<T::Value>;
}

/// Create a strategy to generate `VecDeque`s containing elements drawn from
/// `element` and with a size range given by `size`.
pub fn vec_deque<T: Strategy>(element: T, size: impl Into<SizeRange>) -> VecDequeStrategy<T> {
  VecDequeStrategy(statics::Map::new(vec(element, size), VecToDeque))
}

/// Fallible form of [`vec_deque`]: returns a typed [`EmptySizeRange`] error
/// instead of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
#[allow(
  clippy::single_call_fn,
  reason = "the public deque constructor owns typed size validation independently of infallible strategy construction"
)]
pub fn try_vec_deque<T: Strategy>(element: T, size: impl Into<SizeRange>) -> Result<VecDequeStrategy<T>, EmptySizeRange> {
  Ok(VecDequeStrategy(statics::Map::new(try_vec(element, size)?, VecToDeque)))
}

mapfn! {
    [] fn VecToLl[<T : fmt::Debug>](vec: Vec<T>) -> LinkedList<T> {
        vec.into_iter().collect()
    }
}

opaque_strategy_wrapper! {
    /// Strategy to create `LinkedList`s with a length in a certain range.
    ///
    /// Created by the `linkedlist()` function in the same module.
    #[derive(Clone, Debug)]
    pub struct LinkedListStrategy[<T>][where T : Strategy](
        statics::Map<VecStrategy<T>, VecToLl>)
        -> LinkedListValueTree<T::Tree>;
    /// `ValueTree` corresponding to `LinkedListStrategy`.
    #[derive(Clone, Debug)]
    pub struct LinkedListValueTree[<T>][where T : ValueTree](
        statics::Map<VecValueTree<T>, VecToLl>)
        -> LinkedList<T::Value>;
}

/// Create a strategy to generate `LinkedList`s containing elements drawn from
/// `element` and with a size range given by `size`.
pub fn linked_list<T: Strategy>(element: T, size: impl Into<SizeRange>) -> LinkedListStrategy<T> {
  LinkedListStrategy(statics::Map::new(vec(element, size), VecToLl))
}

/// Fallible form of [`linked_list`]: returns a typed [`EmptySizeRange`] error
/// instead of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
#[allow(
  clippy::single_call_fn,
  reason = "the public linked list constructor owns typed size validation independently of infallible strategy construction"
)]
pub fn try_linked_list<T: Strategy>(element: T, size: impl Into<SizeRange>) -> Result<LinkedListStrategy<T>, EmptySizeRange> {
  Ok(LinkedListStrategy(statics::Map::new(try_vec(element, size)?, VecToLl)))
}

mapfn! {
    [] fn VecToBinHeap[<T : fmt::Debug + Ord>](vec: Vec<T>) -> BinaryHeap<T> {
        vec.into()
    }
}

opaque_strategy_wrapper! {
    /// Strategy to create `BinaryHeap`s with a length in a certain range.
    ///
    /// Created by the `binary_heap()` function in the same module.
    #[derive(Clone, Debug)]
    pub struct BinaryHeapStrategy[<T>][where T : Strategy, T::Value : Ord](
        statics::Map<VecStrategy<T>, VecToBinHeap>)
        -> BinaryHeapValueTree<T::Tree>;
    /// `ValueTree` corresponding to `BinaryHeapStrategy`.
    #[derive(Clone, Debug)]
    pub struct BinaryHeapValueTree[<T>][where T : ValueTree, T::Value : Ord](
        statics::Map<VecValueTree<T>, VecToBinHeap>)
        -> BinaryHeap<T::Value>;
}

/// Create a strategy to generate `BinaryHeap`s containing elements drawn from
/// `element` and with a size range given by `size`.
pub fn binary_heap<T: Strategy>(element: T, size: impl Into<SizeRange>) -> BinaryHeapStrategy<T>
where
  T::Value: Ord,
{
  BinaryHeapStrategy(statics::Map::new(vec(element, size), VecToBinHeap))
}

/// Fallible form of [`binary_heap`]: returns a typed [`EmptySizeRange`] error
/// instead of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
#[allow(
  clippy::single_call_fn,
  reason = "the public binary heap constructor owns typed size validation independently of infallible strategy construction"
)]
pub fn try_binary_heap<T: Strategy>(element: T, size: impl Into<SizeRange>) -> Result<BinaryHeapStrategy<T>, EmptySizeRange>
where
  T::Value: Ord,
{
  Ok(BinaryHeapStrategy(statics::Map::new(try_vec(element, size)?, VecToBinHeap)))
}

mapfn! {
    {#[cfg(feature = "std")]}
    [] fn VecToHashSet[<T : fmt::Debug + Hash + Eq>](vec: Vec<T>)
                                                     -> HashSet<T> {
        vec.into_iter().collect()
    }
}

/// Minimum-size filter predicate shared by the set and map strategies.
///
/// Carries the required minimum element count so a `statics::Filter` can reject
/// collections that fell below it when duplicate keys collapsed the length.
#[derive(Debug, Clone, Copy)]
struct MinSize(usize);

/// Collect generated elements and reject collections whose duplicate keys
/// reduce their length below the source strategy's minimum size.
fn distinct_collection<T, F>(
  source: VecStrategy<T>,
  collect: F,
  reason: &'static str,
) -> statics::Filter<statics::Map<VecStrategy<T>, F>, MinSize>
where
  T: Strategy,
  F: statics::MapFn<Vec<T::Value>>,
  MinSize: statics::FilterFn<F::Output>,
{
  let minimum = MinSize(source.size.start());
  statics::Filter::new(statics::Map::new(source, collect), reason.into(), minimum)
}

#[cfg(feature = "std")]
impl<T: Eq + Hash> statics::FilterFn<HashSet<T>> for MinSize {
  fn apply(&self, set: &HashSet<T>) -> bool {
    set.len() >= self.0
  }
}

opaque_strategy_wrapper! {
    {#[cfg(feature = "std")]}
    {#[cfg_attr(docsrs, doc(cfg(feature = "std")))]}
    /// Strategy to create `HashSet`s with a length in a certain range.
    ///
    /// Created by the `hash_set()` function in the same module.
    #[derive(Clone, Debug)]
    pub struct HashSetStrategy[<T>][where T : Strategy, T::Value : Hash + Eq](
        statics::Filter<statics::Map<VecStrategy<T>, VecToHashSet>, MinSize>)
        -> HashSetValueTree<T::Tree>;
    /// `ValueTree` corresponding to `HashSetStrategy`.
    #[derive(Clone, Debug)]
    pub struct HashSetValueTree[<T>][where T : ValueTree, T::Value : Hash + Eq](
        statics::Filter<statics::Map<VecValueTree<T>, VecToHashSet>, MinSize>)
        -> HashSet<T::Value>;
}

/// Create a strategy to generate `HashSet`s containing elements drawn from
/// `element` and with a size range given by `size`.
///
/// This strategy will implicitly do local rejects to ensure that the `HashSet`
/// has at least the minimum number of elements, in case `element` should
/// produce duplicate values.
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub fn hash_set<T: Strategy>(element: T, size: impl Into<SizeRange>) -> HashSetStrategy<T>
where
  T::Value: Hash + Eq,
{
  HashSetStrategy(distinct_collection(vec(element, size), VecToHashSet, "HashSet minimum size"))
}

/// Fallible form of [`hash_set`]: returns a typed [`EmptySizeRange`] error
/// instead of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[allow(
  clippy::single_call_fn,
  reason = "the public hash set constructor owns typed size validation independently of infallible strategy construction"
)]
pub fn try_hash_set<T: Strategy>(element: T, size: impl Into<SizeRange>) -> Result<HashSetStrategy<T>, EmptySizeRange>
where
  T::Value: Hash + Eq,
{
  Ok(HashSetStrategy(distinct_collection(
    try_vec(element, size)?,
    VecToHashSet,
    "HashSet minimum size",
  )))
}

mapfn! {
    [] fn VecToBTreeSet[<T : fmt::Debug + Ord>](vec: Vec<T>)
                                                -> BTreeSet<T> {
        vec.into_iter().collect()
    }
}

impl<T: Ord> statics::FilterFn<BTreeSet<T>> for MinSize {
  fn apply(&self, set: &BTreeSet<T>) -> bool {
    set.len() >= self.0
  }
}

opaque_strategy_wrapper! {
    /// Strategy to create `BTreeSet`s with a length in a certain range.
    ///
    /// Created by the `btree_set()` function in the same module.
    #[derive(Clone, Debug)]
    pub struct BTreeSetStrategy[<T>][where T : Strategy, T::Value : Ord](
        statics::Filter<statics::Map<VecStrategy<T>, VecToBTreeSet>, MinSize>)
        -> BTreeSetValueTree<T::Tree>;
    /// `ValueTree` corresponding to `BTreeSetStrategy`.
    #[derive(Clone, Debug)]
    pub struct BTreeSetValueTree[<T>][where T : ValueTree, T::Value : Ord](
        statics::Filter<statics::Map<VecValueTree<T>, VecToBTreeSet>, MinSize>)
        -> BTreeSet<T::Value>;
}

/// Create a strategy to generate `BTreeSet`s containing elements drawn from
/// `element` and with a size range given by `size`.
///
/// This strategy will implicitly do local rejects to ensure that the
/// `BTreeSet` has at least the minimum number of elements, in case `element`
/// should produce duplicate values.
pub fn btree_set<T: Strategy>(element: T, size: impl Into<SizeRange>) -> BTreeSetStrategy<T>
where
  T::Value: Ord,
{
  BTreeSetStrategy(distinct_collection(vec(element, size), VecToBTreeSet, "BTreeSet minimum size"))
}

/// Fallible form of [`btree_set`]: returns a typed [`EmptySizeRange`] error
/// instead of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
#[allow(
  clippy::single_call_fn,
  reason = "the public ordered set constructor owns typed size validation independently of infallible strategy construction"
)]
pub fn try_btree_set<T: Strategy>(element: T, size: impl Into<SizeRange>) -> Result<BTreeSetStrategy<T>, EmptySizeRange>
where
  T::Value: Ord,
{
  Ok(BTreeSetStrategy(distinct_collection(
    try_vec(element, size)?,
    VecToBTreeSet,
    "BTreeSet minimum size",
  )))
}

mapfn! {
    {#[cfg(feature = "std")]}
    [] fn VecToHashMap[<K : fmt::Debug + Hash + Eq, V : fmt::Debug>]
        (vec: Vec<(K, V)>) -> HashMap<K, V>
    {
        vec.into_iter().collect()
    }
}

#[cfg(feature = "std")]
impl<K: Hash + Eq, V> statics::FilterFn<HashMap<K, V>> for MinSize {
  fn apply(&self, map: &HashMap<K, V>) -> bool {
    map.len() >= self.0
  }
}

opaque_strategy_wrapper! {
    {#[cfg(feature = "std")]}
    {#[cfg_attr(docsrs, doc(cfg(feature = "std")))]}
    /// Strategy to create `HashMap`s with a length in a certain range.
    ///
    /// Created by the `hash_map()` function in the same module.
    #[derive(Clone, Debug)]
    pub struct HashMapStrategy[<K, V>]
        [where K : Strategy, V : Strategy, K::Value : Hash + Eq](
            statics::Filter<statics::Map<VecStrategy<(K,V)>,
            VecToHashMap>, MinSize>)
        -> HashMapValueTree<K::Tree, V::Tree>;
    /// `ValueTree` corresponding to `HashMapStrategy`.
    #[derive(Clone, Debug)]
    pub struct HashMapValueTree[<K, V>]
        [where K : ValueTree, V : ValueTree, K::Value : Hash + Eq](
            statics::Filter<statics::Map<VecValueTree<TupleValueTree<(K, V)>>,
            VecToHashMap>, MinSize>)
        -> HashMap<K::Value, V::Value>;
}

/// Create a strategy to generate `HashMap`s containing keys and values drawn
/// from `key` and `value` respectively, and with a size within the given
/// range.
///
/// This strategy will implicitly do local rejects to ensure that the `HashMap`
/// has at least the minimum number of elements, in case `key` should produce
/// duplicate values.
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub fn hash_map<K: Strategy, V: Strategy>(key: K, value_strategy: V, size: impl Into<SizeRange>) -> HashMapStrategy<K, V>
where
  K::Value: Hash + Eq,
{
  HashMapStrategy(distinct_collection(
    vec((key, value_strategy), size),
    VecToHashMap,
    "HashMap minimum size",
  ))
}

/// Fallible form of [`hash_map()`]: returns a typed [`EmptySizeRange`] error
/// instead of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[allow(
  clippy::single_call_fn,
  reason = "the public hash map constructor owns typed size validation independently of infallible strategy construction"
)]
pub fn try_hash_map<K: Strategy, V: Strategy>(
  key: K,
  value_strategy: V,
  size: impl Into<SizeRange>,
) -> Result<HashMapStrategy<K, V>, EmptySizeRange>
where
  K::Value: Hash + Eq,
{
  Ok(HashMapStrategy(distinct_collection(
    try_vec((key, value_strategy), size)?,
    VecToHashMap,
    "HashMap minimum size",
  )))
}

mapfn! {
    [] fn VecToBTreeMap[<K : fmt::Debug + Ord, V : fmt::Debug>]
        (vec: Vec<(K, V)>) -> BTreeMap<K, V>
    {
        vec.into_iter().collect()
    }
}

impl<K: Ord, V> statics::FilterFn<BTreeMap<K, V>> for MinSize {
  fn apply(&self, map: &BTreeMap<K, V>) -> bool {
    map.len() >= self.0
  }
}

opaque_strategy_wrapper! {
    /// Strategy to create `BTreeMap`s with a length in a certain range.
    ///
    /// Created by the `btree_map()` function in the same module.
    #[derive(Clone, Debug)]
    pub struct BTreeMapStrategy[<K, V>]
        [where K : Strategy, V : Strategy, K::Value : Ord](
            statics::Filter<statics::Map<VecStrategy<(K,V)>,
            VecToBTreeMap>, MinSize>)
        -> BTreeMapValueTree<K::Tree, V::Tree>;
    /// `ValueTree` corresponding to `BTreeMapStrategy`.
    #[derive(Clone, Debug)]
    pub struct BTreeMapValueTree[<K, V>]
        [where K : ValueTree, V : ValueTree, K::Value : Ord](
            statics::Filter<statics::Map<VecValueTree<TupleValueTree<(K, V)>>,
            VecToBTreeMap>, MinSize>)
        -> BTreeMap<K::Value, V::Value>;
}

/// Create a strategy to generate `BTreeMap`s containing keys and values drawn
/// from `key` and `value` respectively, and with a size within the given
/// range.
///
/// This strategy will implicitly do local rejects to ensure that the
/// `BTreeMap` has at least the minimum number of elements, in case `key`
/// should produce duplicate values.
pub fn btree_map<K: Strategy, V: Strategy>(key: K, value_strategy: V, size: impl Into<SizeRange>) -> BTreeMapStrategy<K, V>
where
  K::Value: Ord,
{
  BTreeMapStrategy(distinct_collection(
    vec((key, value_strategy), size),
    VecToBTreeMap,
    "BTreeMap minimum size",
  ))
}

/// Fallible form of [`btree_map`]: returns a typed [`EmptySizeRange`] error
/// instead of panicking when `size` is an empty range.
///
/// # Errors
///
/// Returns [`EmptySizeRange`] when `size` resolves to an empty range such as
/// `0..0`.
#[allow(
  clippy::single_call_fn,
  reason = "the public ordered map constructor owns typed size validation independently of infallible strategy construction"
)]
pub fn try_btree_map<K: Strategy, V: Strategy>(
  key: K,
  value_strategy: V,
  size: impl Into<SizeRange>,
) -> Result<BTreeMapStrategy<K, V>, EmptySizeRange>
where
  K::Value: Ord,
{
  Ok(BTreeMapStrategy(distinct_collection(
    try_vec((key, value_strategy), size)?,
    VecToBTreeMap,
    "BTreeMap minimum size",
  )))
}

/// The shrink operation a `VecValueTree` will attempt next.
#[derive(Clone, Copy, Debug)]
enum Shrink {
  /// Drop the element at the given index from the output.
  DeleteElement(usize),
  /// Shrink the element at the given index in place.
  ShrinkElement(usize),
}

/// `ValueTree` corresponding to `VecStrategy`.
#[derive(Clone, Debug)]
pub struct VecValueTree<T: ValueTree> {
  /// Value trees for every generated element, including excluded ones.
  elements:          Vec<T>,
  /// Which element indices currently contribute to the output.
  included_elements: VarBitSet,
  /// Fewest elements shrinking is allowed to leave included.
  min_size:          usize,
  /// The shrink operation to try on the next `simplify()`.
  shrink:            Shrink,
  /// The last applied shrink, undone in reverse by `complicate()`.
  prev_shrink:       Option<Shrink>,
}

impl<T: Strategy> Strategy for VecStrategy<T> {
  type Tree = VecValueTree<T::Tree>;
  type Value = Vec<T::Value>;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    self.size.ensure_nonempty().map_err(|error| Reason::from(error.to_string()))?;
    let (start, end) = self.size.start_end_incl();
    let max_size = sample_uniform_incl(runner, start, end)?;
    let mut elements = Vec::with_capacity(max_size);
    while elements.len() < max_size {
      elements.push(self.element.new_tree(runner)?);
    }

    Ok(VecValueTree {
      elements,
      included_elements: VarBitSet::saturated(max_size),
      min_size: start,
      shrink: Shrink::DeleteElement(0),
      prev_shrink: None,
    })
  }
}

impl<T: Strategy> Strategy for Vec<T> {
  type Tree = VecValueTree<T::Tree>;
  type Value = Vec<T::Value>;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    let len = self.len();
    let elements = self
      .iter()
      .map(|strategy| strategy.new_tree(runner))
      .collect::<Result<Vec<_>, Reason>>()?;

    Ok(VecValueTree {
      elements,
      included_elements: VarBitSet::saturated(len),
      min_size: len,
      shrink: Shrink::ShrinkElement(0),
      prev_shrink: None,
    })
  }
}

impl<T: ValueTree> ValueTree for VecValueTree<T> {
  type Value = Vec<T::Value>;

  fn current(&self) -> Vec<T::Value> {
    self
      .elements
      .iter()
      .enumerate()
      .filter(|&(ix, _)| self.included_elements.test(ix))
      .map(|(_, element)| element.current())
      .collect()
  }

  fn simplify(&mut self) -> bool {
    // The overall strategy here is to iteratively delete elements from the
    // list until we can do so no further, then to shrink each remaining
    // element in sequence.
    //
    // For `complicate()`, we simply undo the last shrink operation, if
    // there was any.
    if let Shrink::DeleteElement(ix) = self.shrink {
      // Can't delete an element if beyond the end of the vec or if it
      // would put us under the minimum length.
      if ix >= self.elements.len() || self.included_elements.count() == self.min_size {
        self.shrink = Shrink::ShrinkElement(0);
      } else {
        self.included_elements.clear(ix);
        self.prev_shrink = Some(self.shrink);
        self.shrink = Shrink::DeleteElement(ix.saturating_add(1));
        return true;
      }
    }

    while let Shrink::ShrinkElement(ix) = self.shrink {
      if ix >= self.elements.len() {
        // Nothing more we can do
        return false;
      }

      if !self.included_elements.test(ix) {
        // No use shrinking something we're not including.
        self.shrink = Shrink::ShrinkElement(ix.saturating_add(1));
        continue;
      }

      // A missing slot (impossible: `ix` is bounded by the element
      // count above) reads as "cannot simplify further".
      let simplified = self.elements.get_mut(ix).is_some_and(ValueTree::simplify);
      if simplified {
        self.prev_shrink = Some(self.shrink);
        return true;
      }
      // Move on to the next element.
      self.shrink = Shrink::ShrinkElement(ix.saturating_add(1));
    }

    false
  }

  fn complicate(&mut self) -> bool {
    match self.prev_shrink {
      None => false,
      Some(Shrink::DeleteElement(ix)) => {
        // Undo the last item we deleted. Can't complicate any further,
        // so unset prev_shrink.
        self.included_elements.set(ix);
        self.prev_shrink = None;
        true
      }
      Some(Shrink::ShrinkElement(ix)) => {
        // A missing slot (impossible: `ix` came from a simplify pass
        // over the same elements) reads as "cannot complicate".
        let complicated = self.elements.get_mut(ix).is_some_and(ValueTree::complicate);
        if complicated {
          // Don't unset prev_shrink; we may be able to complicate
          // again.
          true
        } else {
          // Can't complicate the last element any further.
          self.prev_shrink = None;
          false
        }
      }
    }
  }
}

//==============================================================================
// Tests
//==============================================================================

#[cfg(test)]
mod test {
  use std::string::ToString as _;
  use std::vec;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::bits;
  use crate::std_facade::Box;
  use crate::strategy::BoxedStrategy;
  use crate::strategy::Just;
  use crate::strategy::check_strategy_sanity;
  use crate::strategy::trace_shrink_steps;
  use crate::test_runner::TestCaseError;
  use crate::test_runner::TestCaseResult;
  use crate::test_runner::TestError;
  use crate::test_runner::test_runner_without_persistence;

  /// Native assertion subjects stay concrete and allocated on failure.
  type Check<S> = Result<(), Box<PredicateFailure<S>>>;
  /// Helpers retain their complete checked subjects on success as well as failure.
  type Checked<S> = Result<S, Box<PredicateFailure<S>>>;
  /// Native ordered maps share a strategy type for keys and values.
  type OrderedMap<S = Range<u8>> = BTreeMapStrategy<S, S>;
  /// Native hashed maps share a strategy type for keys and values.
  #[cfg(feature = "std")]
  type HashedMap<S = Range<u8>> = HashMapStrategy<S, S>;
  /// A collection constructor retains its concrete strategy or size error.
  type Construction<S> = Result<S, EmptySizeRange>;
  /// Deterministic generation preserves every native tree, shrink value, or reason.
  type Samples<S> = Vec<Result<(<S as Strategy>::Tree, Vec<<S as Strategy>::Value>), Reason>>;
  /// A constructed strategy and its native generation result.
  type Constructed<S> = Result<(S, NewTree<S>), EmptySizeRange>;
  /// All collection constructors retain their concrete strategy and error types.
  #[derive(Debug)]
  struct Constructors {
    /// Variable vector constructor and actual generated tree.
    vector:      Constructed<VecStrategy<Range<u8>>>,
    /// Deque constructor.
    deque:       Result<VecDequeStrategy<Range<u8>>, EmptySizeRange>,
    /// Linked-list constructor.
    list:        Result<LinkedListStrategy<Range<u8>>, EmptySizeRange>,
    /// Binary-heap constructor.
    heap:        Result<BinaryHeapStrategy<Range<u8>>, EmptySizeRange>,
    /// Ordered-set constructor.
    ordered_set: Result<BTreeSetStrategy<Range<u8>>, EmptySizeRange>,
    /// Ordered-map constructor.
    ordered_map: Construction<OrderedMap>,
    /// Hash-set constructor.
    #[cfg(feature = "std")]
    hashed_set:  Result<HashSetStrategy<Range<u8>>, EmptySizeRange>,
    /// Hash-map constructor.
    #[cfg(feature = "std")]
    hashed_map:  Construction<HashedMap>,
  }

  /// Exercise each public fallible constructor without erasing successful strategies.
  fn constructors(vector_size: Range<usize>, other_size: Range<usize>) -> Constructors {
    let mut runner = TestRunner::deterministic();
    Constructors {
      vector: try_vec(0_u8..4, vector_size).map(|strategy| {
        let tree = strategy.new_tree(&mut runner);
        (strategy, tree)
      }),
      deque: try_vec_deque(0_u8..4, other_size.clone()),
      list: try_linked_list(0_u8..4, other_size.clone()),
      heap: try_binary_heap(0_u8..4, other_size.clone()),
      ordered_set: try_btree_set(0_u8..4, other_size.clone()),
      #[cfg(feature = "std")]
      hashed_set: try_hash_set(0_u8..4, other_size.clone()),
      #[cfg(feature = "std")]
      hashed_map: try_hash_map(0_u8..4, 0_u8..4, other_size.clone()),
      ordered_map: try_btree_map(0_u8..4, 0_u8..4, other_size),
    }
  }

  #[test]
  fn try_constructors_accept_nonempty_size_ranges() -> Check<Constructors> {
    ensure_that(
      constructors(1..4, 1..4),
      "nonempty size ranges preserve successful constructors and a correctly sized generated vector",
      |observed| {
        let accepted = observed
          .vector
          .as_ref()
          .is_ok_and(|reached| reached.1.as_ref().is_ok_and(|tree| (1..4).contains(&tree.current().len())))
          && observed.deque.is_ok()
          && observed.list.is_ok()
          && observed.heap.is_ok()
          && observed.ordered_set.is_ok()
          && observed.ordered_map.is_ok();
        #[cfg(feature = "std")]
        {
          accepted && observed.hashed_set.is_ok() && observed.hashed_map.is_ok()
        }
        #[cfg(not(feature = "std"))]
        {
          accepted
        }
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn try_constructors_reject_empty_size_ranges() -> Check<Constructors> {
    ensure_that(
      constructors(3..3, 0..0),
      "empty ranges retain their concrete bounds and corrective diagnostic",
      |observed| {
        let empty = Some(&EmptySizeRange {
          start: 0, end_excl: 0
        });
        let rejected = observed.vector.as_ref().is_err_and(|error| {
          *error
            == EmptySizeRange {
              start: 3, end_excl: 3
            }
            && error
              .to_string()
              .contains("did you accidentally write 3..3 where you meant 3..=3")
        }) && observed.deque.as_ref().err() == empty
          && observed.list.as_ref().err() == empty
          && observed.heap.as_ref().err() == empty
          && observed.ordered_set.as_ref().err() == empty
          && observed.ordered_map.as_ref().err() == empty;
        #[cfg(feature = "std")]
        {
          rejected && observed.hashed_set.as_ref().err() == empty && observed.hashed_map.as_ref().err() == empty
        }
        #[cfg(not(feature = "std"))]
        {
          rejected
        }
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// The native one-case API deliberately receives its own legacy failure contract.
  #[allow(
    clippy::single_call_fn,
    reason = "the sum boundary names the failing property independently of generation and shrinking observations"
  )]
  fn reject_vec_sum_at_nine(generated: &[usize]) -> TestCaseResult {
    if generated.iter().copied().fold(0_usize, usize::saturating_add) >= 9 {
      Err(TestCaseError::fail("greater than 8"))
    } else {
      Ok(())
    }
  }

  /// Initial vector and the reached legacy one-case outcome.
  type VectorCase = Result<(Vec<usize>, Result<bool, TestError<Vec<usize>>>), Reason>;

  #[test]
  fn test_vec() -> Check<Vec<VectorCase>> {
    let input = vec(1_usize..20, 5..20);
    let mut runner = TestRunner::deterministic();
    let observations = (0..256)
      .map(|_| {
        input.new_tree(&mut runner).map(|case| {
          let start = case.current();
          let result = runner.run_one(case, |generated| reject_vec_sum_at_nine(&generated));
          (start, result)
        })
      })
      .collect();
    let minimal_vector = |observation: &VectorCase| {
      let Ok(ref reached) = *observation else {
        return false;
      };
      (5..20).contains(&reached.0.len())
        && reached.0.iter().copied().collect::<BTreeSet<_>>().len() >= 2
        && match reached.1 {
          Ok(true) => true,
          Err(TestError::Fail(_, ref minimal)) => {
            (5..=9).contains(&minimal.len()) && minimal.iter().copied().fold(0_usize, usize::saturating_add) == 9
          }
          Ok(false) | Err(TestError::Abort(_)) => false,
        }
    };
    ensure_that(
      observations,
      "generated vectors satisfy their shape and shrink to the minimal nine-sum counterexample",
      |subjects: &Vec<VectorCase>| {
        subjects.iter().all(minimal_vector)
          && subjects
            .iter()
            .any(|observation| matches!(*observation, Ok((_, Err(TestError::Fail(..))))))
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn test_vec_sanity() -> Result<(), Reason> {
    check_strategy_sanity(vec(0_i32..1000, 5..10), None)
  }

  /// Reached parallel vector tree and every native vector produced during shrinking.
  struct ParallelWalk {
    /// Keep the original heterogeneous value trees available to the caller.
    tree:   <Vec<BoxedStrategy<u32>> as Strategy>::Tree,
    /// Every observed candidate, in shrink order.
    values: Vec<Vec<u32>>,
  }

  impl fmt::Debug for ParallelWalk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      // Boxed value trees do not promise Debug, but their native current value does.
      formatter
        .debug_struct("ParallelWalk")
        .field("current", &self.tree.current())
        .field("values", &self.values)
        .finish_non_exhaustive()
    }
  }

  #[test]
  fn test_parallel_vec() -> Check<Vec<Result<ParallelWalk, Reason>>> {
    let input = vec![(1_u32..10).boxed(), bits::u32::masked(0xf0_u32).boxed()];
    let observations = (0..256)
      .map(|_| {
        let mut runner = test_runner_without_persistence();
        input.new_tree(&mut runner)
      })
      .map(|generated| {
        generated.map(|initial_tree| {
          let (tree, values) = trace_shrink_steps(initial_tree);
          ParallelWalk {
            tree,
            values,
          }
        })
      })
      .collect();
    let valid_parallel_walk = |observation: &Result<ParallelWalk, Reason>| {
      let Ok(ref walk) = *observation else {
        return false;
      };
      walk
        .values
        .iter()
        .all(|value| matches!(*value.as_slice(), [first, second] if (1..=10).contains(&first) && second & !0xf0 == 0))
    };
    ensure_that(
      observations,
      "parallel vectors keep their fixed length and both element strategies during shrinking",
      |subjects: &Vec<Result<ParallelWalk, Reason>>| subjects.iter().all(valid_parallel_walk),
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// Check collection cardinality throughout generation and shrinking while
  /// retaining every reached tree and value, including generation failures.
  fn check_collection_sizes<S: Strategy>(input: &S, expected_size: usize, length: impl Fn(&S::Value) -> usize) -> Checked<Samples<S>>
  where
    S::Tree: fmt::Debug,
  {
    let mut runner = TestRunner::deterministic();
    let observed = (0..256).map(|_| input.new_tree(&mut runner).map(trace_shrink_steps)).collect();
    ensure_that(
      observed,
      "collections preserve their requested cardinality through generation and shrinking despite duplicate keys",
      |subjects: &Samples<S>| {
        subjects.iter().all(|result| {
          result
            .as_ref()
            .is_ok_and(|walk| length(&walk.0.current()) == expected_size && walk.1.iter().all(|value| length(value) == expected_size))
        })
      },
    )
    .map_err(Box::new)
  }

  /// A strategy with one possible key cannot satisfy a two-key minimum;
  /// preserve its generation result when the local rejection budget is exhausted.
  fn check_collision_exhaustion<S: Strategy>(input: &S) -> Checked<NewTree<S>>
  where
    S::Tree: fmt::Debug,
  {
    let mut runner = TestRunner::new(Config {
      max_local_rejects: 4,
      ..Config::default()
    });
    ensure_that(
      input.new_tree(&mut runner),
      "duplicate keys cannot satisfy the collection minimum and exhaust the local rejection budget",
      |observed| observed.as_ref().err() == Some(&Reason::from("Too many local rejects")),
    )
    .map_err(Box::new)
  }

  #[cfg(feature = "std")]
  #[test]
  fn test_map() -> Check<Samples<HashMapStrategy<&'static str, &'static str>>> {
    check_collection_sizes(&hash_map("[ab]{3}", "a", 2..3), 2, HashMap::len).map(drop)
  }

  #[cfg(feature = "std")]
  #[test]
  fn test_set() -> Check<Samples<HashSetStrategy<&'static str>>> {
    check_collection_sizes(&hash_set("[ab]{3}", 2..3), 2, HashSet::len).map(drop)
  }

  #[test]
  fn ordered_map_preserves_size_while_shrinking() -> Check<Samples<OrderedMap>> {
    check_collection_sizes(&btree_map(0_u8..8, 0_u8..4, 2..3), 2, BTreeMap::len).map(drop)
  }

  #[test]
  fn ordered_set_preserves_size_while_shrinking() -> Check<Samples<BTreeSetStrategy<Range<u8>>>> {
    check_collection_sizes(&btree_set(0_u8..8, 2..3), 2, BTreeSet::len).map(drop)
  }

  #[test]
  fn ordered_map_rejects_unavoidable_key_collisions() -> Check<NewTree<OrderedMap<Just<u8>>>> {
    check_collision_exhaustion(&btree_map(Just(1_u8), Just(2_u8), 2)).map(drop)
  }

  #[test]
  fn ordered_set_rejects_unavoidable_key_collisions() -> Check<NewTree<BTreeSetStrategy<Just<u8>>>> {
    check_collision_exhaustion(&btree_set(Just(1_u8), 2)).map(drop)
  }

  #[cfg(feature = "std")]
  #[test]
  fn hashed_map_rejects_unavoidable_key_collisions() -> Check<NewTree<HashedMap<Just<u8>>>> {
    check_collision_exhaustion(&hash_map(Just(1_u8), Just(2_u8), 2)).map(drop)
  }

  #[cfg(feature = "std")]
  #[test]
  fn hashed_set_rejects_unavoidable_key_collisions() -> Check<NewTree<HashSetStrategy<Just<u8>>>> {
    check_collision_exhaustion(&hash_set(Just(1_u8), 2)).map(drop)
  }
}
