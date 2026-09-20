//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Modified versions of the normal strategy combinators which take specialised
//! traits instead of normal functions.
//!
//! This entire module is strictly a workaround until
//! <https://github.com/rust-lang/rfcs/pull/1522> and
//! <https://github.com/rust-lang/rfcs/pull/2071> are available in stable. It
//! allows naming types built on the combinators without resorting to dynamic
//! dispatch or causing `Arc` to allocate space for a function pointer.
//!
//! External code is discouraged from using this module directly. It is
//! deliberately not exposed in a convenient way (i.e., via the `Strategy`
//! trait itself), but is nonetheless exposed since external trait implementors
//! may face the same issues.
//!
//! **This module is subject to removal at some point after the language
//! features linked above become stable.**

use crate::std_facade::fmt;
use crate::strategy::filter::new_filtered_tree;
use crate::strategy::filter::recover_filtered_value;
use crate::strategy::traits::NewTree;
use crate::strategy::traits::Strategy;
use crate::strategy::traits::ValueTree;
use crate::test_runner::Reason;
use crate::test_runner::TestRunner;

//==============================================================================
// Filter
//==============================================================================

/// Essentially `Fn (&T) -> bool`.
pub trait FilterFn<T> {
  /// Test whether `subject` passes the filter.
  fn apply(&self, subject: &T) -> bool;
}

/// Static version of `strategy::Filter`.
#[derive(Clone)]
#[must_use = "strategies do nothing unless used"]
pub struct Filter<S, F> {
  /// The strategy or value tree whose values are being filtered.
  source: S,
  /// The reason recorded with the runner each time a value is rejected.
  whence: Reason,
  /// The `FilterFn` predicate deciding acceptance, stored by value rather
  /// than behind an `Arc`.
  fun:    F,
}

impl<S, F> Filter<S, F> {
  /// Adapt strategy `source` to reject values which do not pass `filter`,
  /// using `whence` as the reported reason/location.
  pub const fn new(source: S, whence: Reason, filter: F) -> Self {
    // NOTE: We don't use universal quantification R: Into<Reason>
    // since the module is not conveniently exposed.
    Self {
      source,
      whence,
      fun: filter,
    }
  }
}

impl<S: fmt::Debug, F> fmt::Debug for Filter<S, F> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Filter")
      .field("source", &self.source)
      .field("whence", &self.whence)
      .field("fun", &"<function>")
      .finish()
  }
}

impl<S: Strategy, F: FilterFn<S::Value> + Clone> Strategy for Filter<S, F> {
  type Tree = Filter<S::Tree, F>;
  type Value = S::Value;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    new_filtered_tree(&self.source, &self.whence, |candidate| self.fun.apply(candidate), runner)
      .map(|source| Filter::new(source, "unused".into(), self.fun.clone()))
  }
}

impl<S: ValueTree, F: FilterFn<S::Value>> Filter<S, F> {
  /// After the source shrinks, `complicate()` it back until the predicate
  /// accepts the current value again. If recovery fails, report that this
  /// shrink step produced no usable change.
  fn ensure_acceptable(&mut self) -> bool {
    recover_filtered_value(&mut self.source, |candidate| self.fun.apply(candidate))
  }
}

impl<S: ValueTree, F: FilterFn<S::Value>> ValueTree for Filter<S, F> {
  type Value = S::Value;

  fn current(&self) -> S::Value {
    self.source.current()
  }

  fn simplify(&mut self) -> bool {
    self.source.simplify() && self.ensure_acceptable()
  }

  fn complicate(&mut self) -> bool {
    self.source.complicate() && self.ensure_acceptable()
  }
}

//==============================================================================
// Map
//==============================================================================

/// Essentially `Fn (T) -> Output`.
pub trait MapFn<T> {
  /// The mapped value produced by [`MapFn::apply`].
  type Output: fmt::Debug;

  /// Map `T` to `Output`.
  fn apply(&self, subject: T) -> Self::Output;
}

