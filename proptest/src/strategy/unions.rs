//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::error::Error;
use core::mem;

#[cfg(all(not(feature = "std"), not(test)))]
use num_traits::MulAdd as _;
use num_traits::ToPrimitive as _;
#[cfg(all(not(feature = "std"), not(test)))]
use num_traits::float::FloatCore as _;

use crate::num::sample_uniform;
use crate::std_facade::Arc;
use crate::std_facade::Rc;
use crate::std_facade::Vec;
use crate::std_facade::fmt;
use crate::strategy::lazy::LazyValueTree;
use crate::strategy::traits::NewTree;
use crate::strategy::traits::Strategy;
use crate::strategy::traits::ValueTree;
use crate::test_runner::Reason;
use crate::test_runner::TestRunner;

/// A **relative** `weight` of a particular `Strategy` corresponding to `T`
/// coupled with `T` itself. The weight is currently given in `u32`.
pub type Weighted<T> = (u32, T);

/// A **relative** `weight` of a particular `Strategy` corresponding to `T`
/// coupled with `Rc<T>`. The weight is currently given in `u32`.
pub type WeightedStrategy<T> = (u32, Rc<T>);

/// A **relative** `weight` of a dynamic `Union` strategy branch coupled with
/// `Arc<T>` so the union remains compatible with thread-safe type erasure.
type WeightedUnionStrategy<T> = (u32, Arc<T>);

/// Error returned by the fallible [`Union`] constructors (and
/// [`try_float_to_weight`]) when the requested option set cannot form a valid
/// weighted union.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnionBuildError {
  /// No options were supplied; a union must have at least one.
  Empty,
  /// An option carried a relative weight of zero.
  ZeroWeight,
  /// The sum of all relative weights overflows a `u32`.
  WeightSumOverflow,
  /// A probability handed to [`try_float_to_weight`] was not a real number
  /// strictly between 0.0 and 1.0.
  InvalidProbability,
}

impl fmt::Display for UnionBuildError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match *self {
      Self::Empty => write!(f, "Union must have at least one option"),
      Self::ZeroWeight => write!(f, "Union option has a weight of 0"),
      Self::WeightSumOverflow => {
        write!(f, "Union weights overflow u32")
      }
      Self::InvalidProbability => {
        write!(f, "probability must be within (0.0, 1.0) exclusive")
      }
    }
  }
}

impl Error for UnionBuildError {}

/// Validate the relative weights of a prospective union: at least one option,
/// no zero weights, and a weight sum that fits in a `u32`.
#[allow(
  clippy::single_call_fn,
  reason = "check a union's relative weight table for zero weights, emptiness, and overflow"
)]
fn validate_weights(weights: impl Iterator<Item = u32>) -> Result<(), UnionBuildError> {
  let mut saw_weight = false;
  let mut sum = 0_u64;
  for weight in weights {
    if weight == 0 {
      return Err(UnionBuildError::ZeroWeight);
    }
    saw_weight = true;
    sum = sum.checked_add(u64::from(weight)).ok_or(UnionBuildError::WeightSumOverflow)?;
  }
  if !saw_weight {
    return Err(UnionBuildError::Empty);
  }
  if sum > u64::from(u32::MAX) {
    return Err(UnionBuildError::WeightSumOverflow);
  }
  Ok(())
}

/// A `Strategy` which picks from one of several delegate `Strategy`s.
///
/// See `Strategy::prop_union()`.
#[derive(Clone, Debug)]
#[must_use = "strategies do nothing unless used"]
pub struct Union<T: Strategy> {
  // In principle T could be any `Strategy + Clone`, but that isn't possible
  // for BC reasons with the 0.9 series.
  /// The weighted delegate strategies, each paired with its relative weight
  /// and wrapped in an `Arc`; one is picked per generated value.
  options: Vec<WeightedUnionStrategy<T>>,
}

