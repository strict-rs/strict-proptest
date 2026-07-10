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
use std::sync::atomic::{self, AtomicUsize};

use crate::strategy::ReferenceStateMachine;
use proptest::strict::{TestFailure, TestResult};
use proptest::test_runner::Config;

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
    fn init_test(
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest;

    /// Apply a transition in the SUT state and check post-conditions.
    /// The post-conditions are properties of your state machine that you want
    /// to uphold; express them with the `strict_test_support` `ensure*`
    /// helpers (or any other `TestFailure` constructor) and propagate them
    /// with `?`, returning the new SUT state on success.
    ///
    /// Note that the `ref_state` is the state *after* this `transition` is
    /// applied. You can use it to compare it with your SUT after you apply
    /// the transition.
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
    fn check_invariants(
        state: &Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> TestResult {
        // This is to avoid `unused_variables` warning
        let _ = (state, ref_state);
        Ok(())
    }

    /// Override this function to add some teardown logic on the SUT state
    /// at the end of each test case, returning a teardown failure as a
    /// [`TestFailure`]. The default implementation simply drops the state.
    #[allow(
        clippy::single_call_fn,
        reason = "default no-op teardown hook for per-case state-machine-test cleanup"
    )]
    fn teardown(
        state: Self::SystemUnderTest,
        ref_state: <Self::Reference as ReferenceStateMachine>::State,
    ) -> TestResult {
        // Consume the arguments; the default teardown just drops them.
        drop(state);
        drop(ref_state);
        Ok(())
    }

    /// Run the test sequentially, returning the strict test verdict.
    /// You typically don't need to override this method.
    fn test_sequential(
        config: Config,
        mut ref_state: <Self::Reference as ReferenceStateMachine>::State,
        transitions: Vec<
            <Self::Reference as ReferenceStateMachine>::Transition,
        >,
        mut seen_counter: Option<Arc<AtomicUsize>>,
    ) -> TestResult {
        #[cfg(feature = "std")]
        use proptest::test_runner::INFO_LOG;

        let trans_len = transitions.len();
        #[cfg(feature = "std")]
        if config.verbose >= INFO_LOG {
            eprintln!();
            eprintln!("Running a test case with {} transitions.", trans_len);
        }
        #[cfg(not(feature = "std"))]
        drop((config, trans_len));

        let mut concrete_state = Self::init_test(&ref_state);

        // Check the invariants on the initial state
        Self::check_invariants(&concrete_state, &ref_state)?;

        for (ix, transition) in transitions.into_iter().enumerate() {
            // The counter is `Some` only before shrinking. When it's `Some` it
            // must be incremented before every transition that's being applied
            // to inform the strategy that the transition has been applied for
            // the first step of its shrinking process which removes any unseen
            // transitions.
            if let Some(seen_counter) = seen_counter.as_mut() {
                let _previous_seen =
                    seen_counter.fetch_add(1, atomic::Ordering::SeqCst);
            }

            #[cfg(feature = "std")]
            if config.verbose >= INFO_LOG {
                eprintln!();
                eprintln!(
                    "Applying transition {}/{}: {:?}",
                    ix + 1,
                    trans_len,
                    transition
                );
            }
            #[cfg(not(feature = "std"))]
            let _ = ix;

            // Apply the transition on the states
            ref_state = <Self::Reference as ReferenceStateMachine>::apply(
                ref_state,
                &transition,
            );
            concrete_state =
                Self::apply(concrete_state, &ref_state, transition)?;

            // Check the invariants after the transition is applied
            Self::check_invariants(&concrete_state, &ref_state)?;
        }

        Self::teardown(concrete_state, ref_state)?;
        Ok(())
    }
}

