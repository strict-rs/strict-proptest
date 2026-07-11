//-
// Copyright 2019 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{Arc, Box, Rc, fmt};
use core::mem;

use crate::strategy::traits::{NewTree, Strategy};
use crate::test_runner::TestRunner;

/// Represents a value tree that is initialized on the first call to any
/// methods.
///
/// This is used to defer potentially expensive generation to shrinking time. It
/// is public only to allow APIs to expose it as an intermediate value.
pub struct LazyValueTree<S: Strategy> {
    /// The current generation state: either the deferred inputs, the tree once
    /// built, or the failed marker.
    state: LazyValueTreeState<S>,
}

/// The generation state of a `LazyValueTree`, tracking whether its inner tree
/// has yet been produced.
enum LazyValueTreeState<S: Strategy> {
    /// The inner value tree has been generated and is ready to use.
    Initialized(S::Tree),
    /// The inner value tree has not been generated yet; it will be built from
    /// the retained strategy and runner on first use.
    Uninitialized {
        /// The strategy whose value tree will be generated on demand.
        strategy: LazyValueTreeStrategy<S>,
        /// The runner clone to generate that value tree with.
        runner: Box<TestRunner>,
    },
    /// Generation was attempted and failed; the tree stays permanently empty.
    Failed,
}

/// The retained strategy handle used while `LazyValueTree` is still deferred.
///
/// `TupleUnion` uses local `Rc` sharing, while dynamic `Union` keeps `Arc`
/// sharing so type-erased thread-safe strategies remain `Send + Sync`.
enum LazyValueTreeStrategy<S: Strategy> {
    /// Locally-shared strategy handle for static tuple-union branches.
    Rc(Rc<S>),
    /// Atomically-shared strategy handle for dynamic union branches.
    Arc(Arc<S>),
}

impl<S: Strategy> LazyValueTreeStrategy<S> {
    /// Generate the deferred value tree through the stored handle.
    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<S> {
        match *self {
            Self::Rc(ref strategy) => strategy.as_ref().new_tree(runner),
            Self::Arc(ref strategy) => strategy.as_ref().new_tree(runner),
        }
    }
}

impl<S: Strategy> LazyValueTree<S> {
    /// Create a new value tree where initial generation is deferred until
    /// `maybe_init` is called.
    pub(crate) fn new(strategy: Rc<S>, runner: &mut TestRunner) -> Self {
        let runner_clone = runner.partial_clone();
        Self {
            state: LazyValueTreeState::Uninitialized {
                strategy: LazyValueTreeStrategy::Rc(strategy),
                runner: Box::new(runner_clone),
            },
        }
    }

    /// Create a new value tree where initial generation is deferred and the
    /// deferred strategy is stored behind an atomically-counted handle.
    #[allow(
        clippy::single_call_fn,
        reason = "construct the Arc-backed lazy value tree used by shared union strategy branches"
    )]
    pub(crate) fn new_arc(strategy: Arc<S>, runner: &mut TestRunner) -> Self {
        let runner_clone = runner.partial_clone();
        Self {
            state: LazyValueTreeState::Uninitialized {
                strategy: LazyValueTreeStrategy::Arc(strategy),
                runner: Box::new(runner_clone),
            },
        }
    }

    /// Take the initialized inner value tree, leaving this lazy slot failed.
    pub(crate) fn take_initialized(&mut self) -> Option<S::Tree> {
        let state = mem::replace(&mut self.state, LazyValueTreeState::Failed);
        match state {
            LazyValueTreeState::Initialized(tree) => Some(tree),
            LazyValueTreeState::Uninitialized { .. }
            | LazyValueTreeState::Failed => {
                self.state = state;
                None
            }
        }
    }

    /// Try initializing the value tree.
    pub(crate) fn maybe_init(&mut self) {
        if !self.is_uninitialized() {
            return;
        }

        let state = mem::replace(&mut self.state, LazyValueTreeState::Failed);
        if let LazyValueTreeState::Uninitialized {
            strategy,
            mut runner,
        } = state
        {
            if let Ok(tree) = strategy.new_tree(&mut runner) {
                self.state = LazyValueTreeState::Initialized(tree);
            }
        } else {
            self.state = state;
        }
    }

    /// Whether this value tree still needs to be initialized.
    pub(crate) const fn is_uninitialized(&self) -> bool {
        match self.state {
            LazyValueTreeState::Uninitialized { .. } => true,
            LazyValueTreeState::Initialized(_) | LazyValueTreeState::Failed => {
                false
            }
        }
    }
}

impl<S: Strategy> Clone for LazyValueTree<S>
where
    S::Tree: Clone,
{
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<S: Strategy> fmt::Debug for LazyValueTree<S>
where
    S::Tree: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LazyValueTree")
            .field("state", &self.state)
            .finish()
    }
}

impl<S: Strategy> Clone for LazyValueTreeState<S>
where
    S::Tree: Clone,
{
    fn clone(&self) -> Self {
        match *self {
            Self::Initialized(ref tree) => Self::Initialized(tree.clone()),
            Self::Uninitialized {
                ref strategy,
                ref runner,
            } => Self::Uninitialized {
                strategy: strategy.clone(),
                runner: runner.clone(),
            },
            Self::Failed => Self::Failed,
        }
    }
}

impl<S: Strategy> Clone for LazyValueTreeStrategy<S> {
    fn clone(&self) -> Self {
        match *self {
            Self::Rc(ref strategy) => Self::Rc(Rc::clone(strategy)),
            Self::Arc(ref strategy) => Self::Arc(Arc::clone(strategy)),
        }
    }
}

impl<S: Strategy> fmt::Debug for LazyValueTreeState<S>
where
    S::Tree: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Initialized(ref value_tree) => {
                f.debug_tuple("Initialized").field(value_tree).finish()
            }
            Self::Uninitialized { ref strategy, .. } => f
                .debug_struct("Uninitialized")
                .field("strategy", strategy)
                .finish(),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

impl<S: Strategy> fmt::Debug for LazyValueTreeStrategy<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Rc(ref strategy) => fmt::Debug::fmt(strategy, f),
            Self::Arc(ref strategy) => fmt::Debug::fmt(strategy, f),
        }
    }
}
