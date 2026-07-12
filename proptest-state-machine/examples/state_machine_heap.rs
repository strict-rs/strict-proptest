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
use proptest::strict::TestFailure;
use proptest::strict::TestResult;
use proptest::test_runner::Config;
use proptest_state_machine::ReferenceStateMachine;
use proptest_state_machine::StateMachineTest;
use proptest_state_machine::prop_state_machine;
use proptest_state_machine::strict_state_machine_config;
use strict_test_support::ensure;
use system_under_test::MyHeap;

// Setup the state machine test using the `prop_state_machine!` macro
prop_state_machine! {
    #![proptest_config(Config {
        // Enable verbose mode to make the state machine test print the
        // transitions for each case.
        verbose: 1,
        .. strict_state_machine_config()
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
  ensure(
    !PopImplementation::OPTIONS.is_empty(),
    "the heap example exposes at least one pop implementation",
  )?;
  // The generated test fn returns the strict verdict; returning it from
  // `main` reports a falsified property through the process exit status
  // instead of a panic.
  run_my_heap_test()
}

/// Which pop behavior the heap example should use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PopImplementation {
  /// Remove the root without restoring heap order.
  Wrong,
  /// Remove the root and restore heap order afterward.
  Correct,
}

impl PopImplementation {
  /// The intentionally wrong implementation used by the runnable example.
  const BUGGY_DEFAULT: Self = Self::Wrong;
  /// The corrected implementation that makes the state machine pass.
  const CORRECT: Self = Self::Correct;
  /// All supported pop implementations, keeping the teaching alternatives
  /// visible in the example binary.
  const OPTIONS: [Self; 2] = [Self::BUGGY_DEFAULT, Self::CORRECT];
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

  fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
    match *transition {
      Transition::Pop => {
        let _popped = state.pop();
      }
      Transition::Push(element) => state.push(element),
    }
    state
  }
}

impl StateMachineTest for MyHeap<i32> {
  type SystemUnderTest = Self;
  type Reference = HeapStateMachine;

  fn init_test(_ref_state: &<Self::Reference as ReferenceStateMachine>::State) -> Self::SystemUnderTest {
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
        let result = state.pop_using(PopImplementation::BUGGY_DEFAULT);

        // NOTE: To fix the issue that gets found by the state machine,
        // switch the implementation from `BUGGY_DEFAULT` to
        // `CORRECT`.

        // Check a post-condition.
        match result {
          Some(popped) => {
            ensure(!was_empty, "a popped value implies the heap was non-empty")?;
            ensure_popped_value_is_heap_max(popped, &state)?;
          }
          None => ensure(was_empty, "an empty pop implies the heap was empty")?,
        }
      }
      Transition::Push(element) => state.push(element),
    }
    Ok(state)
  }

  fn check_invariants(state: &Self::SystemUnderTest, _ref_state: &<Self::Reference as ReferenceStateMachine>::State) -> TestResult {
    // Check that the heap's API gives consistent results
    match state.len() {
      0 => ensure(state.is_empty(), "a zero-length heap reports is_empty"),
      _ => ensure(!state.is_empty(), "a non-zero-length heap does not report is_empty"),
    }
  }
}

#[allow(
  clippy::single_call_fn,
  reason = "the heap example names the pop post-condition checked after every generated Pop transition"
)]
/// Verify that the popped value is still at least every value left in the heap.
fn ensure_popped_value_is_heap_max(popped: i32, state: &MyHeap<i32>) -> Result<(), TestFailure> {
  // The heap must not contain any value which was greater than the
  // "maximum" we were just given.
  for in_heap in state.iter() {
    ensure(
      popped >= *in_heap,
      "the popped value is greater than or equal to every value still in the heap",
    )?;
  }
  Ok(())
}

/// A hand-rolled implementation of a binary heap, like
/// <https://doc.rust-lang.org/stable/std/collections/struct.BinaryHeap.html>,
/// except slow and buggy.
pub mod system_under_test {
  use core::mem;

  use super::PopImplementation;

  /// Minimal max-heap implementation used as the system under test.
  #[derive(Clone, Debug)]
  pub struct MyHeap<T> {
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
      Self {
        data: vec![]
      }
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