impl<T: Strategy> Union<T> {
  /// Create a strategy which selects uniformly from the given delegate
  /// strategies.
  ///
  /// When shrinking, after maximal simplification of the chosen element, the
  /// strategy will move to earlier options and continue simplification with
  /// those.
  ///
  /// If `options` is empty, the resulting strategy reports a generation
  /// error from [`Strategy::new_tree`]. [`Union::try_new_uniform`] is the
  /// eager validation form.
  #[allow(
    clippy::single_call_fn,
    reason = "public uniform-union constructor names the infallible API next to the checked constructor"
  )]
  pub fn new(options: impl IntoIterator<Item = T>) -> Self {
    Self {
      options: options.into_iter().map(|strategy| (1, Arc::new(strategy))).collect(),
    }
  }

  /// Fallible form of [`Union::new`]: returns a typed error instead of
  /// panicking when `options` is empty.
  ///
  /// ## Errors
  ///
  /// Returns [`UnionBuildError::Empty`] if `options` yields no elements.
  #[allow(
    clippy::single_call_fn,
    reason = "a uniform-weight Union from an options iterator, erroring instead of panicking when empty"
  )]
  pub fn try_new_uniform(options: impl IntoIterator<Item = T>) -> Result<Self, UnionBuildError> {
    let weighted_options: Vec<WeightedUnionStrategy<T>> = options.into_iter().map(|strategy| (1, Arc::new(strategy))).collect();
    if weighted_options.is_empty() {
      return Err(UnionBuildError::Empty);
    }
    Ok(Self {
      options: weighted_options
    })
  }

  /// Create a strategy which selects from the given delegate strategies.
  ///
  /// Each strategy is assigned a non-zero weight which determines how
  /// frequently that strategy is chosen. For example, a strategy with a
  /// weight of 2 will be chosen twice as frequently as one with a weight of
  /// 1\.
  ///
  /// Empty or all-zero option lists report a generation error from
  /// [`Strategy::new_tree`]. [`Union::try_new_weighted`] is the eager
  /// validation form.
  pub fn new_weighted(options: Vec<Weighted<T>>) -> Self {
    Self {
      options: options
        .into_iter()
        .map(|(weight, strategy)| (weight, Arc::new(strategy)))
        .collect(),
    }
  }

  /// Fallible form of [`Union::new_weighted`]: returns a typed error
  /// instead of panicking when `options` is empty, an option's weight is
  /// zero, or the weight sum overflows a `u32`.
  ///
  /// ## Errors
  ///
  /// Returns [`UnionBuildError::Empty`] if `options` is empty,
  /// [`UnionBuildError::ZeroWeight`] if any option has a weight of zero, or
  /// [`UnionBuildError::WeightSumOverflow`] if the weights sum past `u32`.
  #[allow(
    clippy::single_call_fn,
    reason = "validate a weighted option table and build the Union without panicking on bad weights"
  )]
  pub fn try_new_weighted(options: Vec<Weighted<T>>) -> Result<Self, UnionBuildError> {
    validate_weights(options.iter().map(|&(weight, _)| weight))?;
    let shared_options = options
      .into_iter()
      .map(|(weight, strategy)| (weight, Arc::new(strategy)))
      .collect();
    Ok(Self {
      options: shared_options
    })
  }

  /// Add `other` as an additional alternate strategy with weight 1.
  pub fn or(mut self, other: T) -> Self {
    self.options.push((1, Arc::new(other)));
    self
  }
}

/// Randomly select an option index, biased by the relative `weights`.
///
/// The weights are supplied twice (`weights1`, `weights2`) because the
/// iterator is consumed once to compute their sum and once to locate the
/// chosen index.
///
/// ## Errors
///
/// Returns a `Reason` abort if every weight is zero, since sampling an empty
/// range would otherwise panic inside `rand`.
fn pick_weighted<I: Iterator<Item = u32>>(runner: &mut TestRunner, weights1: I, weights2: I) -> Result<usize, Reason> {
  let sum = weights1.map(u64::from).sum();
  if 0 == sum {
    // `TupleUnion` accepts arbitrary weights, so an all-zero tuple can
    // reach this point; sampling an empty range would panic inside rand.
    return Err("all union weights are zero".into());
  }
  let weighted_pick = sample_uniform(runner, 0, sum)?;
  Ok(
    weights2
      .scan(0_u64, |state, weight| {
        *state = state.saturating_add(u64::from(weight));
        Some(*state)
      })
      .filter(|&cumulative_weight| cumulative_weight <= weighted_pick)
      .count(),
  )
}

impl<T: Strategy> Strategy for Union<T> {
  type Tree = UnionValueTree<T>;
  type Value = T::Value;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    const fn extract_weight<V>(&(weight, _): &WeightedUnionStrategy<V>) -> u32 {
      weight
    }

    let pick = pick_weighted(
      runner,
      self.options.iter().map(extract_weight::<T>),
      self.options.iter().map(extract_weight::<T>),
    )?;

    let mut options = Vec::with_capacity(pick);

    // Delay initialization for all options less than pick.
    for option in self.options.iter().take(pick) {
      options.push(LazyValueTree::new_arc(Arc::clone(&option.1), runner));
    }

