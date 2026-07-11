//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for generating `bool` values.

#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::{NewTree, Strategy, ValueTree};
use crate::test_runner::TestRunner;

use rand::RngExt as _;

/// The type of the `ANY` constant.
#[derive(Clone, Copy, Debug)]
pub struct Any(());

/// Generates boolean values by picking `true` or `false` uniformly.
///
/// Shrinks `true` to `false`.
pub const ANY: Any = Any(());

impl Strategy for Any {
    type Tree = BoolValueTree;
    type Value = bool;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        Ok(BoolValueTree::new(runner.rng().random()))
    }
}

/// Generates boolean values by picking `true` with the given `probability`
/// (1.0 = always true, 0.0 = always false).
///
/// Shrinks `true` to `false`.
pub const fn weighted(probability: f64) -> Weighted {
    Weighted(probability)
}

/// The return type from `weighted()`.
#[must_use = "strategies do nothing unless used"]
#[derive(Clone, Copy, Debug)]
pub struct Weighted(f64);

impl Strategy for Weighted {
    type Tree = BoolValueTree;
    type Value = bool;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        Ok(BoolValueTree::new(runner.rng().random_bool(self.0)))
    }
}

/// The `ValueTree` to shrink booleans to false.
#[derive(Clone, Copy, Debug)]
pub struct BoolValueTree {
    /// The boolean this tree currently represents.
    current: bool,
    /// How far shrinking has progressed for this tree.
    state: ShrinkState,
}

/// Tracks how far a `BoolValueTree` has moved through its `true` → `false`
/// shrink.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ShrinkState {
    /// No shrink step has been taken yet.
    Untouched,
    /// The value was simplified from `true` to `false`.
    Simplified,
    /// Shrinking is exhausted; no further step will change the value.
    Final,
}

impl BoolValueTree {
    /// Creates a tree holding `current` with a fresh, untouched shrink state.
    const fn new(current: bool) -> Self {
        Self {
            current,
            state: ShrinkState::Untouched,
        }
    }
}

impl ValueTree for BoolValueTree {
    type Value = bool;

    fn current(&self) -> bool {
        self.current
    }
    fn simplify(&mut self) -> bool {
        match self.state {
            ShrinkState::Untouched if self.current => {
                self.current = false;
                self.state = ShrinkState::Simplified;
                true
            }

            ShrinkState::Untouched
            | ShrinkState::Simplified
            | ShrinkState::Final => {
                self.state = ShrinkState::Final;
                false
            }
        }
    }
    fn complicate(&mut self) -> bool {
        match self.state {
            ShrinkState::Untouched | ShrinkState::Final => {
                self.state = ShrinkState::Final;
                false
            }

            ShrinkState::Simplified => {
                self.current = true;
                self.state = ShrinkState::Final;
                true
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::test_runner::Reason;

    use strict_test_support::{TestFailure, ensure_all};

    use super::*;

    #[test]
    fn test_sanity() -> Result<(), Reason> {
        check_strategy_sanity(ANY, None)
    }

    #[test]
    fn shrinks_properly() -> Result<(), TestFailure> {
        let mut tree = BoolValueTree::new(true);
        ensure_all(&[
            (tree.simplify(), "true simplifies once"),
            (!tree.current(), "simplified tree reads false"),
            (!tree.clone().simplify(), "simplified tree cannot simplify"),
            (tree.complicate(), "simplified tree complicates back"),
            (
                !tree.clone().complicate(),
                "complicated tree cannot complicate again",
            ),
            (tree.current(), "complicated tree reads true"),
            (!tree.simplify(), "complicated tree cannot simplify"),
            (tree.current(), "tree still reads true"),
        ])?;

        tree = BoolValueTree::new(false);
        ensure_all(&[
            (!tree.clone().simplify(), "false cannot simplify"),
            (!tree.clone().complicate(), "false cannot complicate"),
            (!tree.current(), "false tree reads false"),
        ])
    }
}
