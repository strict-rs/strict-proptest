//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::fmt;
use core::marker::PhantomData;

use crate::std_facade::Arc;
use crate::strategy::traits::NewTree;
use crate::strategy::traits::Strategy;
use crate::strategy::traits::ValueTree;
use crate::test_runner::TestRng;
use crate::test_runner::TestRunner;

//==============================================================================
// Map
//==============================================================================

/// `Strategy` and `ValueTree` map adaptor.
///
/// See `Strategy::prop_map()`.
#[must_use = "strategies do nothing unless used"]
pub struct Map<S, F> {
  /// The strategy or value tree whose values are being mapped.
  pub(super) source: S,
  /// The mapping function, applied on every `current()` and held behind an
  /// `Arc` so the wrapper clones cheaply.
  pub(super) fun:    Arc<F>,
}

impl_debug_struct!(Map<S, F> [S: fmt::Debug] |self| {
  source: self.source,
  fun: "<function>",
});

impl_clone_shared_fn!(Map < S, F > |self| {});

impl<S: Strategy, O: fmt::Debug, F: Fn(S::Value) -> O> Strategy for Map<S, F> {
  type Tree = Map<S::Tree, F>;
  type Value = O;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    self.source.new_tree(runner).map(|tree| Map {
      source: tree,
      fun:    Arc::clone(&self.fun),
    })
  }
}

impl<S: ValueTree, O: fmt::Debug, F: Fn(S::Value) -> O> ValueTree for Map<S, F> {
  type Value = O;

  fn current(&self) -> O {
    (self.fun)(self.source.current())
  }

  fn simplify(&mut self) -> bool {
    self.source.simplify()
  }

  fn complicate(&mut self) -> bool {
    self.source.complicate()
  }
}

//==============================================================================
// MapInto
//==============================================================================

// NOTE: Since this is external stable API,
// we avoid relying on the Map in `statics`.

/// `Strategy` and `ValueTree` map into adaptor.
///
/// See `Strategy::prop_map_into()`.
#[must_use = "strategies do nothing unless used"]
pub struct MapInto<S, O> {
  /// The strategy or value tree whose values are converted via `Into`.
  pub(super) source: S,
  /// Marker recording the target type `O` the source values convert into.
  pub(super) output: PhantomData<O>,
}

impl<S, O> MapInto<S, O> {
  /// Construct a `MapInto` mapper from an `S` strategy into a strategy
  /// producing `O`s.
  pub(super) const fn new(source: S) -> Self {
    Self {
      source,
      output: PhantomData,
    }
  }
}

impl_debug_struct!(MapInto<S, O> [S: fmt::Debug] |self| {
  source: self.source,
});

impl<S: Clone, O> Clone for MapInto<S, O> {
  fn clone(&self) -> Self {
    Self::new(self.source.clone())
  }
}

impl<S: Strategy, O: fmt::Debug> Strategy for MapInto<S, O>
where
  S::Value: Into<O>,
{
  type Tree = MapInto<S::Tree, O>;
  type Value = O;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    self.source.new_tree(runner).map(MapInto::new)
  }
}

impl<S: ValueTree, O: fmt::Debug> ValueTree for MapInto<S, O>
where
  S::Value: Into<O>,
{
  type Value = O;

  fn current(&self) -> O {
    self.source.current().into()
  }

  fn simplify(&mut self) -> bool {
    self.source.simplify()
  }

  fn complicate(&mut self) -> bool {
    self.source.complicate()
  }
}

//==============================================================================
// Perturb
//==============================================================================

/// `Strategy` perturbation adaptor.
///
/// See `Strategy::prop_perturb()`.
#[must_use = "strategies do nothing unless used"]
pub struct Perturb<S, F> {
  /// The strategy whose values are perturbed.
  pub(super) source: S,
  /// The perturbation function, given the value and a random generator, held
  /// behind an `Arc` so the wrapper clones cheaply.
  pub(super) fun:    Arc<F>,
}

impl_debug_struct!(Perturb<S, F> [S: fmt::Debug] |self| {
  source: self.source,
  fun: "<function>",
});

impl_clone_shared_fn!(Perturb < S, F > |self| {});

impl<S: Strategy, O: fmt::Debug, F: Fn(S::Value, TestRng) -> O> Strategy for Perturb<S, F> {
  type Tree = PerturbValueTree<S::Tree, F>;
  type Value = O;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    let rng = runner.new_rng();

    self.source.new_tree(runner).map(|source| PerturbValueTree {
      source,
      rng,
      fun: Arc::clone(&self.fun),
    })
  }
}

/// `ValueTree` perturbation adaptor.
///
/// See `Strategy::prop_perturb()`.
pub struct PerturbValueTree<S, F> {
  /// The source value tree being shrunk.
  source: S,
  /// The perturbation function, held behind an `Arc` so the tree clones
  /// cheaply.
  fun:    Arc<F>,
  /// The generator snapshotted at `new_tree` time and cloned on every
  /// `current()`, so the perturbation stays stable across shrink steps.
  rng:    TestRng,
}

impl_debug_struct!(PerturbValueTree<S, F> [S: fmt::Debug] |self| {
  source: self.source,
  fun: "<function>",
  rng: self.rng,
});

impl_clone_shared_fn!(PerturbValueTree<S, F> |self| {
  rng: self.rng.clone(),
});

