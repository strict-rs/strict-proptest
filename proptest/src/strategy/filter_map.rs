//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{Arc, fmt};

use crate::strategy::traits::{NewTree, Strategy, ValueTree};
#[cfg(test)]
use crate::strategy::{CheckStrategySanityOptions, check_strategy_sanity};
use crate::test_runner::{Reason, TestRunner};

/// `Strategy` and `ValueTree` `filter_map` adaptor.
///
/// See `Strategy::prop_filter_map()`.
#[must_use = "strategies do nothing unless used"]
pub struct FilterMap<S, F> {
    /// The strategy whose values are mapped and filtered.
    pub(super) source: S,
    /// The reason recorded with the runner each time a value is rejected.
    pub(super) whence: Reason,
    /// The closure mapping a source value to `Some(output)` or `None`, held
    /// behind an `Arc` so the wrapper clones cheaply.
    pub(super) fun: Arc<F>,
}

impl<S, F> FilterMap<S, F> {
    /// Wrap `source` so that only values `fun` maps to `Some` are produced,
    /// recording `whence` with the runner on each rejection.
    #[allow(
        clippy::single_call_fn,
        reason = "cache a FilterMap combinator's mapping closure and rejection reason"
    )]
    pub(super) fn new(source: S, whence: Reason, fun: F) -> Self {
        Self {
            source,
            whence,
            fun: Arc::new(fun),
        }
    }
}

impl<S: fmt::Debug, F> fmt::Debug for FilterMap<S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FilterMap")
            .field("source", &self.source)
            .field("whence", &self.whence)
            .field("fun", &"<function>")
            .finish()
    }
}

impl<S: Clone, F> Clone for FilterMap<S, F> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            whence: self.whence.clone(),
            fun: Arc::clone(&self.fun),
        }
    }
}

impl<S: Strategy, F: Fn(S::Value) -> Option<O>, O> Strategy for FilterMap<S, F>
where
    O: Clone + fmt::Debug,
{
    type Tree = FilterMapValueTree<S::Tree, F, O>;
    type Value = O;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        loop {
            let source_tree = self.source.new_tree(runner)?;
            if let Some(current) = (self.fun)(source_tree.current()) {
                return Ok(FilterMapValueTree {
                    source: source_tree,
                    current,
                    stalled: false,
                    fun: Arc::clone(&self.fun),
                });
            }
            runner.reject_local(self.whence.clone())?;
        }
    }
}

/// `ValueTree` corresponding to `FilterMap`.
pub struct FilterMapValueTree<V, F, O> {
    /// The source value tree being shrunk.
    source: V,
    /// The mapped output cached after the last accepted source state.
    current: O,
    /// Whether an attempted shrink moved the source into an unrecoverable
    /// rejected state. Once stalled, the tree keeps reporting the cached
    /// accepted output and stops changing.
    stalled: bool,
    /// The closure mapping a source value to `Some(output)` or `None`, held
    /// behind an `Arc` so the tree clones cheaply.
    fun: Arc<F>,
}

impl<V: Clone + ValueTree, F: Fn(V::Value) -> Option<O>, O> Clone
    for FilterMapValueTree<V, F, O>
where
    O: Clone,
{
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            current: self.current.clone(),
            stalled: self.stalled,
            fun: Arc::clone(&self.fun),
        }
    }
}

impl<V: fmt::Debug, F, O> fmt::Debug for FilterMapValueTree<V, F, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FilterMapValueTree")
            .field("source", &self.source)
            .field("current", &"<current>")
            .field("stalled", &self.stalled)
            .field("fun", &"<function>")
            .finish()
    }
}

impl<V: ValueTree, F: Fn(V::Value) -> Option<O>, O> FilterMapValueTree<V, F, O>
where
    O: Clone,
{
    /// Recompute the mapped output from the source's current value.
    fn fresh_current(&self) -> Option<O> {
        (self.fun)(self.source.current())
    }

    /// Record the mapped output as accepted.
    fn record_accepted_value(&mut self, current: O) {
        self.current = current;
    }

    /// After the source shrinks, `complicate()` it back until the closure maps
    /// the current value to `Some`, caching that output.
    /// If no accepted value can be recovered, leave the public value at the
    /// cached accepted output and report that this shrink step produced no
    /// usable change.
    fn ensure_acceptable(&mut self) -> bool {
        loop {
            if let Some(current) = (self.fun)(self.source.current()) {
                self.record_accepted_value(current);
                return true;
            }

            if !self.source.complicate() {
                self.stalled = true;
                return false;
            }
        }
    }
}

impl<V: ValueTree, F: Fn(V::Value) -> Option<O>, O> ValueTree
    for FilterMapValueTree<V, F, O>
where
    O: Clone + fmt::Debug,
{
    type Value = O;

    fn current(&self) -> O {
        if self.stalled {
            return self.current.clone();
        }

        self.fresh_current().unwrap_or_else(|| self.current.clone())
    }

    fn simplify(&mut self) -> bool {
        if self.stalled {
            return false;
        }
        self.source.simplify() && self.ensure_acceptable()
    }

    fn complicate(&mut self) -> bool {
        if self.stalled {
            return false;
        }
        self.source.complicate() && self.ensure_acceptable()
    }
}

#[cfg(test)]
mod test {
    use strict_test_support::{TestFailure, ensure_eq, ensure_some};

    use super::*;
    use crate::test_runner::test_runner_without_persistence;

    #[test]
    fn test_filter_map() -> Result<(), TestFailure> {
        let input = (0..256_i32).prop_filter_map("%3 + 1", |candidate| {
            (candidate.rem_euclid(3) == 0).then_some(candidate + 1)
        });

        for _ in 0..256 {
            let mut runner = test_runner_without_persistence();
            let mut case = ensure_some(
                input.new_tree(&mut runner).ok(),
                "filter_map strategy generates a value tree",
            )?;

            ensure_eq(
                &0,
                &(case.current() - 1).rem_euclid(3),
                "the generated value is a mapped survivor",
            )?;

            while case.simplify() {
                ensure_eq(
                    &0,
                    &(case.current() - 1).rem_euclid(3),
                    "every simplified value is a mapped survivor",
                )?;
            }
            ensure_eq(
                &0,
                &(case.current() - 1).rem_euclid(3),
                "the fully simplified value is a mapped survivor",
            )?;
        }
        Ok(())
    }

    #[test]
    fn test_filter_map_sanity() -> Result<(), Reason> {
        check_strategy_sanity(
            (0..256_i32).prop_filter_map("!%5 * 2", |candidate| {
                candidate
                    .rem_euclid(5)
                    .is_positive()
                    .then_some(candidate * 2)
            }),
            Some(CheckStrategySanityOptions {
                // Due to internal rejection sampling, `simplify()` can
                // converge back to what `complicate()` would do.
                strict_complicate_after_simplify: false,
                ..CheckStrategySanityOptions::default()
            }),
        )
    }
}