    // Initialize the tree at pick so at least one value is available. Note
    // that if generation for the value at pick fails, the entire strategy
    // will fail. This seems like the right call.
    //
    // `pick_weighted` returns the index of one of the summed options, so
    // the lookup cannot miss; an inconsistency aborts generation rather
    // than panicking.
    let picked = self
      .options
      .get(pick)
      .ok_or_else(|| Reason::from("union pick out of range (internal invariant)"))?;
    let current = picked.1.new_tree(runner)?;

    Ok(UnionValueTree {
      options,
      current,
      pick,
      min_pick: 0,
      prev: None,
    })
  }
}

/// `ValueTree` corresponding to `Union`.
pub struct UnionValueTree<T: Strategy> {
  /// Lazily generated value trees for options earlier than the current
  /// pick; only branches reached while shrinking are initialized.
  options:  Vec<LazyValueTree<T>>,
  /// The currently selected branch's initialized value tree.
  current:  T::Tree,
  // This struct maintains the invariant that between function calls,
  // `pick` and `prev` (if Some) always point to initialized trees.
  /// The index of the currently chosen option.
  pick:     usize,
  /// The lowest option index shrinking is still allowed to reach.
  min_pick: usize,
  /// The active option to complicate back to, set while walking to an
  /// earlier pick.
  prev:     Option<(usize, T::Tree)>,
}

impl<T: Strategy> ValueTree for UnionValueTree<T> {
  type Value = T::Value;

  fn current(&self) -> Self::Value {
    self.current.current()
  }

  fn simplify(&mut self) -> bool {
    if self.current.simplify() {
      self.prev = None;
      return true;
    }

    if self.pick <= self.min_pick {
      return false;
    }

    let mut next_pick = self.pick;
    while next_pick > self.min_pick {
      next_pick = next_pick.saturating_sub(1);
      let Some(option) = self.options.get_mut(next_pick) else {
        continue;
      };
      option.maybe_init();
      let Some(next) = option.take_initialized() else {
        continue;
      };

      let previous = mem::replace(&mut self.current, next);
      self.prev = Some((self.pick, previous));
      self.pick = next_pick;
      return true;
    }

    false
  }

  fn complicate(&mut self) -> bool {
    if let Some((pick, previous)) = self.prev.take() {
      self.current = previous;
      self.pick = pick;
      self.min_pick = pick;
      true
    } else {
      self.current.complicate()
    }
  }
}

impl<T: Strategy> Clone for UnionValueTree<T>
where
  T::Tree: Clone,
{
  fn clone(&self) -> Self {
    Self {
      options:  self.options.clone(),
      current:  self.current.clone(),
      pick:     self.pick,
      min_pick: self.min_pick,
      prev:     self.prev.clone(),
    }
  }
}

impl<T: Strategy> fmt::Debug for UnionValueTree<T>
where
  T::Tree: fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("UnionValueTree")
      .field("options", &self.options)
      .field("current", &self.current)
      .field("pick", &self.pick)
      .field("min_pick", &self.min_pick)
      .field("prev", &self.prev)
      .finish()
  }
}

/// Take an initialized lazy tuple slot, leaving `None` once the tree has moved
/// into the active branch.
fn take_tuple_slot<S: Strategy>(slot: &mut Option<LazyValueTree<S>>) -> Option<S::Tree> {
  let lazy = slot.as_mut()?;
  lazy.maybe_init();
  let tree = lazy.take_initialized();
  if tree.is_some() {
    *slot = None;
  }
  tree
}

/// Lazy tuple slots that can yield a typed active branch by index.
trait TupleUnionSlots {
  /// The active branch enum produced from one initialized tuple slot.
  type Active: ValueTree;

  /// Initialize and take the slot at `pick`, returning `None` if the slot is
  /// unavailable or failed generation.
  fn take_active_at(&mut self, pick: usize) -> Option<Self::Active>;
}

