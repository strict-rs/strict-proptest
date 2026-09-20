//-
// Copyright 2023 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Test declaration helpers and runners for abstract state machine testing.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic;
use std::sync::atomic::AtomicUsize;
use std::vec::IntoIter;

#[cfg(feature = "std")]
use proptest::std_facade::fmt;
#[cfg(feature = "std")]
use proptest::std_facade::fmt::Debug;
use proptest::strict::strict_default_config;
use proptest::test_runner::Config;
#[cfg(feature = "std")]
use proptest::test_runner::INFO_LOG;
use proptest::test_runner::PropertyResult;
#[cfg(feature = "std")]
use proptest::test_runner::emit_diagnostic_line;

use crate::strategy::ReferenceStateMachine;

/// Display adapter for transition values whose public contract is `Debug`.
#[cfg(feature = "std")]
struct DebugDiagnostic<'a, T: ?Sized>(&'a T);

#[cfg(feature = "std")]
impl<T: Debug + ?Sized> fmt::Display for DebugDiagnostic<'_, T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    Debug::fmt(self.0, f)
  }
}

/// Report case size separately from the sequential driver's state transitions.
#[cfg(feature = "std")]
#[allow(
  clippy::single_call_fn,
  reason = "case-start diagnostics remain separate from sequential execution and failure ownership"
)]
fn trace_case_start(config: &Config, transitions: usize) {
  if config.verbose >= INFO_LOG {
    emit_diagnostic_line(format_args!(""));
    emit_diagnostic_line(format_args!("Running a test case with {transitions} transitions."));
  }
}

/// Report the next attempted transition without taking ownership of its value.
#[cfg(feature = "std")]
#[allow(
  clippy::single_call_fn,
  reason = "transition diagnostics borrow the native value before the model and SUT advance"
)]
fn trace_transition(config: &Config, index: usize, total: usize, transition: &impl Debug) {
  if config.verbose >= INFO_LOG {
    emit_diagnostic_line(format_args!(""));
    emit_diagnostic_line(format_args!("Applying transition {index}/{total}: {}", DebugDiagnostic(transition)));
  }
}

/// Build the `Config` used by state-machine drivers that run under the strict
/// property harness.
///
/// The outer `proptest::strict` runner already owns generation, shrinking, and
/// persistence policy. The inner state-machine driver receives a `Config` only
/// for per-transition behavior such as verbose diagnostics, so its default must
/// not re-enable the legacy failure-persistence backend.
#[must_use]
pub fn strict_state_machine_config() -> Config {
  let mut config = strict_default_config();
  config.failure_persistence = None;
  config
}

/// Normalize a caller-provided state-machine config for strict execution.
///
/// Callers may still request local state-machine behavior such as `verbose`,
/// but persistence belongs to the outer strict property runner and stays off.
#[must_use]
#[allow(
  clippy::single_call_fn,
  reason = "macro expansion adapter names the strict state-machine config boundary"
)]
pub fn strict_state_machine_config_from(mut config: Config) -> Config {
  config.failure_persistence = None;
  config
}

/// Evidence from one attempted transition, including a partially completed check.
#[derive(Debug)]
pub struct TransitionEvidence<T, A, I> {
  /// The native transition attempted by the driver.
  pub transition:  T,
  /// Successful application evidence, absent when application failed.
  pub application: Option<A>,
  /// Successful invariant evidence, absent when that check was not completed.
  pub invariant:   Option<I>,
}

/// Ordered native observations produced by a sequential state-machine case.
#[derive(Debug)]
pub struct SequentialEvidence<T, A, I> {
  /// Initial invariant evidence, absent only on an initial-check failure.
  pub initial_invariant: Option<I>,
  /// Every reached transition and its completed checks.
  pub transitions:       Vec<TransitionEvidence<T, A, I>>,
}

/// The hook at which a sequential case stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequentialStage {
  /// Invariants of the initialized SUT.
  InitialInvariant,
  /// SUT application, after advancing the model.
  Application,
  /// Invariants after SUT application.
  Invariant,
  /// Normal end-of-case teardown.
  Teardown,
}

