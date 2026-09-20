// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Explicit, statically dispatched transport for typed forked properties.

use core::convert::Infallible;
use core::marker::PhantomData;

use super::CaseOrigin;
use super::EvaluationId;
use super::EvaluationOutcome;
use super::ExecutionError;
use crate::std_facade::Vec;

/// A decoded case and native callback return, or the codec's concrete failure.
pub type DecodedEvaluation<V, A, E, T> = Result<(V, Result<A, E>), T>;

/// Replay may provide an evaluation, admit a new callback, or fail with native engine evidence.
pub(super) type ReplayOutcome<A, E, T> = Result<Option<EvaluationOutcome<A, E>>, ExecutionError<T>>;

/// The caller-selected representation of values crossing a property fork.
///
/// The runner owns framing, process lifecycle, timeouts, replay, and temporary
/// files. This codec owns only the representation of the declared case and
/// callback types, including its own concrete failures. It is never required
/// for in-process properties.
///
/// Encoding must preserve all values promised by the property API. A resource
/// meaningful only inside one process needs an explicit transferable model;
/// the runner cannot infer one. Decoding must reject malformed payloads.
pub trait PropertyTransport<V, A, E> {
  /// Concrete failures of this codec, preserved in the run's engine outcome.
  type Error;

  /// Encode case state before an evaluation which may terminate the process.
  ///
  /// # Errors
  /// Returns the concrete failure to encode the case.
  fn encode_case(&mut self, case: &V) -> Result<Vec<u8>, Self::Error>;

  /// Decode a case recorded before an interrupted evaluation.
  ///
  /// # Errors
  /// Returns the concrete failure to decode the case.
  fn decode_case(&mut self, payload: &[u8]) -> Result<V, Self::Error>;

  /// Encode one returned result and the case state observed after its callback.
  ///
  /// # Errors
  /// Returns the concrete encoding failure without discarding previous records.
  fn encode(&mut self, case: &V, result: &Result<A, E>) -> Result<Vec<u8>, Self::Error>;

  /// Recover the native case and returned result from one complete payload.
  ///
  /// # Errors
  /// Returns the concrete decoding failure for an invalid payload.
  fn decode(&mut self, payload: &[u8]) -> DecodedEvaluation<V, A, E, Self::Error>;

  /// Restore shared state in the regenerated case before the next shrink step.
  ///
  /// For example, a state-machine codec restores its seen-transition counter
  /// into the regenerated value tree's shared counter. Stateless cases can
  /// return success without mutation. This must never invoke the property.
  ///
  /// # Errors
  /// Returns a concrete failure when the generated and transported cases
  /// cannot be reconciled faithfully.
  fn restore(&mut self, generated: &V, transported: &V) -> Result<(), Self::Error>;

  /// Restore the state needed to shrink a case whose callback never returned.
  ///
  /// The transported value is its last published state, which can precede the
  /// interruption. A stateful codec must conservatively retain work whose
  /// execution is uncertain; it must not treat unpublished progress as proof
  /// that the work was never attempted.
  ///
  /// # Errors
  /// Returns a concrete failure when the interrupted state cannot be restored.
  fn restore_interrupted(&mut self, generated: &V, transported: &V) -> Result<(), Self::Error>;

  /// Encode a codec failure for delivery from a child to its parent.
  ///
  /// This operation is infallible so a failed value encoder still has an
  /// independent error-reporting path. Native I/O failures remain separate.
  fn encode_error(&mut self, error: &Self::Error) -> Vec<u8>;

  /// Decode a child's codec failure, or return the failure decoding that record.
  ///
  /// # Errors
  /// Returns the native decoding error when an error record is malformed.
  fn decode_error(&mut self, payload: &[u8]) -> Result<Self::Error, Self::Error>;
}

/// The execution adapter's replay and recording capability.
pub(super) trait EvaluationChannel<V, A, E> {
  /// Concrete transport failure, uninhabited for local execution.
  type Error;

  /// Replay one evaluation and restore its shared state, or admit a new one.
  fn replay(&mut self, id: EvaluationId, case: &mut V, origin: CaseOrigin) -> ReplayOutcome<A, E, Self::Error>;

  /// Publish one complete evaluation record.
  fn record(
    &mut self,
    id: EvaluationId,
    case: &V,
    origin: CaseOrigin,
    outcome: &EvaluationOutcome<A, E>,
  ) -> Result<(), ExecutionError<Self::Error>>;

  /// Record or replay a complication performed without calling the property.
  fn backtrack(&mut self) -> Result<(), ExecutionError<Self::Error>>;

  /// Preserve the child's shrink-budget decision during replay.
  fn shrink_budget(&mut self, exhausted: bool) -> Result<bool, ExecutionError<Self::Error>>;

  /// Complete a normal traversal and finalize output on every stopping path.
  /// An engine-stopped traversal must not demand a fictional completion frame.
  fn finish(&mut self, completed: bool) -> Vec<ExecutionError<Self::Error>>;

  /// Whether the parent, rather than this process, owns persistence.
  fn is_in_fork(&self) -> bool;
}

/// Local execution has no transport and imposes no codec bounds.
pub(super) struct InProcess<T = Infallible>(pub(super) PhantomData<fn() -> T>);

impl<V, A, E, T> EvaluationChannel<V, A, E> for InProcess<T> {
  type Error = T;

  fn replay(&mut self, _: EvaluationId, _: &mut V, _: CaseOrigin) -> ReplayOutcome<A, E, T> {
    Ok(None)
  }

  fn record(&mut self, _: EvaluationId, _: &V, _: CaseOrigin, _: &EvaluationOutcome<A, E>) -> Result<(), ExecutionError<T>> {
    Ok(())
  }

  fn backtrack(&mut self) -> Result<(), ExecutionError<T>> {
    Ok(())
  }

  fn shrink_budget(&mut self, exhausted: bool) -> Result<bool, ExecutionError<T>> {
    Ok(exhausted)
  }

  fn finish(&mut self, _: bool) -> Vec<ExecutionError<T>> {
    Vec::new()
  }

  fn is_in_fork(&self) -> bool {
    false
  }
}