/// This macro helps to turn a state machine test implementation into a runnable
/// test. The macro expects a function header whose arguments follow a special
/// syntax rules: First, we declare if we want to apply the state machine
/// transitions sequentially or concurrently (currently, only the `sequential`
/// is supported). Next, we give a range of how many transitions to generate,
/// followed by `=>` and finally, an identifier that must implement
/// `StateMachineTest`.
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
///                 ::proptest::test_runner::Config::default(),
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
                        // per-case `__sugar_to_owned` evaluation; it now feeds
                        // only `test_sequential`'s verbose logging because the
                        // strict runner builds its own configuration.
                        let config = $config.__sugar_to_owned();
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
                            ::proptest::test_runner::Config::default(), initial_state, transitions, seen_counter)
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

        // Note: no imports here, so as to guarantee hygienic macros

        /// A no-op test. Exists strictly as something to reference
        /// in the macro invocation.
        struct Test;
        impl crate::ReferenceStateMachine for Test {
            type State = ();
            type Transition = ();

            fn init_state() -> proptest::strategy::BoxedStrategy<Self::State> {
                use proptest::prelude::*;
                Just(()).boxed()
            }

            fn transitions(
                _: &Self::State,
            ) -> proptest::strategy::BoxedStrategy<Self::Transition>
            {
                use proptest::prelude::*;
                Just(()).boxed()
            }

            fn apply(_: Self::State, _: &Self::Transition) -> Self::State {}
        }

        impl crate::StateMachineTest for Test {
            type SystemUnderTest = ();

            type Reference = Self;

            fn init_test(
                _: &<Self::Reference as crate::ReferenceStateMachine>::State,
            ) -> Self::SystemUnderTest {
            }

            fn apply(
                _: Self::SystemUnderTest,
                _: &<Self::Reference as crate::ReferenceStateMachine>::State,
                _: <Self::Reference as crate::ReferenceStateMachine>::Transition,
            ) -> Result<Self::SystemUnderTest, proptest::strict::TestFailure>
            {
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
            #![proptest_config(::proptest::test_runner::Config::default())]

            #[test]
            fn with_config_annotation(sequential 1..2 => Test);
        }
    }

    mod strict_behavior {
        //! Positive-polarity coverage: a model with real state-mutating
        //! transitions runs through the strict macro path and the
        //! `test_sequential` driver, returning `Ok(())` end to end.

        use proptest::strategy::{BoxedStrategy, Just, Strategy};
        use proptest::strict::{TestFailure, TestResult};
        use proptest::test_runner::Config;
        use strict_test_support::{ensure, ensure_some};

        use crate::{ReferenceStateMachine, StateMachineTest};

        #[derive(Clone, Debug, PartialEq)]
        enum Op {
            Push(u8),
            Pop,
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

            fn transitions(
                state: &Self::State,
            ) -> BoxedStrategy<Self::Transition> {
                use proptest::prelude::any;
                if state.is_empty() {
                    any::<u8>().prop_map(Op::Push).boxed()
                } else {
                    proptest::prop_oneof![
                        any::<u8>().prop_map(Op::Push),
                        Just(Op::Pop),
                    ]
                    .boxed()
                }
            }

            fn apply(
                mut state: Self::State,
                transition: &Self::Transition,
            ) -> Self::State {
                match transition {
                    Op::Push(value) => state.push(*value),
                    Op::Pop => {
                        let _popped = state.pop();
                    }
                }
                state
            }

            fn preconditions(
                state: &Self::State,
                transition: &Self::Transition,
            ) -> bool {
                match transition {
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

            fn apply(
                mut state: Self::SystemUnderTest,
                ref_state: &Vec<u8>,
                transition: Op,
            ) -> Result<Self::SystemUnderTest, TestFailure> {
                match transition {
                    Op::Push(value) => state.push(value),
                    Op::Pop => {
                        ensure(
                            !state.is_empty(),
                            "Pop only reaches a non-empty stack",
                        )?;
                        let _popped = ensure_some(
                            state.pop(),
                            "Pop removes an element from a non-empty stack",
                        )?;
                    }
                }
                ensure(
                    state == *ref_state,
                    "the SUT mirrors the model after every transition",
                )?;
                Ok(state)
            }

            fn check_invariants(
                state: &Self::SystemUnderTest,
                ref_state: &Vec<u8>,
            ) -> TestResult {
                ensure(
                    state.len() == ref_state.len(),
                    "SUT and model agree on the stack depth",
                )
            }
        }

        // The macro path itself is the positive proof: the expansion is an
        // ordinary `#[test]` returning `TestResult`, so a passing model
        // means the harness sees `Ok(())` from a run whose generated
        // transitions really mutate state.
        prop_state_machine! {
            #[test]
            fn passing_stack_model_runs_through_the_strict_macro(
                sequential 1..16 => StackSut
            );
        }

        /// Driving `test_sequential` directly with a hand-built valid
        /// sequence returns `Ok(())`.
        #[test]
        fn test_sequential_returns_ok_for_a_valid_sequence()
        -> Result<(), TestFailure> {
            let transitions = vec![Op::Push(1), Op::Push(2), Op::Pop];
            <StackSut as StateMachineTest>::test_sequential(
                Config::default(),
                Vec::new(),
                transitions,
                None,
            )
        }
    }
}