/// A failed case with reached evidence, the original error, and unconsumed work.
///
/// A consuming application or teardown hook owns resources passed into it;
/// its concrete error must preserve any resources that it promises to return.
/// The driver retains resources still in its custody and does not add recovery.
#[derive(Debug, thiserror::Error)]
#[error("state-machine {stage:?} failed: {failure:?}")]
pub struct SequentialFailure<S, R, T, A, I, E> {
  /// The hook which returned the failure.
  pub stage:     SequentialStage,
  /// Its original concrete error.
  pub failure:   E,
  /// Every observation produced before stopping.
  pub evidence:  SequentialEvidence<T, A, I>,
  /// SUT still owned by the driver when an invariant failed.
  pub system:    Option<S>,
  /// Model still owned by the driver, absent after consuming teardown.
  pub reference: Option<R>,
  /// Transitions after the failing transition, never executed by the driver.
  pub remaining: IntoIter<T>,
}

/// The model state of a concrete state-machine test.
pub type ModelState<M> = <<M as StateMachineTest>::Reference as ReferenceStateMachine>::State;
/// The transition of a concrete state-machine test.
pub type ModelTransition<M> = <<M as StateMachineTest>::Reference as ReferenceStateMachine>::Transition;
/// Native strategy input, including the shared seen-transition counter.
pub type StateMachineCase<M> = (ModelState<M>, Vec<ModelTransition<M>>, Option<Arc<AtomicUsize>>);
/// Complete successful sequential observations.
pub type StateMachineEvidence<M> =
  SequentialEvidence<ModelTransition<M>, <M as StateMachineTest>::TransitionEvidence, <M as StateMachineTest>::InvariantEvidence>;
/// Concrete failure and progress of a sequential case.
pub type StateMachineFailure<M> = SequentialFailure<
  <M as StateMachineTest>::SystemUnderTest,
  ModelState<M>,
  ModelTransition<M>,
  <M as StateMachineTest>::TransitionEvidence,
  <M as StateMachineTest>::InvariantEvidence,
  <M as StateMachineTest>::Failure,
>;
/// The result of one sequential case.
pub type SequentialResult<M> = Result<StateMachineEvidence<M>, Box<StateMachineFailure<M>>>;
/// The next SUT state and complete native application evidence, or its original error.
pub type TransitionResult<M> = Result<
  (
    <M as StateMachineTest>::SystemUnderTest,
    <M as StateMachineTest>::TransitionEvidence,
  ),
  <M as StateMachineTest>::Failure,
>;
/// Native property result returned by a state-machine macro wrapper.
pub type StateMachinePropertyResult<M, T = Infallible> =
  PropertyResult<StateMachineCase<M>, StateMachineEvidence<M>, Box<StateMachineFailure<M>>, T>;

/// State machine test that relies on a reference state-machine model.
pub trait StateMachineTest: Sized {
  /// Concrete system under test.
  type SystemUnderTest;
  /// Model driving valid transition generation and shrinking.
  type Reference: ReferenceStateMachine;
  /// Concrete error composed by the test's hooks.
  type Failure;
  /// Native observations returned by successful transition applications.
  type TransitionEvidence;
  /// Native observations returned by successful invariant checks.
  type InvariantEvidence;

  /// Initialize the SUT from the generated model state.
  fn init_test(ref_state: &ModelState<Self>) -> Self::SystemUnderTest;

  /// Advance the SUT using the already-advanced model, returning its state and evidence.
  ///
  /// # Errors
  /// Returns the hook's original error; the driver does not run teardown after failure.
  fn apply(state: Self::SystemUnderTest, ref_state: &ModelState<Self>, transition: ModelTransition<Self>) -> TransitionResult<Self>;

  /// Inspect invariants initially and after each successful application.
  ///
  /// # Errors
  /// Returns the original invariant failure without erasing earlier evidence.
  fn check_invariants(state: &Self::SystemUnderTest, ref_state: &ModelState<Self>) -> Result<Self::InvariantEvidence, Self::Failure>;

