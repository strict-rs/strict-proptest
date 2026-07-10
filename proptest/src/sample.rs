//-
// Copyright 2017, 2018 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for generating values by taking samples of collections.
//!
//! Note that the strategies in this module are not native combinators; that
//! is, the input collection is not itself a strategy, but is rather fixed when
//! the strategy is created.

use crate::std_facade::{Arc, Cow, Vec};
use core::fmt;
use core::ops::Range;

use rand::RngExt;

use crate::bits::{self, BitSetValueTree, SampledBitSetStrategy, VarBitSet};
use crate::num;
use crate::strategy::*;
use crate::test_runner::*;

/// Re-exported to make usage more ergonomic.
pub use crate::collection::{SizeRange, size_range};

/// Sample subsequences whose size are within `size` from the given collection
/// `values`.
///
/// A subsequence is a subset of the elements in a collection in the order they
/// occur in that collection. The elements are not chosen to be contiguous.
///
/// This is roughly analogous to `rand::sample`, except that it guarantees that
/// the order is preserved.
///
/// `values` may be a static slice or a `Vec`.
///
/// ## Panics
///
/// Panics if the maximum size implied by `size` is larger than the size of
/// `values`.
///
/// Panics if `size` is a zero-length range.
pub fn subsequence<T: Clone + 'static>(
    values: impl Into<Cow<'static, [T]>>,
    size: impl Into<SizeRange>,
) -> Subsequence<T> {
    match try_subsequence(values, size) {
        Ok(strategy) => strategy,
        Err(error) => panic!("{}", error),
    }
}

/// Error returned by [`try_subsequence`] when the requested size range cannot
/// select a subsequence of the input collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubsequenceError {
    /// The requested size range is empty.
    EmptySizeRange(crate::collection::EmptySizeRange),
    /// The requested maximum subsequence size exceeds the input length.
    TooLarge {
        /// Inclusive maximum of the requested size range.
        size_end_incl: usize,
        /// Length of the input collection.
        len: usize,
    },
}

impl fmt::Display for SubsequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySizeRange(inner) => inner.fmt(f),
            Self::TooLarge { size_end_incl, len } => write!(
                f,
                "Maximum size of subsequence {} exceeds length of input {}",
                size_end_incl, len
            ),
        }
    }
}

impl core::error::Error for SubsequenceError {}

/// Fallible form of [`subsequence`]: returns a typed error instead of
/// panicking when `size` is an empty range or exceeds the input length.
///
/// ## Errors
///
/// Returns `SubsequenceError::EmptySizeRange` when `size` is a zero-length
/// range, or `SubsequenceError::TooLarge` when the inclusive maximum of
/// `size` exceeds the length of `values`.
#[allow(
    clippy::single_call_fn,
    reason = "check the size bounds, then build the fallible order-preserving subsequence strategy"
)]
pub fn try_subsequence<T: Clone + 'static>(
    values: impl Into<Cow<'static, [T]>>,
    size: impl Into<SizeRange>,
) -> Result<Subsequence<T>, SubsequenceError> {
    let values = values.into();
    let len = values.len();
    let size = size.into();

    size.ensure_nonempty()
        .map_err(SubsequenceError::EmptySizeRange)?;
    if size.end_incl() > len {
        return Err(SubsequenceError::TooLarge {
            size_end_incl: size.end_incl(),
            len,
        });
    }
    Ok(Subsequence {
        values: Arc::new(values),
        bit_strategy: bits::varsize::sampled(size, 0..len),
    })
}

/// Strategy to generate `Vec`s by sampling a subsequence from another
/// collection.
///
/// This is created by the `subsequence` function in the same module.
#[derive(Debug, Clone)]
#[must_use = "strategies do nothing unless used"]
pub struct Subsequence<T: Clone + 'static> {
    /// Shared source collection that generated subsequences draw from.
    values: Arc<Cow<'static, [T]>>,
    /// Chooses which indices of `values` a generated subsequence keeps.
    bit_strategy: SampledBitSetStrategy<VarBitSet>,
}

impl<T: fmt::Debug + Clone + 'static> Strategy for Subsequence<T> {
    type Tree = SubsequenceValueTree<T>;
    type Value = Vec<T>;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        Ok(SubsequenceValueTree {
            values: Arc::clone(&self.values),
            inner: self.bit_strategy.new_tree(runner)?,
        })
    }
}

