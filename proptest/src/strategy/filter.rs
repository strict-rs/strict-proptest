//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{Arc, fmt};

use crate::strategy::traits::*;
use crate::test_runner::*;

/// `Strategy` and `ValueTree` filter adaptor.
///
/// See `Strategy::prop_filter()`.
#[must_use = "strategies do nothing unless used"]
pub struct Filter<S, F> {
    /// The strategy or value tree whose values are being filtered.
    pub(super) source: S,
    /// The reason recorded with the runner each time a value is rejected.
    pub(super) whence: Reason,
    /// The predicate deciding acceptance, held behind an `Arc` so the wrapper
    /// clones cheaply.
    pub(super) fun: Arc<F>,
}

impl<S, F> Filter<S, F> {
    /// Wrap `source` so that only values accepted by `fun` are produced,
    /// recording `whence` with the runner on each rejection.
    #[allow(
        clippy::single_call_fn,
        reason = "cache a Filter combinator's predicate and rejection reason behind an Arc"
    )]
    pub(super) fn new(source: S, whence: Reason, fun: F) -> Self {
        Self {
            source,
            whence,
            fun: Arc::new(fun),
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

impl<S: Clone, F> Clone for Filter<S, F> {
    fn clone(&self) -> Self {
        Filter {
            source: self.source.clone(),
            whence: "unused".into(),
            fun: Arc::clone(&self.fun),
        }
    }
}

impl<S: Strategy, F: Fn(&S::Value) -> bool> Strategy for Filter<S, F> {
    type Tree = Filter<S::Tree, F>;
    type Value = S::Value;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        loop {
            let source_tree = self.source.new_tree(runner)?;
            if !(self.fun)(&source_tree.current()) {
                runner.reject_local(self.whence.clone())?;
            } else {
                return Ok(Filter {
                    source: source_tree,
                    whence: self.whence.clone(),
                    fun: Arc::clone(&self.fun),
                });
            }
        }
    }
}

impl<S: ValueTree, F: Fn(&S::Value) -> bool> Filter<S, F> {
    /// After the source shrinks, `complicate()` it back until the predicate
    /// accepts the current value again.
    ///
    /// # Panics
    ///
    /// Panics if the source cannot be complicated back into an accepted
    /// value, which would indicate a broken source `ValueTree`.
    fn ensure_acceptable(&mut self) {
        while !(self.fun)(&self.source.current()) {
            if !self.source.complicate() {
                panic!(
                    "Unable to complicate filtered strategy \
                     back into acceptable value"
                );
            }
        }
    }
}

impl<S: ValueTree, F: Fn(&S::Value) -> bool> ValueTree for Filter<S, F> {
    type Value = S::Value;

    fn current(&self) -> S::Value {
        self.source.current()
    }

    fn simplify(&mut self) -> bool {
        if self.source.simplify() {
            self.ensure_acceptable();
            true
        } else {
            false
        }
    }

    fn complicate(&mut self) -> bool {
        if self.source.complicate() {
            self.ensure_acceptable();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use strict_test_support::{TestFailure, ensure, ensure_some};

    use super::*;

    #[test]
    fn test_filter() -> Result<(), TestFailure> {
        let input = (0..256).prop_filter("%3", |&candidate| 0 == candidate % 3);

        for _ in 0..256 {
            let mut runner = TestRunner::default();
            let mut case = ensure_some(
                input.new_tree(&mut runner).ok(),
                "filter strategy generates a value tree",
            )?;

            ensure(
                0 == case.current() % 3,
                "the generated value satisfies the filter",
            )?;

            while case.simplify() {
                ensure(
                    0 == case.current() % 3,
                    "every simplified value satisfies the filter",
                )?;
            }
            ensure(
                0 == case.current() % 3,
                "the fully simplified value satisfies the filter",
            )?;
        }
        Ok(())
    }

    #[test]
    fn test_filter_sanity() {
        check_strategy_sanity(
            (0..256).prop_filter("!%5", |&candidate| 0 != candidate % 5),
            Some(CheckStrategySanityOptions {
                // Due to internal rejection sampling, `simplify()` can
                // converge back to what `complicate()` would do.
                strict_complicate_after_simplify: false,
                ..CheckStrategySanityOptions::default()
            }),
        );
    }
}