  /// Consume the final SUT and model after all transitions succeed.
  ///
  /// # Errors
  /// A custom teardown can return its concrete failure. The default drops both states.
  #[allow(
    clippy::single_call_fn,
    reason = "the public lifecycle hook allows implementers to finalize each successful case"
  )]
  fn teardown(state: Self::SystemUnderTest, ref_state: ModelState<Self>) -> Result<(), Self::Failure> {
    drop((state, ref_state));
    Ok(())
  }

  /// Execute sequentially, preserving native evidence through the stopping boundary.
  ///
  /// The seen counter increments before application, the model advances before
  /// the SUT, failures short-circuit, and teardown runs only on the normal path.
  ///
  /// # Errors
  /// Returns the original hook error, completed observations, driver-owned states,
  /// and the unattempted transition iterator.
  fn test_sequential(
    config: Config,
    mut ref_state: ModelState<Self>,
    transitions: Vec<ModelTransition<Self>>,
    seen_counter: Option<Arc<AtomicUsize>>,
  ) -> SequentialResult<Self> {
    #[cfg(feature = "std")]
    let trans_len = transitions.len();
    #[cfg(feature = "std")]
    trace_case_start(&config, trans_len);
    #[cfg(not(feature = "std"))]
    drop(config);
    let mut remaining = transitions.into_iter();
    let mut evidence = SequentialEvidence {
      initial_invariant: None,
      transitions:       Vec::new(),
    };
    let mut system = Self::init_test(&ref_state);
    match Self::check_invariants(&system, &ref_state) {
      Ok(observed) => evidence.initial_invariant = Some(observed),
      Err(failure) => {
        return Err(Box::new(SequentialFailure {
          stage: SequentialStage::InitialInvariant,
          failure,
          evidence,
          system: Some(system),
          reference: Some(ref_state),
          remaining,
        }));
      }
    }
    while let Some(transition) = remaining.next() {
      if let Some(counter) = seen_counter.as_ref() {
        let _previous_seen = counter.fetch_add(1, atomic::Ordering::SeqCst);
      }
      #[cfg(feature = "std")]
      trace_transition(&config, evidence.transitions.len().saturating_add(1), trans_len, &transition);
      let mut step = TransitionEvidence {
        transition:  transition.clone(),
        application: None,
        invariant:   None,
      };
      ref_state = Self::Reference::apply(ref_state, &transition);
      match Self::apply(system, &ref_state, transition) {
        Ok((next, observed)) => {
          system = next;
          step.application = Some(observed);
        }
        Err(failure) => {
          evidence.transitions.push(step);
          return Err(Box::new(SequentialFailure {
            stage: SequentialStage::Application,
            failure,
            evidence,
            system: None,
            reference: Some(ref_state),
            remaining,
          }));
        }
      }
      match Self::check_invariants(&system, &ref_state) {
        Ok(observed) => step.invariant = Some(observed),
        Err(failure) => {
          evidence.transitions.push(step);
          return Err(Box::new(SequentialFailure {
            stage: SequentialStage::Invariant,
            failure,
            evidence,
            system: Some(system),
            reference: Some(ref_state),
            remaining,
          }));
        }
      }
      evidence.transitions.push(step);
    }
    match Self::teardown(system, ref_state) {
      Ok(()) => Ok(evidence),
      Err(failure) => Err(Box::new(SequentialFailure {
        stage: SequentialStage::Teardown,
        failure,
        evidence,
        system: None,
        reference: None,
        remaining,
      })),
    }
  }
}