impl<S: ValueTree, O: fmt::Debug, F: Fn(S::Value, TestRng) -> O> ValueTree for PerturbValueTree<S, F> {
  type Value = O;

  fn current(&self) -> O {
    (self.fun)(self.source.current(), self.rng.clone())
  }

  fn simplify(&mut self) -> bool {
    self.source.simplify()
  }

  fn complicate(&mut self) -> bool {
    self.source.complicate()
  }
}

//==============================================================================
// Tests
//==============================================================================

#[cfg(test)]
mod test {
  use core::array;
  use std::collections::HashSet;

  use rand::Rng as _;
  #[cfg(feature = "strict-test")]
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Vec;
  use crate::std_facade::format;
  use crate::strategy::just::Just;
  #[cfg(feature = "strict-test")]
  use crate::strict::ensure_property;
  #[cfg(feature = "strict-test")]
  use crate::test_runner::PropertyResult;
  use crate::test_runner::Reason;
  use crate::test_runner::test_runner_without_persistence;

  /// Generated perturbation trees and every requested observation of their values.
  type PerturbSamples<T, const READS: usize> = Vec<Result<(T, [u32; READS]), Reason>>;

  /// Sample independently seeded trees while retaining repeated reads from each one.
  fn sample_perturb_trees<const READS: usize>(count: usize) -> PerturbSamples<impl ValueTree<Value = u32> + fmt::Debug, READS> {
    let mut runner = test_runner_without_persistence();
    let input = Just(1_u32).prop_perturb(|element, mut rng| element.wrapping_add(rng.next_u32()));
    (0..count)
      .map(|_| {
        input.new_tree(&mut runner).map(|tree| {
          let values = array::from_fn(|_| tree.current());
          (tree, values)
        })
      })
      .collect()
  }

  #[test]
  fn cloned_sources_are_independent_with_a_non_clone_function() -> Result<(), impl fmt::Debug> {
    use core::sync::atomic::AtomicU32;
    use core::sync::atomic::Ordering;

    let captured = AtomicU32::new(5);
    let input = Just(7_u32).prop_map(move |sample| sample.saturating_add(captured.load(Ordering::Relaxed)));
    let mut cloned = input.clone();
    cloned.source = Just(11);
    ensure_that(
      (input, cloned),
      "cloning permits a non-Clone function and gives each adapter an independent source",
      |observed| observed.0.current() == 12 && observed.1.current() == 16,
    )
    .map(drop)
  }

  #[test]
  fn debug_preserves_fields_without_requiring_closure_debug() -> Result<(), impl fmt::Debug> {
    let input = Just(7_u8).prop_map(|sample| sample.saturating_add(1));
    ensure_eq(
      [format!("{input:?}"), format!("{input:#?}")],
      [
        "Map { source: Just(7), fun: \"<function>\" }",
        "Map {\n    source: Just(\n        7,\n    ),\n    fun: \"<function>\",\n}",
      ],
      "compact and pretty diagnostics retain the source and opaque function field",
    )
    .map(drop)
  }

  #[test]
  fn debug_propagates_writer_failures() -> Result<(), impl fmt::Debug> {
    struct RejectWrites;

    impl fmt::Write for RejectWrites {
      fn write_str(&mut self, _: &str) -> fmt::Result {
        Err(fmt::Error)
      }
    }

    let input = Just(7_u8).prop_map(|sample| sample.saturating_add(1));
    ensure_eq(
      fmt::write(&mut RejectWrites, format_args!("{input:?}")),
      Err(fmt::Error),
      "an unavailable diagnostic sink remains a formatting failure",
    )
    .map(drop)
  }

  #[cfg(feature = "strict-test")]
  #[test]
  fn test_map() -> PropertyResult<i32, i32, PredicateFailure<i32>> {
    ensure_property(
      &(0..10_i32).prop_map(|element| element.saturating_mul(2)),
      "prop_map applies the mapping to every value",
      |mapped| ensure_that(mapped, "the mapped value is even", |subject| subject.rem_euclid(2) == 0),
    )
  }

  #[cfg(feature = "strict-test")]
  #[test]
  fn test_map_into() -> PropertyResult<usize, usize, PredicateFailure<usize>> {
    ensure_property(
      &(0..10_u8).prop_map_into::<usize>(),
      "prop_map_into converts every value",
      |converted| ensure_that(converted, "the converted value keeps its bound", |subject| *subject < 10),
    )
  }

  #[test]
  fn perturb_uses_same_rng_every_time() -> Result<(), impl fmt::Debug> {
    ensure_that(
      sample_perturb_trees::<2>(16),
      "current is stable across repeated calls on every perturb tree",
      |observed| {
        observed.iter().all(|sample| {
          sample.as_ref().is_ok_and(|reached| {
            let [first, second] = reached.1;
            first == second
          })
        })
      },
    )
    .map(drop)
  }

  #[test]
  fn perturb_uses_varying_random_seeds() -> Result<(), impl fmt::Debug> {
    ensure_that(
      sample_perturb_trees::<1>(64),
      "each of the 64 perturb trees draws a distinct seed",
      |observed| {
        observed.iter().all(Result::is_ok)
          && observed
            .iter()
            .filter_map(|sample| sample.as_ref().ok().map(|reached| &reached.1))
            .collect::<HashSet<_>>()
            .len()
            == 64
      },
    )
    .map(drop)
  }
}