/// `ValueTree` type for `Subsequence`.
#[derive(Debug, Clone)]
pub struct SubsequenceValueTree<T: Clone + 'static> {
    /// Shared source collection the selected indices refer back into.
    values: Arc<Cow<'static, [T]>>,
    /// Set of chosen indices, driving both `current` and shrinking.
    inner: BitSetValueTree<VarBitSet>,
}

impl<T: fmt::Debug + Clone + 'static> ValueTree for SubsequenceValueTree<T> {
    type Value = Vec<T>;

    fn current(&self) -> Self::Value {
        let inner = self.inner.current();
        // The bit set was sized to `values`, so every index resolves; a
        // missing slot (impossible) is skipped rather than panicking.
        inner
            .iter()
            .filter_map(|ix| self.values.get(ix).cloned())
            .collect()
    }

    fn simplify(&mut self) -> bool {
        self.inner.simplify()
    }

    fn complicate(&mut self) -> bool {
        self.inner.complicate()
    }
}

/// `MapFn` turning a chosen index into the element at that index.
///
/// Backs `Select` by mapping the sampled `usize` onto a clone of the value
/// at that position in the captured collection.
#[derive(Debug, Clone)]
struct SelectMapFn<T: Clone + 'static>(Arc<Cow<'static, [T]>>);

impl<T: fmt::Debug + Clone + 'static> statics::MapFn<usize> for SelectMapFn<T> {
    type Output = T;

    fn apply(&self, ix: usize) -> T {
        self.0[ix].clone()
    }
}

opaque_strategy_wrapper! {
    /// Strategy to produce one value from a fixed collection of options.
    ///
    /// Created by the `select()` in the same module.
    #[derive(Clone, Debug)]
    pub struct Select[<T>][where T : Clone + fmt::Debug + 'static](
        statics::Map<Range<usize>, SelectMapFn<T>>)
        -> SelectValueTree<T>;
    /// `ValueTree` corresponding to `Select`.
    #[derive(Clone, Debug)]
    pub struct SelectValueTree[<T>][where T : Clone + fmt::Debug + 'static](
        statics::Map<num::usize::BinarySearch, SelectMapFn<T>>)
        -> T;
}

/// Create a strategy which uniformly selects one value from `values`.
///
/// `values` should be a `&'static [T]` or a `Vec<T>`, or potentially another
/// type that can be coerced to `Cow<'static,[T]>`.
///
/// This is largely equivalent to making a `Union` of a bunch of `Just`
/// strategies, but is substantially more efficient and shrinks by binary
/// search.
///
/// If `values` is also to be generated by a strategy, see
/// [`Index`](struct.Index.html) for a more efficient way to select values than
/// using `prop_flat_map()`.
///
/// ## Panics
///
/// Panics if `values` is empty. Use [`try_select`] to handle that case with
/// a typed error instead.
pub fn select<T: Clone + fmt::Debug + 'static>(
    values: impl Into<Cow<'static, [T]>>,
) -> Select<T> {
    match try_select(values) {
        Ok(strategy) => strategy,
        Err(error) => panic!("{}", error),
    }
}

/// Error returned by [`try_select`] when the input collection is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptySelection;

impl fmt::Display for EmptySelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cannot select from empty collection")
    }
}

impl core::error::Error for EmptySelection {}

/// Fallible form of [`select`]: returns a typed error instead of panicking
/// when `values` is empty.
///
/// ## Errors
///
/// Returns [`EmptySelection`] if `values` contains no elements.
#[allow(
    clippy::single_call_fn,
    reason = "reject an empty pool, then build the fallible uniform-selection strategy"
)]
pub fn try_select<T: Clone + fmt::Debug + 'static>(
    values: impl Into<Cow<'static, [T]>>,
) -> Result<Select<T>, EmptySelection> {
    let cow = values.into();

    if cow.is_empty() {
        return Err(EmptySelection);
    }

    Ok(Select(statics::Map::new(
        0..cow.len(),
        SelectMapFn(Arc::new(cow)),
    )))
}

