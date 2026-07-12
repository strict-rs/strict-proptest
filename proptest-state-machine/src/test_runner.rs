//-
// Copyright 2023 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Test declaration helpers and runners for abstract state machine testing.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{
  self,
};

#[cfg(feature = "std")]
use proptest::std_facade::fmt::Debug;
#[cfg(feature = "std")]
use proptest::std_facade::fmt::{
  self,
};
use proptest::strict::TestFailure;
use proptest::strict::TestResult;
use proptest::strict::strict_default_config;
use proptest::test_runner::Config;
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

/// State machine test that relies on a reference state machine model
pub trait StateMachineTest {
  /// The concrete state, that is the system under test (SUT).
  type SystemUnderTest;

  /// The abstract state machine that implements [`ReferenceStateMachine`]
  /// drives the generation of the state machine's transitions.
  type Reference: ReferenceStateMachine;

  /// Initialize the state of SUT.
  ///
  /// If the reference state machine is generated from a non-constant
  /// strategy, ensure to use it to initialize the SUT to a corresponding
  /// state.
  fn init_test(ref_state: &<Self::Reference as ReferenceStateMachine>::State) -> Self::SystemUnderTest;

  /// Apply a transition in the SUT state and check post-conditions.
  /// The post-conditions are properties of your state machine that you want
  /// to uphold; express them with the `strict_test_support` `ensure*`
  /// helpers (or any other `TestFailure` constructor) and propagate them
  /// with `?`, returning the new SUT state on success.
  ///
  /// Note that the `ref_state` is the state *after* this `transition` is
  /// applied. You can use it to compare it with your SUT after you apply
  /// the transition.
  ///
  /// # Errors
  ///
  /// Returns [`TestFailure`] when applying the transition violates a
  /// post-condition or the system under test cannot advance.
  fn apply(
    state: Self::SystemUnderTest,
    ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    transition: <Self::Reference as ReferenceStateMachine>::Transition,
  ) -> Result<Self::SystemUnderTest, TestFailure>;

  /// Check some invariant on the SUT state after every transition,
  /// returning a violated invariant as a [`TestFailure`] instead of
  /// panicking. The default implementation checks nothing.
  ///
  /// Note that just like in [`StateMachineTest::apply`] you can use
  /// the `ref_state` to compare it with your SUT.
  ///
  /// # Errors
  ///
  /// Returns [`TestFailure`] when an invariant is violated.
  fn check_invariants(_: &Self::SystemUnderTest, _: &<Self::Reference as ReferenceStateMachine>::State) -> TestResult {
    Ok(())
  }