/// Implement an active-branch enum and lazy-slot access for one tuple arity.
macro_rules! tuple_union_active {
    (
        $active:ident,
        $first_variant:ident $first_gen:ident $first_ix:tt
        $(, $variant:ident $gen:ident $ix:tt)*
    ) => {
        /// Active branch value tree for one `TupleUnion` arity.
        #[derive(Clone, Copy, Debug)]
        pub enum $active<$first_gen, $($gen),*> {
            #[doc = concat!(
                "The active value tree for tuple-union slot ",
                stringify!($first_ix),
                "."
            )]
            $first_variant($first_gen),
            $(
            #[doc = concat!(
                "The active value tree for tuple-union slot ",
                stringify!($ix),
                "."
            )]
            $variant($gen),
            )*
        }

        impl<$first_gen, $($gen),*> ValueTree
            for $active<$first_gen, $($gen),*>
        where
            $first_gen: ValueTree,
            $($gen: ValueTree<Value = $first_gen::Value>),*
        {
            type Value = $first_gen::Value;

            fn current(&self) -> Self::Value {
                match *self {
                    Self::$first_variant(ref tree) => tree.current(),
                    $(
                    Self::$variant(ref tree) => tree.current(),
                    )*
                }
            }

            fn simplify(&mut self) -> bool {
                match *self {
                    Self::$first_variant(ref mut tree) => tree.simplify(),
                    $(
                    Self::$variant(ref mut tree) => tree.simplify(),
                    )*
                }
            }

            fn complicate(&mut self) -> bool {
                match *self {
                    Self::$first_variant(ref mut tree) => tree.complicate(),
                    $(
                    Self::$variant(ref mut tree) => tree.complicate(),
                    )*
                }
            }
        }

        impl<$first_gen: Strategy, $($gen: Strategy<Value = $first_gen::Value>),*>
            TupleUnionSlots
            for (
                Option<LazyValueTree<$first_gen>>,
                $(Option<LazyValueTree<$gen>>),*
            )
        {
            type Active = $active<$first_gen::Tree, $($gen::Tree),*>;

            fn take_active_at(&mut self, pick: usize) -> Option<Self::Active> {
                match pick {
                    $first_ix => take_tuple_slot(&mut self.$first_ix)
                        .map($active::$first_variant),
                    $(
                    $ix => take_tuple_slot(&mut self.$ix)
                        .map($active::$variant),
                    )*
                    _ => None,
                }
            }
        }
    };
}

tuple_union_active!(
    TupleUnionActive2,
    Slot0 A 0,
    Slot1 B 1
);
tuple_union_active!(
    TupleUnionActive3,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2
);
tuple_union_active!(
    TupleUnionActive4,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2,
    Slot3 D 3
);
tuple_union_active!(
    TupleUnionActive5,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2,
    Slot3 D 3,
    Slot4 E 4
);
tuple_union_active!(
    TupleUnionActive6,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2,
    Slot3 D 3,
    Slot4 E 4,
    Slot5 F 5
);
tuple_union_active!(
    TupleUnionActive7,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2,
    Slot3 D 3,
    Slot4 E 4,
    Slot5 F 5,
    Slot6 G 6
);
tuple_union_active!(
    TupleUnionActive8,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2,
    Slot3 D 3,
    Slot4 E 4,
    Slot5 F 5,
    Slot6 G 6,
    Slot7 H 7
);
tuple_union_active!(
    TupleUnionActive9,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2,
    Slot3 D 3,
    Slot4 E 4,
    Slot5 F 5,
    Slot6 G 6,
    Slot7 H 7,
    Slot8 I 8
);
tuple_union_active!(
    TupleUnionActiveA,
    Slot0 A 0,
    Slot1 B 1,
    Slot2 C 2,
    Slot3 D 3,
    Slot4 E 4,
    Slot5 F 5,
    Slot6 G 6,
    Slot7 H 7,
    Slot8 I 8,
    Slot9 J 9
);

/// Similar to `Union`, but internally uses a tuple to hold the strategies.
///
/// This allows better performance than vanilla `Union` since one does not need
/// to resort to boxing and dynamic dispatch to handle heterogeneous
/// strategies.
///
/// The difference between this and `TupleUnion` is that with this, value trees
/// for variants that aren't picked at first are generated lazily.
#[must_use = "strategies do nothing unless used"]
#[derive(Clone, Copy, Debug)]
pub struct TupleUnion<T>(T);

impl<T> TupleUnion<T> {
  /// Wrap `tuple` in a `TupleUnion`.
  ///
  /// The struct definition allows any `T` for `tuple`, but to be useful, it
  /// must be a 2- to 10-tuple of `(u32, Rc<impl Strategy>)` pairs where all
  /// strategies ultimately produce the same value. Each `u32` indicates the
  /// relative weight of its corresponding strategy.
  /// You may use `WeightedStrategy<S>` as an alias for `(u32, Rc<S>)`.
  ///
  /// Using this constructor directly is discouraged; prefer to use
  /// `prop_oneof!` since it is generally clearer.
  pub const fn new(tuple: T) -> Self {
    Self(tuple)
  }
}