/// A stand-in for an index into a slice or similar collection or conceptually
/// similar things.
///
/// At the lowest level, `Index` is a mechanism for generating `usize` values
/// in the range [0..N), for some N whose value is not known until it is
/// needed. (Contrast with using `0..N` itself as a strategy, where you need to
/// know N when you define the strategy.)
///
/// For any upper bound, the actual index produced by an `Index` is the same no
/// matter how many times it is used. Different upper bounds will produce
/// different but not independent values.
///
/// Shrinking will cause the index to binary search through the underlying
/// collection(s) it is used to sample.
///
/// Note that `Index` _cannot_ currently be used as a slice index (e.g.,
/// `slice[index]`) due to the trait coherence rules.
///
/// ## Example
///
/// If the collection itself being indexed is itself generated by a strategy,
/// you can make separately define that strategy and a strategy generating one
/// or more `Index`es and then join the two after input generation, avoiding a
/// call to `prop_flat_map()`.
///
/// ```
/// use proptest::prelude::*;
///
/// proptest! {
///     # /*
///     #[test]
///     # */
///     fn my_test(
///         names in prop::collection::vec("[a-z]+", 10..20),
///         indices in prop::collection::vec(any::<prop::sample::Index>(), 5..10)
///     ) {
///         // We now have Vec<String> of ten to twenty names, and a Vec<Index>
///         // of five to ten indices and can combine them however we like.
///         for index in &indices {
///             println!("Accessing item by index: {}", names[index.index(names.len())]);
///             println!("Accessing item by convenience method: {}", index.get(&names));
///         }
///         // Test stuff...
///     }
/// }
/// #
/// # fn main() { my_test(); }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Index(usize);

impl Index {
    /// Return the real index that would be used to index a collection of size `size`.
    ///
    /// ## Panics
    ///
    /// Panics if `size == 0`. [`Index::try_index`] is the fallible form.
    pub fn index(&self, size: usize) -> usize {
        match self.try_index(size) {
            Some(index) => index,
            None => panic!("Attempt to use `Index` with 0-size collection"),
        }
    }

    /// Fallible form of [`Index::index`]: returns `None` instead of panicking
    /// when `size == 0`.
    pub fn try_index(&self, size: usize) -> Option<usize> {
        if size == 0 {
            return None;
        }

        // No platforms currently have `usize` wider than 64 bits, so `u128` is
        // sufficient to hold the result of a full multiply, letting us do a
        // simple fixed-point multiply.
        Some(
            (((size as u128) * (self.0 as u128)) >> (size_of::<usize>() * 8))
                as usize,
        )
    }

    /// Return a reference to the element in `slice` that this `Index` refers to.
    ///
    /// A shortcut for `&slice[index.index(slice.len())]`.
    pub fn get<'a, T>(&self, slice: &'a [T]) -> &'a T {
        &slice[self.index(slice.len())]
    }

    /// Return a mutable reference to the element in `slice` that this `Index`
    /// refers to.
    ///
    /// A shortcut for `&mut slice[index.index(slice.len())]`.
    pub fn get_mut<'a, T>(&self, slice: &'a mut [T]) -> &'a mut T {
        let ix = self.index(slice.len());
        &mut slice[ix]
    }
}

// This impl is handy for generic code over any type that exposes an internal `Index` -- with it,
// a plain `Index` can be passed in as well.
impl AsRef<Index> for Index {
    fn as_ref(&self) -> &Index {
        self
    }
}

mapfn! {
    [] fn UsizeToIndex[](raw: usize) -> Index {
        Index(raw)
    }
}

opaque_strategy_wrapper! {
    /// Strategy to create `Index`es.
    ///
    /// Created via `any::<Index>()`.
    #[derive(Clone, Debug)]
    pub struct IndexStrategy[][](
        statics::Map<num::usize::Any, UsizeToIndex>)
        -> IndexValueTree;
    /// `ValueTree` corresponding to `IndexStrategy`.
    #[derive(Clone, Debug)]
    pub struct IndexValueTree[][](
        statics::Map<num::usize::BinarySearch,UsizeToIndex>)
        -> Index;
}

