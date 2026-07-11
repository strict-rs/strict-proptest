//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::Arc;
use core::fmt;
use core::marker::PhantomData;

use crate::strategy::traits::{NewTree, Strategy, ValueTree};
use crate::test_runner::{TestRng, TestRunner};

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
    pub(super) fun: Arc<F>,
}

impl<S: fmt::Debug, F> fmt::Debug for Map<S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Map")
            .field("source", &self.source)
            .field("fun", &"<function>")
            .finish()
    }
}

impl<S: Clone, F> Clone for Map<S, F> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            fun: Arc::clone(&self.fun),
        }
    }
}

impl<S: Strategy, O: fmt::Debug, F: Fn(S::Value) -> O> Strategy for Map<S, F> {
    type Tree = Map<S::Tree, F>;
    type Value = O;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        self.source.new_tree(runner).map(|tree| Map {
            source: tree,
            fun: Arc::clone(&self.fun),
        })
    }
}

impl<S: ValueTree, O: fmt::Debug, F: Fn(S::Value) -> O> ValueTree
    for Map<S, F>
{
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

impl<S: fmt::Debug, O> fmt::Debug for MapInto<S, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapInto")
            .field("source", &self.source)
            .finish()
    }
}

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
    pub(super) fun: Arc<F>,
}

impl<S: fmt::Debug, F> fmt::Debug for Perturb<S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Perturb")
            .field("source", &self.source)
            .field("fun", &"<function>")
            .finish()
    }
}

impl<S: Clone, F> Clone for Perturb<S, F> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            fun: Arc::clone(&self.fun),
        }
    }
}

impl<S: Strategy, O: fmt::Debug, F: Fn(S::Value, TestRng) -> O> Strategy
    for Perturb<S, F>
{
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
    fun: Arc<F>,
    /// The generator snapshotted at `new_tree` time and cloned on every
    /// `current()`, so the perturbation stays stable across shrink steps.
    rng: TestRng,
}

impl<S: fmt::Debug, F> fmt::Debug for PerturbValueTree<S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PerturbValueTree")
            .field("source", &self.source)
            .field("fun", &"<function>")
            .field("rng", &self.rng)
            .finish()
    }
}

impl<S: Clone, F> Clone for PerturbValueTree<S, F> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            fun: Arc::clone(&self.fun),
            rng: self.rng.clone(),
        }
    }
}

impl<S: ValueTree, O: fmt::Debug, F: Fn(S::Value, TestRng) -> O> ValueTree
    for PerturbValueTree<S, F>
{
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
    use std::collections::HashSet;

    use rand::Rng as _;

    use strict_test_support::{TestFailure, ensure, ensure_eq, ensure_some};

    use super::*;
    use crate::strategy::just::Just;
    use crate::strict::ensure_property;
    use crate::test_runner::test_runner_without_persistence;

    #[test]
    fn test_map() -> Result<(), TestFailure> {
        ensure_property(
            &(0..10_i32).prop_map(|element| element * 2),
            "prop_map applies the mapping to every value",
            |mapped| {
                ensure(0 == mapped.rem_euclid(2), "the mapped value is even")
            },
        )
    }

    #[test]
    fn test_map_into() -> Result<(), TestFailure> {
        ensure_property(
            &(0..10_u8).prop_map_into::<usize>(),
            "prop_map_into converts every value",
            |converted| {
                ensure(converted < 10, "the converted value keeps its bound")
            },
        )
    }

    #[test]
    fn perturb_uses_same_rng_every_time() -> Result<(), TestFailure> {
        let mut runner = test_runner_without_persistence();
        let input =
            Just(1).prop_perturb(|element, mut rng| element + rng.next_u32());

        for _ in 0..16 {
            let value = ensure_some(
                input.new_tree(&mut runner).ok(),
                "perturb strategy generates a value tree",
            )?;
            ensure_eq(
                &value.current(),
                &value.current(),
                "current() is stable across calls",
            )?;
        }
        Ok(())
    }

    #[test]
    fn perturb_uses_varying_random_seeds() -> Result<(), TestFailure> {
        let mut runner = test_runner_without_persistence();
        let input =
            Just(1).prop_perturb(|element, mut rng| element + rng.next_u32());

        let mut seen = HashSet::new();
        for _ in 0..64 {
            let value = ensure_some(
                input.new_tree(&mut runner).ok(),
                "perturb strategy generates a value tree",
            )?
            .current();
            ensure(seen.insert(value), "each perturb seed is distinct")?;
        }

        ensure_eq(&64, &seen.len(), "every tree drew a distinct seed")
    }
}
