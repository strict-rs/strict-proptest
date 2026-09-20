// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Owned evidence returned by native typed property execution.

use core::convert::Infallible;
use core::num::TryFromIntError;
use core::str::Utf8Error;
#[cfg(feature = "std")]
use std::io;
#[cfg(feature = "std")]
use std::path::PathBuf;
#[cfg(any(feature = "std", test))]
use std::process::ExitCode;
#[cfg(any(feature = "std", test))]
use std::process::Termination;

use thiserror::Error;

use super::CaseOrigin;
use super::EvaluationId;
use super::Reason;
use crate::std_facade::Box;
use crate::std_facade::Vec;

/// A callback's returned values, or evidence that it could not return.
#[derive(Debug)]
pub enum EvaluationOutcome<A, E> {
  /// The original successful subject or assertion failure.
  Returned(Result<A, E>),
  /// A successful callback returned after its configured timeout. Its native
  /// result remains available even though the engine rejects the evaluation.
  TimedOut {
    /// The callback's returned result, preserved without conversion.
    result:     Result<A, E>,
    /// Configured time limit.
    timeout_ms: u32,
    /// Observed callback duration.
    elapsed_ms: u64,
  },
  /// The callback was interrupted without returning an assertion failure.
  Interrupted(Interruption),
  /// This record's failure is owned by the enclosing falsification's cause.
  ///
  /// Its identity and position remain in the run's ordered evidence. The
  /// payload occurs exactly once, in `PropertyCause`.
  RetainedFailure,
}

/// One actual callback evaluation, including evaluations used during shrinking.
#[derive(Debug)]
pub struct Evaluation<A, E> {
  /// Why this candidate was evaluated.
  pub origin:  CaseOrigin,
  /// The native callback result or interruption.
  pub outcome: EvaluationOutcome<A, E>,
}

/// A step in the execution history, without copying any property payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionEvent {
  /// An actual evaluation whose evidence is in the indexed record.
  Evaluated(EvaluationId),
  /// A cache hit referring to an existing evaluation.
  Cached(EvaluationId),
  /// A complication performed when a shrink budget was exhausted.
  Backtracked,
}

/// Native case counts at the run's stopping boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RunStatistics {
  /// Fresh passing cases counted toward `Config::cases`.
  pub successes:      u32,
  /// Inputs rejected inside strategies.
  pub local_rejects:  u32,
  /// Cases rejected by the legacy property contract.
  pub global_rejects: u32,
}

/// Complete reached evidence, including successful subjects and intermediate failures.
///
/// Retained subjects live until this report is consumed or dropped. They need
/// not implement `Clone`, `Send`, `Sync`, or any serialization trait.
#[derive(Debug)]
pub struct PropertyRun<A, E> {
  /// The calling property's diagnostic context.
  pub context:         &'static str,
  /// Labels for the elements of a generated property's argument tuple.
  pub argument_labels: &'static [&'static str],
  /// Actual evaluations in execution order.
  pub evaluations:     Vec<Evaluation<A, E>>,
  /// Evaluations, cache references, and backtracking in traversal order.
  pub events:          Vec<ExecutionEvent>,
  /// Native runner counters at the boundary.
  pub statistics:      RunStatistics,
}

impl<A, E> Default for PropertyRun<A, E> {
  fn default() -> Self {
    Self {
      context:         "property",
      argument_labels: &[],
      evaluations:     Vec::new(),
      events:          Vec::new(),
      statistics:      RunStatistics::default(),
    }
  }
}

#[cfg(any(feature = "std", test))]
impl<A, E> Termination for PropertyRun<A, E> {
  fn report(self) -> ExitCode {
    drop(self);
    ExitCode::SUCCESS
  }
}

/// The annotated result contract of a generated property function.
///
/// This projection lets attribute macros preserve aliases for `Result<A, E>`
/// without inspecting or rewriting the alias's success and failure types.
pub trait PropertyReturn {
  /// The successful subject returned by the property.
  type Success;
  /// The original assertion failure returned by the property.
  type Failure;

  /// Expose the result without converting either payload.
  ///
  /// # Errors
  /// Returns the original property failure unchanged.
  fn into_result(self) -> Result<Self::Success, Self::Failure>;
}

impl<A, E> PropertyReturn for Result<A, E> {
  type Success = A;
  type Failure = E;

  fn into_result(self) -> Self {
    self
  }
}

/// Evidence that the engine interrupted an evaluation.
#[derive(Debug, Error)]
pub enum Interruption {
  /// An unwinding panic caught at the native property boundary.
  #[error("property panicked: {0}")]
  Panicked(Reason),
  /// The parent terminated a child that stopped making progress.
  #[error("property exceeded its {timeout_ms} ms timeout")]
  TimedOut {
    /// Configured time limit.
    timeout_ms: u32,
  },
  /// A child exited before completing the evaluation.
  #[error("property child exited (code {code:?}, signal {signal:?})")]
  ChildExited {
    /// Exit code, when the operating system supplies one.
    code:   Option<i32>,
    /// Terminating signal on Unix, absent on other platforms.
    signal: Option<i32>,
  },
}