/// Implement `Strategy` for a `TupleUnion` of a given arity, picking one slot
/// by weight and eagerly generating only that slot's value tree.
macro_rules! tuple_union {
    ($active:ident; $($gen:ident $variant:ident $ix:tt)*) => {
        impl<A : Strategy, $($gen: Strategy<Value = A::Value>),*>
        Strategy for TupleUnion<
            (WeightedStrategy<A>, $(WeightedStrategy<$gen>),*)
        > {
            type Tree = TupleUnionValueTree<
                (
                    Option<LazyValueTree<A>>,
                    $(Option<LazyValueTree<$gen>>),*
                ),
                $active<A::Tree, $($gen::Tree),*>,
            >;
            type Value = A::Value;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                let weights = [((self.0).0).0, $(((self.0).$ix).0),*];
                let pick = pick_weighted(runner, weights.iter().cloned(),
                                         weights.iter().cloned())?;
                let active = match pick {
                    0 => $active::Slot0(((self.0).0).1.new_tree(runner)?),
                    $(
                        $ix => $active::$variant(
                            ((self.0).$ix).1.new_tree(runner)?
                        ),
                    )*
                    _ => {
                        return Err(
                            "tuple union pick out of range (internal invariant)"
                                .into()
                        );
                    },
                };

                Ok(TupleUnionValueTree {
                    options: (
                        if 0 == pick {
                            None
                        } else {
                            Some(LazyValueTree::new(
                                Rc::clone(&((self.0).0).1), runner)
                            )
                        },
                        $(
                        if $ix == pick {
                            None
                        } else if $ix < pick {
                            Some(LazyValueTree::new(
                                    Rc::clone(&((self.0).$ix).1), runner))
                        } else {
                            None
                        }),*),
                    active,
                    pick,
                    min_pick: 0,
                    prev: None,
                })
            }
        }
    }
}

tuple_union!(TupleUnionActive2; B Slot1 1);
tuple_union!(TupleUnionActive3; B Slot1 1 C Slot2 2);
tuple_union!(TupleUnionActive4; B Slot1 1 C Slot2 2 D Slot3 3);
tuple_union!(TupleUnionActive5; B Slot1 1 C Slot2 2 D Slot3 3 E Slot4 4);
tuple_union!(
    TupleUnionActive6;
    B Slot1 1 C Slot2 2 D Slot3 3 E Slot4 4 F Slot5 5
);
tuple_union!(
    TupleUnionActive7;
    B Slot1 1 C Slot2 2 D Slot3 3 E Slot4 4 F Slot5 5 G Slot6 6
);
tuple_union!(
    TupleUnionActive8;
    B Slot1 1 C Slot2 2 D Slot3 3 E Slot4 4 F Slot5 5 G Slot6 6
    H Slot7 7
);
tuple_union!(
    TupleUnionActive9;
    B Slot1 1 C Slot2 2 D Slot3 3 E Slot4 4 F Slot5 5 G Slot6 6
    H Slot7 7 I Slot8 8
);
tuple_union!(
    TupleUnionActiveA;
    B Slot1 1 C Slot2 2 D Slot3 3 E Slot4 4 F Slot5 5 G Slot6 6
    H Slot7 7 I Slot8 8 J Slot9 9
);

/// `ValueTree` type produced by `TupleUnion`.
#[derive(Clone, Copy, Debug)]
pub struct TupleUnionValueTree<T, A> {
  /// The tuple of lazy per-option value trees that may be reached by future
  /// shrinking.
  options:  T,
  /// The currently selected branch's initialized value tree.
  active:   A,
  /// The index of the currently chosen option.
  pick:     usize,
  /// The lowest option index shrinking is still allowed to reach.
  min_pick: usize,
  /// The active option to complicate back to, set while walking to an
  /// earlier pick.
  prev:     Option<(usize, A)>,
}

impl<T, A> ValueTree for TupleUnionValueTree<T, A>
where
  T: TupleUnionSlots<Active = A>,
  A: ValueTree,
{
  type Value = A::Value;

  fn current(&self) -> Self::Value {
    self.active.current()
  }

  fn simplify(&mut self) -> bool {
    if self.active.simplify() {
      self.prev = None;
      return true;
    }

    if self.pick <= self.min_pick {
      return false;
    }

    let mut next_pick = self.pick;
    while next_pick > self.min_pick {
      next_pick = next_pick.saturating_sub(1);
      let Some(next) = self.options.take_active_at(next_pick) else {
        continue;
      };

      let previous = mem::replace(&mut self.active, next);
      self.prev = Some((self.pick, previous));
      self.pick = next_pick;
      return true;
    }

    false
  }

  fn complicate(&mut self) -> bool {
    if let Some((pick, previous)) = self.prev.take() {
      self.active = previous;
      self.pick = pick;
      self.min_pick = pick;
      true
    } else {
      self.active.complicate()
    }
  }
}

/// The total to which the two weights returned by `float_to_weight` always
/// sum, chosen so the pair never overflows a `u32`.
const WEIGHT_BASE: u32 = 0x8000_0000;