    /// Swap two backing-storage indices when both are in bounds.
    fn swap_indices(values: &mut [T], first: usize, second: usize) -> bool {
      if first == second {
        return first < values.len();
      }

      let (low, high) = if first < second {
        (first, second)
      } else {
        (second, first)
      };
      let Some((prefix, suffix)) = values.split_at_mut_checked(high) else {
        return false;
      };
      let Some(left) = prefix.get_mut(low) else {
        return false;
      };
      let Some(right) = suffix.get_mut(0) else {
        return false;
      };
      mem::swap(left, right);
      true
    }

    /// Remove the root by moving the last element into its slot.
    fn remove_root_without_reorder(&mut self) -> Option<T> {
      let last = self.data.pop()?;
      if self.data.is_empty() {
        return Some(last);
      }

      self.data.first_mut().map(|root| mem::replace(root, last))
    }

    #[allow(
      clippy::single_call_fn,
      reason = "heap insertion names one upward restore step separately from the push loop"
    )]
    /// Restore heap ordering by swapping the node at `index` upward once.
    fn bubble_up_once(&mut self, index: usize) -> Option<usize> {
      if index == 0 {
        return None;
      }

      let parent = index.saturating_sub(1).div_euclid(2);
      let should_swap = self
        .data
        .get(parent)
        .zip(self.data.get(index))
        .is_some_and(|(parent_value, child_value)| parent_value < child_value);
      if !should_swap {
        return None;
      }

      Self::swap_indices(&mut self.data, index, parent).then_some(parent)
    }

    #[allow(
      clippy::single_call_fn,
      reason = "heap removal names one downward restore step separately from the pop loop"
    )]
    /// Restore heap ordering by swapping the node at `index` downward once.
    fn bubble_down_once(&mut self, index: usize) -> Option<usize> {
      let child1 = index.saturating_mul(2).saturating_add(1);
      let child2 = index.saturating_mul(2).saturating_add(2);
      let child = match (self.data.get(child1), self.data.get(child2)) {
        (Some(_), None) => child1,
        (Some(left), Some(right)) if left > right => child1,
        (Some(_), Some(_)) => child2,
        _ => return None,
      };

      let should_swap = self
        .data
        .get(index)
        .zip(self.data.get(child))
        .is_some_and(|(parent, selected_child)| parent < selected_child);
      if !should_swap {
        return None;
      }

      Self::swap_indices(&mut self.data, child, index).then_some(child)
    }

    /// Insert an element and restore the max-heap ordering upward.
    pub(crate) fn push(&mut self, element: T) {
      self.data.push(element);
      let mut index = self.data.len().saturating_sub(1);
      while let Some(parent) = self.bubble_up_once(index) {
        index = parent;
      }
    }

    // This implementation is wrong, because it doesn't preserve ordering
    /// Remove the root without restoring heap order.
    pub(crate) fn pop_wrong(&mut self) -> Option<T> {
      if self.is_empty() {
        None
      } else {
        self.remove_root_without_reorder()
      }
    }

    // Fixed implementation of pop()
    /// Remove the maximum element while preserving heap order.
    pub(crate) fn pop(&mut self) -> Option<T> {
      if self.is_empty() {
        return None;
      }

      let ret = self.remove_root_without_reorder()?;

      // Restore the heap property
      let mut index = 0_usize;
      while let Some(child) = self.bubble_down_once(index) {
        index = child;
      }

      Some(ret)
    }

    /// Remove an element using the selected implementation.
    pub(super) fn pop_using(&mut self, implementation: PopImplementation) -> Option<T> {
      match implementation {
        PopImplementation::Wrong => self.pop_wrong(),
        PopImplementation::Correct => self.pop(),
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;

  use super::system_under_test::MyHeap;

  #[test]
  fn corrected_pop_returns_descending_maxima() -> Result<(), TestFailure> {
    let mut heap = MyHeap::new();
    for value in [3, 1, 9, 4, 7, 2] {
      heap.push(value);
    }

    ensure(heap.pop() == Some(9), "first corrected pop returns the maximum")?;
    ensure(heap.pop() == Some(7), "second corrected pop restores heap order before returning")?;

    let mut unordered = MyHeap::new();
    for value in [5, 8, 6, 10, 1, 4] {
      unordered.push(value);
    }
    let mut observed = Vec::new();
    while let Some(value) = unordered.pop() {
      observed.push(value);
    }
    ensure(
      observed.as_slice() == [10, 8, 6, 5, 4, 1],
      "corrected pop drains unordered pushes in descending order",
    )
  }
}