/// Failures of configuration, execution infrastructure, or declared transport.
#[derive(Debug, Error)]
pub enum ExecutionError<T = Infallible> {
  /// Fork or timeout was requested on an entry point without a transport.
  #[error("fork or timeout requires an explicit property transport")]
  TransportRequired,
  /// Forking requires the test harness name for child re-execution.
  #[error("forked property execution requires Config::test_name")]
  TestNameRequired,
  /// A child terminated before publishing the start of any new evaluation.
  #[error("child terminated before an evaluation started: {0}")]
  ChildBeforeCase(Interruption),
  /// The bounded native child-restart policy was exhausted.
  #[error("giving up after {children} child processes terminated")]
  RestartLimit {
    /// Number of child processes used.
    children: u32,
  },
  /// A cache returned an identity outside this run's evaluation records.
  #[error("cache returned invalid evaluation {0:?}")]
  InvalidEvaluation(EvaluationId),
  /// A caller-supplied codec failed, preserving its concrete error.
  #[error("property transport failed: {0:?}")]
  Transport(T),
  /// Native I/O failed while exchanging or finalizing evidence.
  #[error("property transport I/O failed: {0}")]
  #[cfg(feature = "std")]
  Io(#[source] io::Error),
  /// The runner could not remove its temporary transport file.
  #[error("failed to finalize property transport file {path:?}: {error}")]
  #[cfg(feature = "std")]
  Cleanup {
    /// The temporary path whose removal failed.
    path:  PathBuf,
    /// The native filesystem failure.
    #[source]
    error: io::Error,
  },
  /// The fork launcher failed before it could produce child evidence.
  #[cfg(feature = "fork")]
  #[error("property fork failed: {0}")]
  Fork(#[source] rusty_fork::Error),
  /// The typed wire stream violated its framing contract.
  #[error("invalid property replay: {0}")]
  Protocol(#[source] ReplayError),
}

/// A malformed or incomplete typed replay stream.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ReplayError {
  /// The stream's protocol/version header was not recognized.
  #[error("unrecognized protocol header")]
  Header,
  /// A record ended before its declared payload was complete.
  #[error("truncated record")]
  Truncated,
  /// A record tag was not part of this protocol version.
  #[error("unknown record kind {0}")]
  RecordKind(u8),
  /// An evaluation was out of order or had the wrong origin.
  #[error("evaluation identity or origin does not match traversal")]
  Identity,
  /// A record length could not be represented on this platform.
  #[error("record length exceeds the platform address space")]
  Length(#[source] TryFromIntError),
  /// Interruption text was not valid UTF-8.
  #[error("invalid interruption text: {0}")]
  Encoding(#[source] Utf8Error),
  /// Replay ended before the native algorithm finished.
  #[error("replay ended before the run completed")]
  Exhausted,
  /// The stream contained records after the algorithm completed.
  #[error("unconsumed records after run completion")]
  Trailing,
}

/// The stopping cause, separate from the complete evidence already collected.
#[derive(Debug, Error)]
pub enum PropertyCause<V, E, T = Infallible> {
  /// An original assertion failure paired with its minimized input.
  #[error("property falsified by {counterexample:?}: {failure:?}")]
  Falsified {
    /// The exact candidate associated with this failure.
    counterexample: V,
    /// The callback's original concrete failure.
    failure:        E,
    /// Position of the corresponding `RetainedFailure` record.
    evaluation:     EvaluationId,
  },
  /// Generation or rejection limits prevented the run from continuing.
  #[error("property aborted: {0}")]
  Aborted(Reason),
  /// A minimized input interrupted the callback instead of returning an `E`.
  #[error("property interrupted by {counterexample:?}: {interruption}")]
  Interrupted {
    /// The established failing input.
    counterexample: V,
    /// The engine's observed interruption.
    interruption:   Interruption,
    /// Position of the corresponding `RetainedFailure` record.
    evaluation:     EvaluationId,
  },
  /// Evaluation or transport infrastructure could not continue.
  #[error(transparent)]
  Engine(ExecutionError<T>),
}

/// An established failing candidate retained when infrastructure stopped shrinking.
/// Its original outcome remains in the enclosing run at `evaluation`.
#[derive(Debug)]
pub struct EstablishedFailure<V> {
  /// Exact candidate from the last established failure.
  pub counterexample: V,
  /// Identity of its still-owned native failure or interruption.
  pub evaluation:     EvaluationId,
}

/// A failed run with all reached evidence and any finalization failures.
#[derive(Debug, Error)]
#[error("{context}: {cause}")]
pub struct PropertyFailure<V, A, E, T = Infallible> {
  /// Context attached by the caller or strict façade.
  pub context:             &'static str,
  /// Every evaluation reached before stopping.
  pub run:                 PropertyRun<A, E>,
  /// The native stopping cause.
  pub cause:               PropertyCause<V, E, T>,
  /// Last established failing pair when an engine error interrupted shrinking.
  pub established_failure: Option<EstablishedFailure<V>>,
  /// Infrastructure failures while finalizing an already-failed run.
  pub finalization:        Vec<ExecutionError<T>>,
}

/// The typed runner result; boxing keeps large subjects out of the stack error path.
pub type PropertyResult<V, A, E, T = Infallible> = Result<PropertyRun<A, E>, Box<PropertyFailure<V, A, E, T>>>;