impl IndexStrategy {
    /// Create the strategy behind `any::<Index>()`.
    #[allow(
        clippy::single_call_fn,
        reason = "the deferred-bound Index strategy that backs any::<Index>()"
    )]
    pub(crate) fn new() -> Self {
        IndexStrategy(statics::Map::new(num::usize::ANY, UsizeToIndex))
    }
}

/// A value for picking random values out of iterators.
///
/// This is, in a sense, a more flexible variant of
/// [`Index`](struct.Index.html) in that it can operate on arbitrary
/// `IntoIterator` values.
///
/// Initially, the selection is roughly uniform, with a very slight bias
/// towards items earlier in the iterator.
///
/// Shrinking causes the selection to move toward items earlier in the
/// iterator, ultimately settling on the very first, but this currently happens
/// in a very haphazard way that may fail to find the earliest failing input.
///
/// ## Example
///
/// Generate a non-indexable collection and a value to pick out of it.
///
/// ```
/// use proptest::prelude::*;
///
/// proptest! {
///     # /*
///     #[test]
///     # */
///     fn my_test(
///         names in prop::collection::hash_set("[a-z]+", 10..20),
///         selector in any::<prop::sample::Selector>()
///     ) {
///         println!("Selected name: {}", selector.select(&names));
///         // Test stuff...
///     }
/// }
/// #
/// # fn main() { my_test(); }
/// ```
#[derive(Clone, Debug)]
pub struct Selector {
    /// RNG whose stream scores candidates, so a selection is reproducible.
    rng: TestRng,
    /// Per-position penalty that biases selection toward earlier elements.
    bias_increment: u64,
}

/// Strategy to create `Selector`s.
///
/// Created via `any::<Selector>()`.
///
/// Marked `#[non_exhaustive]` to reserve the right to add real configuration
/// later without a breaking change; the struct cannot be constructed outside
/// the crate.
#[derive(Debug)]
#[non_exhaustive]
pub struct SelectorStrategy;

/// `ValueTree` corresponding to `SelectorStrategy`.
#[derive(Debug)]
pub struct SelectorValueTree {
    /// RNG cloned into each produced `Selector` to score candidates.
    rng: TestRng,
    /// Shrink state for the selection bias, counted down from `u64::MAX`;
    /// simplifying it strengthens the pull toward earlier elements.
    reverse_bias_increment: num::u64::BinarySearch,
}

impl SelectorStrategy {
    /// Create the strategy behind `any::<Selector>()`.
    #[allow(
        clippy::single_call_fn,
        reason = "the iterator-selection strategy that backs any::<Selector>()"
    )]
    pub(crate) fn new() -> Self {
        SelectorStrategy
    }
}

impl Strategy for SelectorStrategy {
    type Tree = SelectorValueTree;
    type Value = Selector;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        Ok(SelectorValueTree {
            rng: runner.new_rng(),
            reverse_bias_increment: num::u64::BinarySearch::new(u64::MAX),
        })
    }
}

impl ValueTree for SelectorValueTree {
    type Value = Selector;

    fn current(&self) -> Selector {
        Selector {
            rng: self.rng.clone(),
            bias_increment: u64::MAX - self.reverse_bias_increment.current(),
        }
    }

    fn simplify(&mut self) -> bool {
        self.reverse_bias_increment.simplify()
    }

    fn complicate(&mut self) -> bool {
        self.reverse_bias_increment.complicate()
    }
}

impl Selector {
    /// Pick a random element from iterable `it`.
    ///
    /// The selection is unaffected by the elements themselves, and is
    /// dependent only on the actual length of `it`.
    ///
    /// `it` is always iterated completely.
    ///
    /// ## Panics
    ///
    /// Panics if `it` has no elements.
    pub fn select<T: IntoIterator>(&self, it: T) -> T::Item {
        self.try_select(it).expect("select from empty iterator")
    }

    /// Pick a random element from iterable `it`.
    ///
    /// Returns `None` if `it` is empty.
    ///
    /// The selection is unaffected by the elements themselves, and is
    /// dependent only on the actual length of `it`.
    ///
    /// `it` is always iterated completely.
    pub fn try_select<T: IntoIterator>(&self, it: T) -> Option<T::Item> {
        let mut bias = 0u64;
        let mut min_score = 0;
        let mut best = None;
        let mut rng = self.rng.clone();

        for candidate in it {
            let score = bias.saturating_add(rng.random());
            if best.is_none() || score < min_score {
                best = Some(candidate);
                min_score = score;
            }

            bias = bias.saturating_add(self.bias_increment);
        }

        best
    }
}