/// Convert a valid probability to its positive and negative union weights.
#[must_use]
fn checked_float_to_weight(f: f64) -> Option<(u32, u32)> {
  if !(f > 0.0 && f < 1.0) {
    return None;
  }

  // Clamp to 1..WEIGHT_BASE-1 so that we never produce a weight of 0.
  let pos = f
    .mul_add(f64::from(WEIGHT_BASE), 0.0)
    .round()
    .to_u32()
    .unwrap_or(WEIGHT_BASE)
    .clamp(1, WEIGHT_BASE.saturating_sub(1));
  let neg = WEIGHT_BASE.saturating_sub(pos);

  Some((pos, neg))
}

/// Convert a floating-point weight in the range (0.0,1.0) to a pair of weights
/// that can be used with `Union` and similar.
///
/// The first return value is the weight corresponding to `f`; the second
/// return value is the weight corresponding to `1.0 - f`.
///
/// This call does not make any guarantees as to what range of weights it may
/// produce, except that adding the two return values will never overflow a
/// `u32`. As such, it is generally not meaningful to combine any other weights
/// with the two returned.
///
/// Returns zero weights if `f` is not a real number between 0.0 and 1.0, both
/// exclusive. [`try_float_to_weight`] is the eager validation form.
#[must_use]
pub fn float_to_weight(f: f64) -> (u32, u32) {
  checked_float_to_weight(f).unwrap_or((0, 0))
}

/// Fallible form of [`float_to_weight`]: returns a typed error instead of
/// panicking when `f` is not a real number strictly between 0.0 and 1.0.
///
/// ## Errors
///
/// Returns [`UnionBuildError::InvalidProbability`] if `f` is not a real number
/// strictly between 0.0 and 1.0, both exclusive.
#[allow(
  clippy::single_call_fn,
  reason = "convert a (0,1) probability into the pos and neg union weight pair without panicking"
)]
pub fn try_float_to_weight(f: f64) -> Result<(u32, u32), UnionBuildError> {
  checked_float_to_weight(f).ok_or(UnionBuildError::InvalidProbability)
}

#[cfg(test)]
mod test {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_ok;
  use strict_test_support::ensure_some;

  use super::*;
  use crate::std_facade::Rc;
  use crate::std_facade::vec;
  use crate::strategy::CheckStrategySanityOptions;
  use crate::strategy::check_strategy_sanity;
  use crate::strategy::just::Just;
  #[cfg(feature = "std")]
  use crate::test_runner::TestCaseError;
  #[cfg(feature = "std")]
  use crate::test_runner::TestError;

  // FIXME(2018-06-01): figure out a way to run this test on no_std.
  // The problem is that the default seed is fixed and does not produce
  // enough passed tests. We need some universal source of non-determinism
  // for the seed, which is unlikely.
  #[cfg(feature = "std")]
  #[test]
  fn test_union() -> Result<(), TestFailure> {
    let input = (10_u32..20_u32).prop_union(30_u32..40_u32);
    // Expect that 25% of cases pass (left input happens to be < 15, and
    // left is chosen as initial value). Of the 75% that fail, 50% should
    // converge to 15 and 50% to 30 (the latter because the left is beneath
    // the passing threshold).
    let mut passed = 0;
    let mut converged_low = 0;
    let mut converged_high = 0;
    let mut runner = TestRunner::deterministic();
    for _ in 0..256 {
      let case = ensure_some(input.new_tree(&mut runner).ok(), "union strategy generates a value tree")?;
      let result = runner.run_one(case, |sample| match sample {
        0..=14 => Ok(()),
        _ => Err(TestCaseError::fail("at least 15")),
      });

      match result {
        Ok(true) => passed += 1,
        Err(TestError::Fail(_, 15)) => converged_low += 1,
        Err(TestError::Fail(_, 30)) => converged_high += 1,
        _ => {
          ensure(false, "run_one converges to one of the two minima")?;
        }
      }
    }

    ensure((32..=96).contains(&passed), "a plausible share of cases passed")?;
    ensure(
      (32..=160).contains(&converged_low),
      "a plausible share converged to the low minimum",
    )?;
    ensure(
      (32..=160).contains(&converged_high),
      "a plausible share converged to the high minimum",
    )
  }

