//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Support for combining strategies into tuples.
//!
//! There is no explicit "tuple strategy"; simply make a tuple containing the
//! strategy and that tuple is itself a strategy.

#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::strategy::{NewTree, Strategy, ValueTree};
use crate::test_runner::TestRunner;

/// Common `ValueTree` implementation for all tuple strategies.
#[derive(Clone, Copy, Debug)]
pub struct TupleValueTree<T> {
    /// The tuple of element `ValueTree`s being shrunk together.
    tree: T,
    /// Index of the element currently being simplified, advancing left to
    /// right as earlier elements stop shrinking.
    shrinker: u32,
    /// Element touched by the last `simplify`, so `complicate` can revisit
    /// it; `None` before any simplification.
    prev_shrinker: Option<u32>,
}

impl<T> TupleValueTree<T> {
    /// Create a new `TupleValueTree` wrapping `inner`.
    ///
    /// It only makes sense for `inner` to be a tuple of an arity for which the
    /// type implements `ValueTree`.
    pub const fn new(inner: T) -> Self {
        Self {
            tree: inner,
            shrinker: 0,
            prev_shrinker: None,
        }
    }
}

/// Implement `Strategy` and `ValueTree` for a tuple of a given arity.
///
/// Each invocation lists the tuple's field indices and type parameters; the
/// generated `TupleValueTree` shrinks the elements left to right.
macro_rules! tuple {
    ($($fld:tt : $typ:ident),*) => {
        impl<$($typ : Strategy),*> Strategy for ($($typ,)*) {
            type Tree = TupleValueTree<($($typ::Tree,)*)>;
            type Value = ($($typ::Value,)*);

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                let values = ($(self.$fld.new_tree(runner)?,)*);
                Ok(TupleValueTree::new(values))
            }
        }

        impl<$($typ : ValueTree),*> ValueTree
        for TupleValueTree<($($typ,)*)> {
            type Value = ($($typ::Value,)*);

            fn current(&self) -> Self::Value {
                ($(self.tree.$fld.current(),)*)
            }

            fn simplify(&mut self) -> bool {
                $(
                    if $fld == self.shrinker {
                        if self.tree.$fld.simplify() {
                            self.prev_shrinker = Some(self.shrinker);
                            return true;
                        }
                        self.shrinker =
                            self.shrinker.saturating_add(1);
                    }
                )*
                false
            }

            fn complicate(&mut self) -> bool {
                if let Some(shrinker) = self.prev_shrinker {$(
                    if $fld == shrinker {
                        if self.tree.$fld.complicate() {
                            self.shrinker = shrinker;
                            return true;
                        }
                        self.prev_shrinker = None;
                        return false;
                    }
                )*}
                false
            }
        }
    }
}

tuple!(0: A);
tuple!(0: A, 1: B);
tuple!(0: A, 1: B, 2: C);
tuple!(0: A, 1: B, 2: C, 3: D);
tuple!(0: A, 1: B, 2: C, 3: D, 4: E);
tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F);
tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G);
tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H);
tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I);
tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J);
tuple!(
    0: A,
    1: B,
    2: C,
    3: D,
    4: E,
    5: F,
    6: G,
    7: H,
    8: I,
    9: J,
    10: K
);
tuple!(
    0: A,
    1: B,
    2: C,
    3: D,
    4: E,
    5: F,
    6: G,
    7: H,
    8: I,
    9: J,
    10: K,
    11: L
);

#[cfg(test)]
mod test {
    use crate::test_runner::{Reason, test_runner_without_persistence};

    use strict_test_support::{TestFailure, ensure, ensure_some};

    use super::*;

    #[allow(
        clippy::single_call_fn,
        reason = "the tuple shrink test names the left-to-right minimal failing walk"
    )]
    fn shrink_to_minimal_failing_tuple<V, P>(case: &mut V, pass: P)
    where
        V: ValueTree<Value = (i32, i32)>,
        P: Fn((i32, i32)) -> bool,
    {
        loop {
            let advanced = if pass(case.current()) {
                case.complicate()
            } else {
                case.simplify()
            };
            if advanced {
                continue;
            }
            break;
        }
    }

    #[test]
    fn shrinks_fully_ltr() -> Result<(), TestFailure> {
        fn pass(pair: (i32, i32)) -> bool {
            pair.0 * pair.1 <= 9
        }

        let input = (0..32, 0..32);
        let mut runner = test_runner_without_persistence();

        let mut cases_tested = 0;
        for _ in 0..256 {
            // Find a failing test case
            let mut case = ensure_some(
                input.new_tree(&mut runner).ok(),
                "tuple strategy generates a value tree",
            )?;
            if pass(case.current()) {
                continue;
            }

            shrink_to_minimal_failing_tuple(&mut case, pass);

            let last = case.current();
            ensure(!pass(last), "the shrunken case still fails")?;
            // Maximally shrunken
            ensure(
                pass((last.0 - 1, last.1)),
                "decrementing the first element passes",
            )?;
            ensure(
                pass((last.0, last.1 - 1)),
                "decrementing the second element passes",
            )?;

            cases_tested += 1;
        }

        ensure(cases_tested > 32, "didn't find enough test cases")?;
        Ok(())
    }

    #[test]
    fn test_sanity() -> Result<(), Reason> {
        check_strategy_sanity((0_i32..100, 0_i32..1000, 0_i32..10000), None)
    }
}