#[cfg(test)]
mod test {
    use crate::std_facade::{BTreeSet, vec};

    use strict_test_support::{TestFailure, ensure, ensure_eq, ensure_some};

    use super::*;
    use crate::arbitrary::any;

    #[test]
    fn strategy_construction_errors_are_copy() {
        use crate::bits::SampledBitsError;
        use crate::collection::EmptySizeRange;
        use crate::range_subset::RangeSubsetError;
        use crate::strategy::UnionBuildError;

        fn assert_copy<T: Copy>() {}
        assert_copy::<EmptySizeRange>();
        assert_copy::<UnionBuildError>();
        assert_copy::<EmptySelection>();
        assert_copy::<SubsequenceError>();
        assert_copy::<SampledBitsError>();
        assert_copy::<RangeSubsetError>();
    }

    #[test]
    fn try_constructors_accept_valid_inputs() -> Result<(), TestFailure> {
        let subsequence_strategy = ensure_some(
            try_subsequence(vec![1u8, 2, 3, 4], 1..3).ok(),
            "try_subsequence accepts a size range within the input length",
        )?;
        let mut runner = TestRunner::deterministic();
        let subsequence_value = ensure_some(
            subsequence_strategy.new_tree(&mut runner).ok(),
            "the fallibly constructed subsequence strategy generates",
        )?
        .current();
        ensure(
            (1..3).contains(&subsequence_value.len()),
            "the sampled subsequence honors the size range",
        )?;
        let select_strategy = ensure_some(
            try_select(vec![7u8, 8, 9]).ok(),
            "try_select accepts a non-empty collection",
        )?;
        let selected = ensure_some(
            select_strategy.new_tree(&mut runner).ok(),
            "the fallibly constructed select strategy generates",
        )?
        .current();
        ensure(
            [7u8, 8, 9].contains(&selected),
            "the selected value comes from the input collection",
        )?;
        let index = Index(usize::MAX / 2);
        ensure_eq(
            &ensure_some(
                index.try_index(10),
                "try_index accepts a non-zero collection size",
            )?,
            &index.index(10),
            "try_index agrees with the panicking form on valid sizes",
        )
    }

    #[test]
    fn try_constructors_reject_invalid_inputs() -> Result<(), TestFailure> {
        ensure(
            matches!(
                try_subsequence(vec![1u8, 2, 3], 2..2),
                Err(SubsequenceError::EmptySizeRange(_))
            ),
            "try_subsequence rejects an empty size range",
        )?;
        ensure_eq(
            &ensure_some(
                try_subsequence(vec![1u8, 2, 3], 1..=5).err(),
                "try_subsequence rejects a size range beyond the input",
            )?,
            &SubsequenceError::TooLarge {
                size_end_incl: 5,
                len: 3,
            },
            "the typed error names the requested size and input length",
        )?;
        ensure_eq(
            &ensure_some(
                try_select(Vec::<u8>::new()).err(),
                "try_select rejects an empty collection",
            )?,
            &EmptySelection,
            "the typed error names the empty selection",
        )?;
        ensure(
            Index(0).try_index(0).is_none(),
            "try_index reports a zero-size collection as None",
        )
    }

    #[test]
    fn sample_slice() -> Result<(), TestFailure> {
        static VALUES: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7];
        let mut size_counts = [0; 8];
        let mut value_counts = [0; 8];

        let mut runner = TestRunner::deterministic();
        let input = subsequence(VALUES, 3..7);

        for _ in 0..2048 {
            let value = ensure_some(
                input.new_tree(&mut runner).ok(),
                "subsequence strategy generates a value tree",
            )?
            .current();
            // Generated the correct number of items
            ensure(
                (3..7).contains(&value.len()),
                "the subsequence length stays within the requested range",
            )?;
            // Chose distinct items
            ensure_eq(
                &value.len(),
                &value.iter().cloned().collect::<BTreeSet<_>>().len(),
                "the subsequence contains only distinct items",
            )?;
            // Values are in correct order
            let mut sorted = value.clone();
            sorted.sort();
            ensure(
                sorted == value,
                "the subsequence preserves the source order",
            )?;

            size_counts[value.len()] += 1;

            for value in value {
                value_counts[value] += 1;
            }
        }