  #[test]
  fn test_union_weighted() -> Result<(), TestFailure> {
    let input = Union::new_weighted(vec![(1, Just(0_usize)), (2, Just(1_usize)), (1, Just(2_usize))]);

    let mut counts = [0_usize, 0, 0];
    let mut runner = TestRunner::deterministic();
    for _ in 0..65536 {
      let generated = ensure_some(input.new_tree(&mut runner).ok(), "weighted union generates a value tree")?.current();
      let bucket = ensure_some(counts.get_mut(generated), "generated union value has a histogram bucket")?;
      *bucket = bucket.saturating_add(1);
    }

    let [first, second, third] = counts;
    ensure(first > 0, "the first option is chosen")?;
    ensure(third > 0, "the third option is chosen")?;
    ensure(
      second.saturating_mul(2) > first.saturating_mul(3),
      "the double-weighted option dominates the first",
    )?;
    ensure(
      second.saturating_mul(2) > third.saturating_mul(3),
      "the double-weighted option dominates the third",
    )
  }

  #[test]
  fn test_union_sanity() -> Result<(), Reason> {
    check_strategy_sanity(
      Union::new_weighted(vec![(1, 0_i32..100), (2, 200_i32..300), (1, 400_i32..500)]),
      None,
    )
  }

  // FIXME(2018-06-01): See note on `test_union`.
  #[cfg(feature = "std")]
  #[test]
  fn test_tuple_union() -> Result<(), TestFailure> {
    let input = TupleUnion::new(((1, Rc::new(10_u32..20_u32)), (1, Rc::new(30_u32..40_u32))));
    // Expect that 25% of cases pass (left input happens to be < 15, and
    // left is chosen as initial value). Of the 75% that fail, 50% should
    // converge to 15 and 50% to 30 (the latter because the left is beneath
    // the passing threshold).
    let mut passed = 0;
    let mut converged_low = 0;
    let mut converged_high = 0;
    let mut runner = TestRunner::deterministic();
    for _ in 0..256 {
      let case = ensure_some(input.new_tree(&mut runner).ok(), "tuple union generates a value tree")?;
      let result = runner.run_one(case, |sample| match sample {
        0..=14 => Ok(()),
        _ => Err(TestCaseError::fail("at least 15")),
      });

      match result {
        Ok(true) => passed += 1,
        Err(TestError::Fail(_, 15)) => converged_low += 1,
        Err(TestError::Fail(_, 30)) => converged_high += 1,
        _ => {
          ensure(false, "run_one converges to one of the two minima")?;
        }
      }
    }

    ensure((32..=96).contains(&passed), "a plausible share of cases passed")?;
    ensure(
      (32..=160).contains(&converged_low),
      "a plausible share converged to the low minimum",
    )?;
    ensure(
      (32..=160).contains(&converged_high),
      "a plausible share converged to the high minimum",
    )
  }

  #[test]
  fn test_tuple_union_weighting() -> Result<(), TestFailure> {
    let input = TupleUnion::new((
      (1, Rc::new(Just(0_usize))),
      (2, Rc::new(Just(1_usize))),
      (1, Rc::new(Just(2_usize))),
    ));

    let mut counts = [0_usize, 0, 0];
    let mut runner = TestRunner::deterministic();
    for _ in 0..65536 {
      let generated = ensure_some(input.new_tree(&mut runner).ok(), "weighted tuple union generates a value tree")?.current();
      let bucket = ensure_some(counts.get_mut(generated), "generated tuple union value has a histogram bucket")?;
      *bucket = bucket.saturating_add(1);
    }

    let [first, second, third] = counts;
    ensure(first > 0, "the first option is chosen")?;
    ensure(third > 0, "the third option is chosen")?;
    ensure(
      second.saturating_mul(2) > first.saturating_mul(3),
      "the double-weighted option dominates the first",
    )?;
    ensure(
      second.saturating_mul(2) > third.saturating_mul(3),
      "the double-weighted option dominates the third",
    )
  }

  #[test]
  fn test_tuple_union_all_sizes() -> Result<(), TestFailure> {
    let mut runner = TestRunner::deterministic();
    let strategy = Rc::new(1_i32..10);

    macro_rules! test {
            ($($part:expr),*) => {{
                let input = TupleUnion::new((
                    $((1, $part.clone())),*,
                    (1, Rc::new(Just(0_i32)))
                ));

                let mut pass = false;
                for _ in 0..1024 {
                    if 0 == ensure_some(
                        input.new_tree(&mut runner).ok(),
                        "tuple union generates a value tree",
                    )?
                    .current()
                    {
                        pass = true;
                        break;
                    }
                }

                ensure(pass, "the final option is eventually chosen")?;
            }}
        }

    test!(strategy); // 2
    test!(strategy, strategy); // 3
    test!(strategy, strategy, strategy); // 4
    test!(strategy, strategy, strategy, strategy); // 5
    test!(strategy, strategy, strategy, strategy, strategy); // 6
    test!(strategy, strategy, strategy, strategy, strategy, strategy); // 7
    test!(strategy, strategy, strategy, strategy, strategy, strategy, strategy); // 8
    test!(strategy, strategy, strategy, strategy, strategy, strategy, strategy, strategy); // 9
    test!(
      strategy, strategy, strategy, strategy, strategy, strategy, strategy, strategy, strategy
    ); // 10
    Ok(())
  }