/// Turn a state machine test implementation into a runnable test.
///
/// The macro expects a function header whose arguments follow special syntax
/// rules: first, whether to apply the state machine transitions sequentially or
/// concurrently (currently, only `sequential` is supported). Next, give a range
/// of how many transitions to generate, followed by `=>` and finally an
/// identifier that must implement `StateMachineTest`.
///
/// ## Example
///
/// ```rust,ignore
/// struct MyTest;
///
/// impl StateMachineTest for MyTest {}
///
/// prop_state_machine! {
///     #[test]
///     fn run_with_macro(sequential 1..20 => MyTest);
/// }
/// ```
///
/// This example will expand to:
///
/// ```rust,ignore
/// struct MyTest;
///
/// impl StateMachineTest for MyTest {}
///
/// #[test]
/// fn run_with_macro() -> ::proptest_state_machine::StateMachinePropertyResult<MyTest> {
///     let strategy = <<MyTest as StateMachineTest>::Reference
///         as ReferenceStateMachine>::sequential_strategy(1..20);
///     ::proptest::strict::ensure_property(
///         &strategy,
///         stringify!(run_with_macro),
///         |(initial_state, transitions, seen_counter)| {
///             MyTest::test_sequential(
///                 ::proptest_state_machine::strict_state_machine_config(),
///                 initial_state,
///                 transitions,
///                 seen_counter,
///             )
///         },
///     )
/// }
/// ```
///
/// The generated test returns [`StateMachinePropertyResult`]. A falsified run
/// retains its minimized native case, the concrete hook failure, and ordered
/// sequential evidence. The optional configuration expression is evaluated per
/// case for the inner driver, including verbose diagnostics; the outer runner
/// uses strict defaults. Use `ensure_property_with_config` or
/// `ensure_property_with_transport` with the sequence strategy for explicit
/// runner configuration or a model-specific replay codec.
#[macro_export]
macro_rules! prop_state_machine {
    // With proptest config annotation
    (#![proptest_config($config:expr)] $($tests:tt)*) => {
        $crate::prop_state_machine! {
            @_CASES [$crate::strict_state_machine_config_from($config.__sugar_to_owned())]
            $($tests)*
        }
    };

    // Both public forms retain one per-case configuration evaluation.
    (@_CASES [$config:expr]
    $(
        $(#[$meta:meta])*
        fn $test_name:ident(sequential $size:expr => $test:ident $(< $( $ty_param:tt ),+ >)?);
    )*) => {
        $(
            $(#[$meta])*
            fn $test_name() -> $crate::StateMachinePropertyResult<$test $(< $( $ty_param ),+ >)?> {
                let strategy = <<$test $(< $( $ty_param ),+ >)? as $crate::StateMachineTest>::Reference as $crate::ReferenceStateMachine>::sequential_strategy($size);
                ::proptest::strict::ensure_property(
                    &strategy,
                    stringify!($test_name),
                    |(initial_state, transitions, seen_counter)| {
                        // The inner driver configuration never selects the
                        // outer runner's generation or persistence policy.
                        let config = $config;
                        <$test $(::< $( $ty_param ),+ >)? as $crate::StateMachineTest>::test_sequential(config, initial_state, transitions, seen_counter)
                    },
                )
            }
        )*
    };

    // Without proptest config annotation
    ($(
        $(#[$meta:meta])*
        fn $test_name:ident(sequential $size:expr => $test:ident $(< $( $ty_param:tt ),+ >)?);
    )*) => {
        $crate::prop_state_machine! {
            @_CASES [$crate::strict_state_machine_config()]
            $(
                $(#[$meta])*
                fn $test_name(sequential $size => $test $(< $( $ty_param ),+ >)?);
            )*
        }
    };
}

#[cfg(test)]
mod macro_test {
  use std::cell::Cell;
  use std::convert::Infallible;
  use std::fmt::Debug;

  use proptest::strategy::BoxedStrategy;
  use proptest::strategy::Just;
  use proptest::strategy::Strategy as _;
  use proptest::strict::strict_default_config;
  use proptest::test_runner::Config;
  use strict_test_support::ensure_that;

  /// A no-op model exercising hygienic macro expansion.
  struct Test;
  impl crate::ReferenceStateMachine for Test {
    type State = ();
    type Transition = ();
    fn init_state() -> BoxedStrategy<()> {
      Just(()).boxed()
    }
    fn transitions(&(): &()) -> BoxedStrategy<()> {
      Just(()).boxed()
    }
    fn apply((): (), &(): &()) {}
  }
  impl crate::StateMachineTest for Test {
    type SystemUnderTest = ();
    type Reference = Self;
    type Failure = Infallible;
    type TransitionEvidence = ();
    type InvariantEvidence = ();
    fn init_test(&(): &()) {}
    fn apply((): (), &(): &(), (): ()) -> Result<((), ()), Infallible> {
      Ok(((), ()))
    }
    fn check_invariants(&(): &(), &(): &()) -> Result<(), Infallible> {
      Ok(())
    }
  }
  prop_state_machine! {
    #[test]
    fn no_config_annotation(sequential 1..2 => Test);
  }
  prop_state_machine! {
    #![proptest_config(Config::default())]
    #[test]
    fn with_config_annotation(sequential 1..2 => Test);
  }

  #[test]
  fn inner_configuration_is_evaluated_for_each_outer_case() -> Result<(), impl Debug> {
    thread_local! {
      /// Configuration evaluations in this thread's generated property run.
      static CONFIGURATIONS: Cell<u32> = const { Cell::new(0) };
    }
    prop_state_machine! {
      #![proptest_config({
        CONFIGURATIONS.with(|count| count.set(count.get().saturating_add(1)));
        Config { cases: 0, ..Config::default() }
      })]
      #[allow(clippy::single_call_fn, reason = "the generated wrapper is invoked directly so its complete report and configuration effects can be inspected")]
      fn configured_cases(sequential 1..2 => Test);
    }
    let expected_cases = strict_default_config().cases;
    CONFIGURATIONS.set(0);
    let report = configured_cases();
    let configurations = CONFIGURATIONS.get();
    ensure_that(
      (expected_cases, configurations, report),
      "the inner configuration is evaluated once per case and cannot override the outer runner's case count",
      |observed| {
        observed.0 == observed.1
          && observed
            .2
            .as_ref()
            .is_ok_and(|run| run.statistics.successes == observed.0 && u32::try_from(run.evaluations.len()) == Ok(observed.1))
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}

#[cfg(test)]
mod strict_behavior {
  use proptest::prelude::any;
  use proptest::strategy::BoxedStrategy;
  use proptest::strategy::Just;
  use proptest::strategy::Strategy as _;
  use strict_test_support::CapturedBinary;
  use strict_test_support::ComparisonFailure;
  use strict_test_support::OptionFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::TestFailure;
  use strict_test_support::capture_ignored_test;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_some;
  use strict_test_support::ensure_that;

  use crate::ReferenceStateMachine;
  use crate::SequentialResult;
  use crate::StateMachineEvidence;
  use crate::StateMachineFailure;
  use crate::StateMachineTest;

  /// Native output or fixture failure from the generated stack test process.
  type CapturedStack = Result<CapturedBinary, TestFailure>;

  /// Observable operations of the stack model.
  #[derive(Clone, Debug, PartialEq)]
  enum Op {
    Push(u8),
    Pop,
  }

  /// Stateful stack reference model.
  struct StackModel;
  impl ReferenceStateMachine for StackModel {
    type State = Vec<u8>;
    type Transition = Op;
    fn init_state() -> BoxedStrategy<Vec<u8>> {
      Just(Vec::new()).boxed()
    }
    fn transitions(state: &Vec<u8>) -> BoxedStrategy<Op> {
      let push = any::<u8>().prop_map(Op::Push);
      if state.is_empty() {
        push.boxed()
      } else {
        proptest::prop_oneof![push, Just(Op::Pop)].boxed()
      }
    }
    fn apply(mut state: Vec<u8>, transition: &Op) -> Vec<u8> {
      match *transition {
        Op::Push(value) => state.push(value),
        Op::Pop => {
          let _popped = state.pop();
        }
      }
      state
    }
    fn preconditions(state: &Vec<u8>, transition: &Op) -> bool {
      match *transition {
        Op::Push(_) => true,
        Op::Pop => !state.is_empty(),
      }
    }
  }

  /// Native errors from stack application and invariant checks.
  #[derive(Debug, thiserror::Error)]
  enum StackFailure {
    /// A pop retained the emptied SUT and native absence.
    #[error("pop failed for {state:?}: {failure}")]
    Pop {
      state:   Vec<u8>,
      failure: OptionFailure<u8>,
    },
    /// Complete SUT and model states that disagree.
    #[error(transparent)]
    State(#[from] ComparisonFailure<Vec<u8>, Vec<u8>>),
    /// Both observed depths.
    #[error(transparent)]
    Depth(#[from] ComparisonFailure<usize, usize>),
  }

  /// SUT mirroring the model and retaining transition and invariant evidence.
  struct StackSut;
  impl StateMachineTest for StackSut {
    type SystemUnderTest = Vec<u8>;
    type Reference = StackModel;
    type Failure = StackFailure;
    type TransitionEvidence = (Option<u8>, Vec<u8>);
    type InvariantEvidence = (usize, usize);
    fn init_test(reference: &Vec<u8>) -> Vec<u8> {
      reference.clone()
    }
    fn apply(mut state: Vec<u8>, reference: &Vec<u8>, transition: Op) -> Result<(Vec<u8>, Self::TransitionEvidence), StackFailure> {
      let operation = match transition {
        Op::Push(value) => {
          state.push(value);
          Ok(None)
        }
        Op::Pop => ensure_some(state.pop(), "Pop removes an element from a non-empty stack").map(Some),
      };
      let popped = match operation {
        Ok(value) => value,
        Err(failure) => {
          return Err(StackFailure::Pop {
            state,
            failure,
          });
        }
      };
      let (checked, observed) = ensure_eq(state, reference.clone(), "SUT and model agree after every transition")?;
      Ok((checked, (popped, observed)))
    }
    fn check_invariants(state: &Vec<u8>, reference: &Vec<u8>) -> Result<Self::InvariantEvidence, StackFailure> {
      ensure_eq(state.len(), reference.len(), "SUT and model agree on stack depth").map_err(StackFailure::Depth)
    }
  }

  /// Ignored child name used by the output-capture contract.
  const STACK_MACRO_CHILD: &str = concat!(
    "test_runner::strict_behavior::",
    "passing_stack_model_runs_through_the_strict_macro_child"
  );
  prop_state_machine! {
    #[test]
    #[ignore = "captured by passing_stack_model_runs_through_the_strict_macro"]
    fn passing_stack_model_runs_through_the_strict_macro_child(sequential 1..16 => StackSut);
  }

  #[test]
  fn passing_stack_model_runs_through_the_strict_macro() -> Result<(), Box<PredicateFailure<CapturedStack>>> {
    ensure_that(
      capture_ignored_test(STACK_MACRO_CHILD),
      "the real macro passes without uncaptured stderr",
      |result| {
        result
          .as_ref()
          .is_ok_and(|captured| captured.output.status.success() && captured.output.stderr.is_empty())
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn test_sequential_returns_ok_for_a_valid_sequence() -> Result<(), PredicateFailure<SequentialResult<StackSut>>> {
    let complete_sequence = |evidence: &StateMachineEvidence<StackSut>| {
      evidence.initial_invariant == Some((0, 0))
        && evidence.transitions.len() == 3
        && evidence
          .transitions
          .last()
          .is_some_and(|step| step.application == Some((Some(2), vec![1])) && step.invariant == Some((1, 1)))
    };
    ensure_that(
      StackSut::test_sequential(
        crate::strict_state_machine_config(),
        Vec::new(),
        vec![Op::Push(1), Op::Push(2), Op::Pop],
        None,
      ),
      "all application and invariant subjects survive a sequential case",
      |result| result.as_ref().is_ok_and(complete_sequence),
    )
    .map(drop)
  }

  #[test]
  fn failed_application_retains_progress_and_unattempted_transitions() -> Result<(), PredicateFailure<SequentialResult<StackSut>>> {
    let stopped_at_pop = |failure: &StateMachineFailure<StackSut>| {
      failure.stage == crate::SequentialStage::Application
        && failure.evidence.transitions.len() == 3
        && failure
          .evidence
          .transitions
          .get(1)
          .is_some_and(|step| step.application == Some((Some(1), vec![])))
        && failure.remaining.as_slice() == [Op::Push(9)]
        && failure.reference == Some(vec![])
        && matches!(failure.failure, StackFailure::Pop { ref state, .. } if state.is_empty())
    };
    ensure_that(
      StackSut::test_sequential(
        crate::strict_state_machine_config(),
        Vec::new(),
        vec![Op::Push(1), Op::Pop, Op::Pop, Op::Push(9)],
        None,
      ),
      "a failed application preserves earlier checks and stops immediately",
      |result| result.as_ref().is_err_and(|failure| stopped_at_pop(failure)),
    )
    .map(drop)
  }
}