  /// Override this function to add some teardown logic on the SUT state
  /// at the end of each test case, returning a teardown failure as a
  /// [`TestFailure`]. The default implementation simply drops the state.
  ///
  /// # Errors
  ///
  /// Returns [`TestFailure`] when custom teardown detects a violated
  /// cleanup condition.
  #[allow(
    clippy::single_call_fn,
    reason = "default no-op teardown hook for per-case state-machine-test cleanup"
  )]
  fn teardown(state: Self::SystemUnderTest, ref_state: <Self::Reference as ReferenceStateMachine>::State) -> TestResult {
    // Consume the arguments; the default teardown just drops them.
    drop(state);
    drop(ref_state);
    Ok(())
  }

  /// Run the test sequentially, returning the strict test verdict.
  /// You typically don't need to override this method.
  ///
  /// # Errors
  ///
  /// Returns [`TestFailure`] when applying a transition, checking
  /// invariants, or tearing down the system under test fails.
  fn test_sequential(
    config: Config,
    mut ref_state: <Self::Reference as ReferenceStateMachine>::State,
    transitions: Vec<<Self::Reference as ReferenceStateMachine>::Transition>,
    mut seen_counter: Option<Arc<AtomicUsize>>,
  ) -> TestResult {
    #[cfg(feature = "std")]
    use proptest::test_runner::INFO_LOG;

    #[cfg(feature = "std")]
    let trans_len = transitions.len();
    #[cfg(feature = "std")]
    if config.verbose >= INFO_LOG {
      emit_diagnostic_line(format_args!(""));
      emit_diagnostic_line(format_args!("Running a test case with {trans_len} transitions."));
    }
    #[cfg(not(feature = "std"))]
    drop(config);

    let mut concrete_state = Self::init_test(&ref_state);

    // Check the invariants on the initial state
    Self::check_invariants(&concrete_state, &ref_state)?;

    #[cfg(feature = "std")]
    for (ix, transition) in transitions.into_iter().enumerate() {
      // The counter is `Some` only before shrinking. When it's `Some` it
      // must be incremented before every transition that's being applied
      // to inform the strategy that the transition has been applied for
      // the first step of its shrinking process which removes any unseen
      // transitions.
      if let Some(counter) = seen_counter.as_mut() {
        let _previous_seen = counter.fetch_add(1, atomic::Ordering::SeqCst);
      }

      #[cfg(feature = "std")]
      if config.verbose >= INFO_LOG {
        emit_diagnostic_line(format_args!(""));
        emit_diagnostic_line(format_args!(
          "Applying transition {}/{}: {}",
          ix.saturating_add(1),
          trans_len,
          DebugDiagnostic(&transition)
        ));
      }

      // Apply the transition on the states
      ref_state = <Self::Reference as ReferenceStateMachine>::apply(ref_state, &transition);
      concrete_state = Self::apply(concrete_state, &ref_state, transition)?;

      // Check the invariants after the transition is applied
      Self::check_invariants(&concrete_state, &ref_state)?;
    }

    #[cfg(not(feature = "std"))]
    for transition in transitions {
      // The counter is `Some` only before shrinking. When it's `Some` it
      // must be incremented before every transition that's being applied
      // to inform the strategy that the transition has been applied for
      // the first step of its shrinking process which removes any unseen
      // transitions.
      if let Some(counter) = seen_counter.as_mut() {
        let _previous_seen = counter.fetch_add(1, atomic::Ordering::SeqCst);
      }

      // Apply the transition on the states
      ref_state = <Self::Reference as ReferenceStateMachine>::apply(ref_state, &transition);
      concrete_state = Self::apply(concrete_state, &ref_state, transition)?;

      // Check the invariants after the transition is applied
      Self::check_invariants(&concrete_state, &ref_state)?;
    }

    Self::teardown(concrete_state, ref_state)?;
    Ok(())
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
/// fn run_with_macro() -> ::proptest::strict::TestResult {
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
/// The generated test returns `::proptest::strict::TestResult`: a falsified
/// property surfaces as `TestFailure::PropertyFalsified` carrying the shrunk
/// minimal failing transition sequence instead of a panic.
#[macro_export]
macro_rules! prop_state_machine {
    // With proptest config annotation
    (#![proptest_config($config:expr)]
    $(
        $(#[$meta:meta])*
        fn $test_name:ident(sequential $size:expr => $test:ident $(< $( $ty_param:tt ),+ >)?);
    )*) => {
        $(
            $(#[$meta])*
            fn $test_name() -> ::proptest::strict::TestResult {
                let strategy = <<$test $(< $( $ty_param ),+ >)? as $crate::StateMachineTest>::Reference as $crate::ReferenceStateMachine>::sequential_strategy($size);
                ::proptest::strict::ensure_property(
                    &strategy,
                    stringify!($test_name),
                    |(initial_state, transitions, seen_counter)| {
                        // Evaluated per generated case, matching the legacy
                        // per-case `__sugar_to_owned` evaluation. The strict
                        // state-machine adapter preserves local driver fields
                        // such as `verbose`, while keeping persistence owned by
                        // the outer strict property runner.
                        let config = $crate::strict_state_machine_config_from(
                            $config.__sugar_to_owned()
                        );
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
        $(
            $(#[$meta])*
            fn $test_name() -> ::proptest::strict::TestResult {
                let strategy = <<$test $(< $( $ty_param ),+ >)? as $crate::StateMachineTest>::Reference as $crate::ReferenceStateMachine>::sequential_strategy($size);
                ::proptest::strict::ensure_property(
                    &strategy,
                    stringify!($test_name),
                    |(initial_state, transitions, seen_counter)| {
                        <$test $(::< $( $ty_param ),+ >)? as $crate::StateMachineTest>::test_sequential(
                            $crate::strict_state_machine_config(), initial_state, transitions, seen_counter)
                    },
                )
            }
        )*
    };
}

#[cfg(test)]
mod tests {

  mod macro_test {
    //! tests to verify that invocations of all forms of the
    //! `prop_state_machine!` macro compile cleanly, and hygenically,
    //!  as intended.

    use proptest::strategy::BoxedStrategy;
    use proptest::strict::TestFailure;
    use proptest::test_runner::Config;

    // The hand-written trait impls import their concrete types, but the
    // macro invocations below still receive no helper imports.

    /// A no-op test. Exists strictly as something to reference
    /// in the macro invocation.
    struct Test;
    impl crate::ReferenceStateMachine for Test {
      type State = ();
      type Transition = ();

      fn init_state() -> BoxedStrategy<Self::State> {
        use proptest::prelude::*;
        Just(()).boxed()
      }

      fn transitions(_state: &Self::State) -> BoxedStrategy<Self::Transition> {
        use proptest::prelude::*;
        Just(()).boxed()
      }

      fn apply(_state: Self::State, _transition: &Self::Transition) -> Self::State {}
    }

    impl crate::StateMachineTest for Test {
      type SystemUnderTest = ();

      type Reference = Self;

      fn init_test(_state: &<Self::Reference as crate::ReferenceStateMachine>::State) -> Self::SystemUnderTest {}

      fn apply(
        _sut: Self::SystemUnderTest,
        _ref_state: &<Self::Reference as crate::ReferenceStateMachine>::State,
        _transition: <Self::Reference as crate::ReferenceStateMachine>::Transition,
      ) -> Result<Self::SystemUnderTest, TestFailure> {
        Ok(())
      }
    }

    // Invocation of the `prop_state_machine` macro without
    // a `![proptest_config]` annotation
    prop_state_machine! {
        #[test]
        fn no_config_annotation(sequential 1..2 => Test);
    }

    // Invocation of the `prop_state_machine` macro with a
    // `![proptest_config]` annotation
    prop_state_machine! {
        #![proptest_config(Config::default())]

        #[test]
        fn with_config_annotation(sequential 1..2 => Test);
    }
  }

  mod strict_behavior {
    //! Positive-polarity coverage: a model with real state-mutating
    //! transitions runs through the strict macro path and the
    //! `test_sequential` driver, returning `Ok(())` end to end.

    use proptest::strategy::BoxedStrategy;
    use proptest::strategy::Just;
    use proptest::strategy::Strategy as _;
    use proptest::strict::TestFailure;
    use proptest::strict::TestResult;
    use strict_test_support::capture_ignored_test;
    use strict_test_support::ensure;
    use strict_test_support::ensure_eq;
    use strict_test_support::ensure_some;

    use crate::ReferenceStateMachine;
    use crate::StateMachineTest;

    #[derive(Clone, Debug, PartialEq)]
    enum Op {
      Push(u8),
      Pop,
    }

    #[allow(
      clippy::single_call_fn,
      reason = "the stack model names state-dependent transition choices separately from the trait adapter"
    )]
    fn stack_transition_choices(state: &[u8]) -> BoxedStrategy<Op> {
      use proptest::prelude::any;

      if state.is_empty() {
        return any::<u8>().prop_map(Op::Push).boxed();
      }

      proptest::prop_oneof![any::<u8>().prop_map(Op::Push), Just(Op::Pop),].boxed()
    }

    #[allow(
      clippy::single_call_fn,
      reason = "the stack model names the pop transition's effect on reference state"
    )]
    fn pop_stack_model(state: &mut Vec<u8>) {
      let _popped = state.pop();
    }

    #[allow(
      clippy::single_call_fn,
      reason = "the stack SUT names the Pop post-condition separately from transition dispatch"
    )]
    fn apply_stack_pop(state: &mut Vec<u8>) -> Result<(), TestFailure> {
      ensure(!state.is_empty(), "Pop only reaches a non-empty stack")?;
      let _popped = ensure_some(state.pop(), "Pop removes an element from a non-empty stack")?;
      Ok(())
    }

    /// Model: a stack of bytes. `Pop` is precondition-gated to non-empty
    /// states, so generation filters invalid transitions at the strategy
    /// level.
    struct StackModel;

    impl ReferenceStateMachine for StackModel {
      type State = Vec<u8>;
      type Transition = Op;

      fn init_state() -> BoxedStrategy<Self::State> {
        Just(Vec::new()).boxed()
      }

      fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        stack_transition_choices(state)
      }

      fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match *transition {
          Op::Push(value) => state.push(value),
          Op::Pop => pop_stack_model(&mut state),
        }
        state
      }

      fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match *transition {
          Op::Pop => !state.is_empty(),
          Op::Push(_) => true,
        }
      }
    }

    /// SUT mirroring the model; every transition re-checks the
    /// precondition and the model alignment through `ensure`.
    struct StackSut;

    impl StateMachineTest for StackSut {
      type SystemUnderTest = Vec<u8>;
      type Reference = StackModel;

      fn init_test(ref_state: &Vec<u8>) -> Self::SystemUnderTest {
        ref_state.clone()
      }

      fn apply(mut state: Self::SystemUnderTest, ref_state: &Vec<u8>, transition: Op) -> Result<Self::SystemUnderTest, TestFailure> {
        match transition {
          Op::Push(value) => state.push(value),
          Op::Pop => apply_stack_pop(&mut state)?,
        }
        ensure(state == *ref_state, "the SUT mirrors the model after every transition")?;
        Ok(state)
      }

      fn check_invariants(state: &Self::SystemUnderTest, ref_state: &Vec<u8>) -> TestResult {
        ensure(state.len() == ref_state.len(), "SUT and model agree on the stack depth")
      }
    }

    const STACK_MACRO_CHILD: &str = concat!(
      "test_runner::tests::strict_behavior::",
      "passing_stack_model_runs_through_the_strict_macro_child"
    );

    // The macro path itself is the positive proof: the expansion is an
    // ordinary `#[test]` returning `TestResult`, so a passing model means
    // the harness sees `Ok(())` from a run whose generated transitions
    // really mutate state. The actual macro run lives in an ignored child
    // test so this wrapper can re-exec it through `strict-test-support`
    // output capture and prove state-machine diagnostics do not leak to
    // the parent harness.
    prop_state_machine! {
        #[test]
        #[ignore = "captured by passing_stack_model_runs_through_the_strict_macro"]
        fn passing_stack_model_runs_through_the_strict_macro_child(
            sequential 1..16 => StackSut
        );
    }

    #[test]
    fn passing_stack_model_runs_through_the_strict_macro() -> Result<(), TestFailure> {
      let captured = capture_ignored_test(STACK_MACRO_CHILD)?;
      ensure(captured.status.success(), "the captured state-machine macro child passes")?;
      ensure_eq(
        &captured.stderr,
        &String::new(),
        "the captured state-machine macro child emits no stderr",
      )
    }

    /// Driving `test_sequential` directly with a hand-built valid
    /// sequence returns `Ok(())`.
    #[test]
    fn test_sequential_returns_ok_for_a_valid_sequence() -> Result<(), TestFailure> {
      let transitions = vec![Op::Push(1), Op::Push(2), Op::Pop];
      <StackSut as StateMachineTest>::test_sequential(crate::strict_state_machine_config(), Vec::new(), transitions, None)
    }
  }
}
