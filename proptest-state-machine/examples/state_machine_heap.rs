//-
// Copyright 2023 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! In this example, we demonstrate using the state machine testing approach
//! for a heap implementation that has a bug in it. The heap `MyHeap` is in the
//! `system_under_test` module inlined at the bottom of this file.

use proptest::prelude::*;
use proptest::strict::{TestFailure, TestResult};
use proptest::test_runner::Config;
use proptest_state_machine::{
    ReferenceStateMachine, StateMachineTest, prop_state_machine,
};
use strict_test_support::ensure;
use system_under_test::MyHeap;

// Setup the state machine test using the `prop_state_machine!` macro
prop_state_machine! {
    #![proptest_config(Config {
        // Turn failure persistence off for demonstration. This means that no
        // regression file will be captured.
        failure_persistence: None,
        // Enable verbose mode to make the state machine test print the
        // transitions for each case.
        verbose: 1,
        .. Config::default()
    })]

    // NOTE: The `#[test]` attribute is commented out in here so we can run it
    // as an example from the `fn main`.

    // #[test]
    fn run_my_heap_test(
        // This is a macro's keyword - only `sequential` is currently supported.
        sequential
        // The number of transitions to be generated for each case. This can
        // be a single numerical value or a range as in here.
        1..20
        // Macro's boilerplate to separate the following identifier.
        =>
        // The name of the type that implements `StateMachineTest`.
        MyHeap<i32>
    );
}

fn main() -> TestResult {
    // The generated test fn returns the strict verdict; returning it from
    // `main` reports a falsified property through the process exit status
    // instead of a panic.
    run_my_heap_test()
}

/// An empty type used for the `ReferenceStateMachine` implementation. The
/// actual state of it represented by `Vec<i32>`, but it doesn't have to
/// contained inside this type.
#[derive(Clone, Copy, Debug)]
pub struct HeapStateMachine;

/// The possible transitions of the state machine.
#[derive(Clone, Copy, Debug)]
pub enum Transition {
    /// Remove the maximum element from the heap.
    Pop,
    /// Insert the given value into the heap.
    Push(i32),
}

// Implementation of the reference state machine that drives the test. That is,
// it's used to generate a sequence of transitions the `StateMachineTest`.
impl ReferenceStateMachine for HeapStateMachine {
    type State = Vec<i32>;
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(vec![]).boxed()
    }

    fn transitions(_state: &Self::State) -> BoxedStrategy<Self::Transition> {
        // Using the regular proptest constructs here, the transitions can be
        // given different weights.
        prop_oneof![
            1 => Just(Transition::Pop),
            2 => (any::<i32>()).prop_map(Transition::Push),
        ]
        .boxed()
    }

    fn apply(
        mut state: Self::State,
        transition: &Self::Transition,
    ) -> Self::State {
        match transition {
            Transition::Pop => {
                let _popped = state.pop();
            }
            Transition::Push(element) => state.push(*element),
        }
        state
    }
}

impl StateMachineTest for MyHeap<i32> {
    type SystemUnderTest = Self;
    type Reference = HeapStateMachine;

    fn init_test(
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        Self::new()
    }

    fn apply(
        mut state: Self::SystemUnderTest,
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
        transition: Transition,
    ) -> Result<Self::SystemUnderTest, TestFailure> {
        match transition {
            Transition::Pop => {
                // We read the state before applying the transition.
                let was_empty = state.is_empty();

                // We use the broken implementation of pop, which should be
                // discovered by the test.
                let result = state.pop_wrong();

                // NOTE: To fix the issue that gets found by the state machine,
                // you can comment out the last statement with `pop_wrong` and
                // uncomment this one to see the test pass:
                // let result = state.pop();

                // Check a post-condition.
                match result {
                    Some(popped) => {
                        ensure(
                            !was_empty,
                            "a popped value implies the heap was non-empty",
                        )?;
                        // The heap must not contain any value which was
                        // greater than the "maximum" we were just given.
                        for in_heap in state.iter() {
                            ensure(
                                popped >= *in_heap,
                                "the popped value is greater than or equal \
                                 to every value still in the heap",
                            )?;
                        }
                    }
                    None => ensure(
                        was_empty,
                        "an empty pop implies the heap was empty",
                    )?,
                }
            }
            Transition::Push(element) => state.push(element),
        }
        Ok(state)
    }

    fn check_invariants(
        state: &Self::SystemUnderTest,
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> TestResult {
        // Check that the heap's API gives consistent results
        match state.len() {
            0 => {
                ensure(state.is_empty(), "a zero-length heap reports is_empty")
            }
            _ => ensure(
                !state.is_empty(),
                "a non-zero-length heap does not report is_empty",
            ),
        }
    }
}

/// A hand-rolled implementation of a binary heap, like
/// <https://doc.rust-lang.org/stable/std/collections/struct.BinaryHeap.html>,
/// except slow and buggy.
mod system_under_test {
    /// Minimal max-heap implementation used as the system under test.
    #[derive(Clone, Debug)]
    pub(crate) struct MyHeap<T> {
        /// Backing array storing heap elements in max-heap order.
        data: Vec<T>,
    }

    impl<T: Ord> MyHeap<T> {
        /// Create an empty heap.
        #[allow(
            clippy::single_call_fn,
            reason = "the empty hand-rolled max-heap the example puts under test"
        )]
        pub(crate) const fn new() -> Self {
            Self { data: vec![] }
        }

        /// Return whether the heap contains no elements.
        pub(crate) const fn is_empty(&self) -> bool {
            self.data.is_empty()
        }

        /// Return the number of elements currently stored in the heap.
        pub(crate) const fn len(&self) -> usize {
            self.data.len()
        }

        /// Iterate over the heap's backing storage.
        pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
            self.data.iter()
        }

        /// Insert an element and restore the max-heap ordering upward.
        pub(crate) fn push(&mut self, element: T) {
            self.data.push(element);
            let mut index = self.data.len() - 1;
            while index > 0 {
                let parent = (index - 1) / 2;
                if self.data[parent] < self.data[index] {
                    self.data.swap(index, parent);
                    index = parent;
                } else {
                    break;
                }
            }
        }

        // This implementation is wrong, because it doesn't preserve ordering
        /// Remove the root without restoring heap order.
        pub(crate) fn pop_wrong(&mut self) -> Option<T> {
            if self.is_empty() {
                None
            } else {
                Some(self.data.swap_remove(0))
            }
        }

        // Fixed implementation of pop()
        /// Remove the maximum element while preserving heap order.
        #[allow(dead_code)]
        pub(crate) fn pop(&mut self) -> Option<T> {
            if self.is_empty() {
                return None;
            }

            let ret = self.data.swap_remove(0);

            // Restore the heap property
            let mut index = 0;
            loop {
                let child1 = index * 2 + 1;
                let child2 = index * 2 + 2;
                if child1 >= self.data.len() {
                    break;
                }

                let child = if child2 == self.data.len()
                    || self.data[child1] > self.data[child2]
                {
                    child1
                } else {
                    child2
                };

                if self.data[index] < self.data[child] {
                    self.data.swap(child, index);
                    index = child;
                } else {
                    break;
                }
            }

            Some(ret)
        }
    }
}