/// Static version of `strategy::Map`.
#[derive(Clone)]
#[must_use = "strategies do nothing unless used"]
pub struct Map<S, F> {
  /// The strategy or value tree whose values are being mapped.
  source: S,
  /// The `MapFn` mapping function, stored by value rather than behind an
  /// `Arc`.
  fun:    F,
}

impl<S, F> Map<S, F> {
  /// Adapt strategy `source` by applying `fun` to values it produces.
  pub const fn new(source: S, fun: F) -> Self {
    Self {
      source,
      fun,
    }
  }
}

impl<S: fmt::Debug, F> fmt::Debug for Map<S, F> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Map")
      .field("source", &self.source)
      .field("fun", &"<function>")
      .finish()
  }
}

impl<S: Strategy, F: Clone + MapFn<S::Value>> Strategy for Map<S, F> {
  type Tree = Map<S::Tree, F>;
  type Value = F::Output;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    self.source.new_tree(runner).map(|tree| Map {
      source: tree,
      fun:    self.fun.clone(),
    })
  }
}

impl<S: ValueTree, F: MapFn<S::Value>> ValueTree for Map<S, F> {
  type Value = F::Output;

  fn current(&self) -> F::Output {
    self.fun.apply(self.source.current())
  }

  fn simplify(&mut self) -> bool {
    self.source.simplify()
  }

  fn complicate(&mut self) -> bool {
    self.source.complicate()
  }
}

impl<I, O: fmt::Debug> MapFn<I> for fn(I) -> O {
  type Output = O;
  fn apply(&self, x: I) -> Self::Output {
    self(x)
  }
}

/// Function-pointer mapper stored by [`static_map`].
type StaticMapFn<S, O> = fn(<S as Strategy>::Value) -> O;

/// Wrap `strat` in a `statics::Map` that applies the function pointer `fun`,
/// letting callers name the resulting type without dynamic dispatch.
pub(crate) fn static_map<S: Strategy, O: fmt::Debug>(strat: S, fun: StaticMapFn<S, O>) -> Map<S, StaticMapFn<S, O>> {
  Map::new(strat, fun)
}

//==============================================================================
// Tests
//==============================================================================

#[cfg(test)]
mod test {
  #[cfg(feature = "strict-test")]
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Vec;
  use crate::strategy::traits::trace_simplifications;
  #[cfg(feature = "strict-test")]
  use crate::strict::ensure_property;
  #[cfg(feature = "strict-test")]
  use crate::test_runner::PropertyResult;
  use crate::test_runner::test_runner_without_persistence;

  #[test]
  fn test_static_filter() -> Result<(), impl fmt::Debug> {
    #[derive(Clone, Copy, Debug)]
    struct MyFilter;
    impl FilterFn<i32> for MyFilter {
      fn apply(&self, &candidate: &i32) -> bool {
        0 == candidate.rem_euclid(3)
      }
    }

    let input = Filter::new(0..256_i32, "%3".into(), MyFilter);

    let walks: Vec<_> = (0..256)
      .map(|_| {
        input
          .new_tree(&mut test_runner_without_persistence())
          .map(trace_simplifications)
      })
      .collect();
    ensure_that(
      walks,
      "the static filter preserves every generated, intermediate, and final survivor",
      |observed| {
        observed.iter().all(|walk| {
          walk
            .as_ref()
            .is_ok_and(|reached| reached.1.iter().all(|value| value.rem_euclid(3) == 0))
        })
      },
    )
    .map(drop)
  }

  #[cfg(feature = "strict-test")]
  #[test]
  fn test_static_map() -> PropertyResult<i32, i32, PredicateFailure<i32>> {
    #[derive(Clone, Copy, Debug)]
    struct MyMap;
    impl MapFn<i32> for MyMap {
      type Output = i32;
      fn apply(&self, element: i32) -> i32 {
        element.saturating_mul(2)
      }
    }

    let input = Map::new(0..10_i32, MyMap);

    ensure_property(&input, "the static map applies its function to every value", |mapped| {
      ensure_that(mapped, "the mapped value is even", |subject| subject.rem_euclid(2) == 0)
    })
  }
}