  #[test]
  fn test_tuple_union_sanity() -> Result<(), Reason> {
    check_strategy_sanity(
      TupleUnion::new((
        (1, Rc::new(0_i32..100_i32)),
        (1, Rc::new(200_i32..1000_i32)),
        (1, Rc::new(2000_i32..3000_i32)),
      )),
      None,
    )
  }

  #[test]
  fn try_constructors_accept_valid_unions() -> Result<(), TestFailure> {
    ensure(
      Union::try_new_uniform(vec![Just(1_usize), Just(2_usize)]).is_ok(),
      "try_new_uniform accepts a non-empty option list",
    )?;
    ensure(
      Union::try_new_weighted(vec![(1, Just(0_usize)), (3, Just(1))]).is_ok(),
      "try_new_weighted accepts positive weights",
    )?;
    let (pos, neg) = ensure_ok(try_float_to_weight(0.25), "try_float_to_weight accepts a probability inside (0, 1)")?;
    ensure(
      pos > 0 && neg > 0 && pos.checked_add(neg).is_some(),
      "the produced weight pair is non-zero and does not overflow",
    )
  }

  #[test]
  fn try_constructors_reject_invalid_unions() -> Result<(), TestFailure> {
    ensure_eq(
      &ensure_some(
        Union::<Just<usize>>::try_new_uniform(vec![]).err(),
        "try_new_uniform rejects an empty option list",
      )?,
      &UnionBuildError::Empty,
      "the empty union error names the violated invariant",
    )?;
    ensure_eq(
      &ensure_some(
        Union::try_new_weighted(vec![(0, Just(0_usize))]).err(),
        "try_new_weighted rejects a zero weight",
      )?,
      &UnionBuildError::ZeroWeight,
      "the zero-weight error names the violated invariant",
    )?;
    ensure_eq(
      &ensure_some(
        Union::try_new_weighted(vec![(u32::MAX, Just(0_usize)), (u32::MAX, Just(1))]).err(),
        "try_new_weighted rejects an overflowing weight sum",
      )?,
      &UnionBuildError::WeightSumOverflow,
      "the overflow error names the violated invariant",
    )?;
    ensure_eq(
      &ensure_some(
        try_float_to_weight(1.5).err(),
        "try_float_to_weight rejects a probability outside (0, 1)",
      )?,
      &UnionBuildError::InvalidProbability,
      "the probability error names the violated invariant",
    )?;
    ensure(try_float_to_weight(f64::NAN).is_err(), "try_float_to_weight rejects NaN")
  }

  #[test]
  fn zero_weight_tuple_union_aborts_generation() -> Result<(), TestFailure> {
    let input = TupleUnion::new(((0, Rc::new(Just(0_usize))), (0, Rc::new(Just(1_usize)))));
    let mut runner = TestRunner::deterministic();
    ensure(
      input.new_tree(&mut runner).is_err(),
      "an all-zero-weight tuple union reports a generation error instead of panicking",
    )
  }

  /// Test that unions work even if local filtering causes errors.
  #[test]
  fn test_filter_union_sanity() -> Result<(), Reason> {
    let filter_strategy = (0_u32..256).prop_filter("!%5", |&sample| 0 != sample.rem_euclid(5));
    check_strategy_sanity(Union::new(vec![filter_strategy; 8]), Some(filter_sanity_options()))
  }

  /// Test that tuple unions work even if local filtering causes errors.
  #[test]
  fn test_filter_tuple_union_sanity() -> Result<(), Reason> {
    let filter_strategy = (0_u32..256).prop_filter("!%5", |&sample| 0 != sample.rem_euclid(5));
    check_strategy_sanity(
      TupleUnion::new((
        (1, Rc::new(filter_strategy.clone())),
        (1, Rc::new(filter_strategy.clone())),
        (1, Rc::new(filter_strategy.clone())),
        (1, Rc::new(filter_strategy.clone())),
      )),
      Some(filter_sanity_options()),
    )
  }

  fn filter_sanity_options() -> CheckStrategySanityOptions {
    CheckStrategySanityOptions {
      // Due to internal rejection sampling, `simplify()` can
      // converge back to what `complicate()` would do.
      strict_complicate_after_simplify: false,
      // Make failed filters return errors to test edge cases.
      error_on_local_rejects: true,
      ..CheckStrategySanityOptions::default()
    }
  }
}
