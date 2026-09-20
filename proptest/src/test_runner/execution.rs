// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Evaluation adapters for the shared generation and shrinking algorithm.

use super::Reason;
use super::TestRunner;
use super::errors::TestCaseOk;

/// Where an evaluation occurs in the native runner's case walk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseOrigin {
  /// A case generated toward the configured success count.
  Generated,
  /// A case replayed from regression persistence.
  Persisted,
  /// An attempt to simplify an established failure.
  Shrink,
}

/// A property decision consumed by the shrink algorithm, separate from payload ownership.
pub(super) enum CaseVerdict<F> {
  /// The case passed with the indicated counting semantics.
  Passed(TestCaseOk),
  /// The case was rejected without falsifying the property.
  Rejected(Reason),
  /// An established failure, owned by or referring to the evaluation adapter.
  Failed(F),
}

/// The common algorithm's stopping condition.
pub(super) enum RunFailure<V, F, X> {
  /// Input generation or the rejection budget aborted the run.
  Aborted(Reason),
  /// The last established failing candidate and its associated failure.
  Falsified(F, V),
  /// Evaluation or transport could not continue.
  Engine {
    /// Infrastructure failure preventing further execution.
    error:   X,
    /// The last established failing pair, when shrinking had already started.
    failing: Option<(F, V)>,
  },
}

impl<V, F, X> RunFailure<V, F, X> {
  /// Construct an engine failure before a failing candidate was established.
  pub(super) const fn engine(error: X) -> Self {
    Self::Engine {
      error,
      failing: None,
    }
  }
}

/// Native algorithm result using the chosen adapter's failure and engine types.
pub(super) type ExecutionResult<O, V, X> = Result<O, RunFailure<V, <X as Execution<V>>::Failure, <X as Execution<V>>::Error>>;

/// The last established failure and the candidate that produced it.
pub(super) type FailingCase<V, X> = (<X as Execution<V>>::Failure, V);

/// Supplies evaluations without imposing payload bounds on the native case walk.
pub(super) trait Execution<V> {
  /// The failure retained alongside a failing candidate during shrinking.
  type Failure;
  /// An engine failure distinct from a property falsification.
  type Error;

  /// Evaluate or replay one candidate, observing shared case state after the callback.
  fn evaluate(
    &mut self,
    runner: &TestRunner,
    observed: &mut V,
    input: V,
    origin: CaseOrigin,
  ) -> Result<CaseVerdict<Self::Failure>, Self::Error>;

  /// Record a complication performed without evaluating the property.
  fn backtrack(&mut self) -> Result<(), Self::Error>;

  /// Replay the recorded budget decision, or record the current local decision.
  fn shrink_budget(&mut self, exhausted: bool) -> Result<bool, Self::Error>;

  /// Whether this execution is a child whose parent owns regression persistence.
  fn is_in_fork(&self) -> bool;
}