        for count in size_counts.iter().take(7).skip(3) {
            ensure(
                (256..1024).contains(count),
                "each size in the requested range is chosen a plausible \
                 number of times",
            )?;
        }

        for &pick_count in value_counts.iter() {
            ensure(
                (1024..1500).contains(&pick_count),
                "each value is chosen a plausible number of times",
            )?;
        }
        Ok(())
    }

    #[test]
    fn sample_vec() -> Result<(), TestFailure> {
        // Just test that the types work out
        let values = vec![0, 1, 2, 3, 4];

        let mut runner = TestRunner::deterministic();
        let input = subsequence(values, 1..3);

        let sample = ensure_some(
            input.new_tree(&mut runner).ok(),
            "subsequence strategy generates a value tree",
        )?
        .current();
        ensure(
            (1..3).contains(&sample.len()),
            "the sampled subsequence respects the requested size range",
        )
    }

    #[test]
    fn test_select() -> Result<(), TestFailure> {
        let values = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let mut counts = [0; 8];

        let mut runner = TestRunner::deterministic();
        let input = select(values);

        for _ in 0..1024 {
            counts[ensure_some(
                input.new_tree(&mut runner).ok(),
                "select strategy generates a value tree",
            )?
            .current()] += 1;
        }

        for &count in counts.iter() {
            ensure(
                (64..256).contains(&count),
                "each value is generated a plausible number of times",
            )?;
        }
        Ok(())
    }

    #[test]
    fn test_sample_sanity() {
        check_strategy_sanity(subsequence(vec![0, 1, 2, 3, 4], 1..3), None);
    }

    #[test]
    fn test_select_sanity() {
        check_strategy_sanity(select(vec![0, 1, 2, 3, 4]), None);
    }

    #[test]
    fn subseq_empty_vec_works() -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();
        let input = subsequence(Vec::<()>::new(), 0..1);
        ensure(
            Vec::<()>::new()
                == ensure_some(
                    input.new_tree(&mut runner).ok(),
                    "subsequence strategy generates a value tree",
                )?
                .current(),
            "an empty source yields the empty subsequence",
        )
    }

    #[test]
    fn subseq_full_vec_works() -> Result<(), TestFailure> {
        let source = vec![1u32, 2u32, 3u32];
        let mut runner = TestRunner::deterministic();
        let input = subsequence(source.clone(), 3);
        ensure(
            source
                == ensure_some(
                    input.new_tree(&mut runner).ok(),
                    "subsequence strategy generates a value tree",
                )?
                .current(),
            "a full-width subsequence covers the whole source",
        )
    }

    #[test]
    fn index_works() -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();
        let input = any::<Index>();
        let col = vec!["foo", "bar", "baz"];
        let mut seen = BTreeSet::new();

        for _ in 0..16 {
            let mut tree = ensure_some(
                input.new_tree(&mut runner).ok(),
                "index strategy generates a value tree",
            )?;
            let _was_new = seen.insert(*tree.current().get(&col));

            while tree.simplify() {}

            ensure_eq(
                &"foo",
                &*tree.current().get(&col),
                "a fully simplified index lands on the first element",
            )?;
        }

        ensure(
            col.into_iter().collect::<BTreeSet<_>>() == seen,
            "sampling touched every element",
        )
    }

    #[test]
    fn selector_works() -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();
        let input = any::<Selector>();
        let col: BTreeSet<&str> =
            vec!["foo", "bar", "baz"].into_iter().collect();
        let mut seen = BTreeSet::new();

        for _ in 0..16 {
            let mut tree = ensure_some(
                input.new_tree(&mut runner).ok(),
                "selector strategy generates a value tree",
            )?;
            let _was_new = seen.insert(*tree.current().select(&col));

            while tree.simplify() {}

            ensure_eq(
                &"bar",
                &*tree.current().select(&col),
                "a fully simplified selector lands on the first ordered \
                 element",
            )?;
        }

        ensure(col == seen, "selection touched every element")
    }
}
