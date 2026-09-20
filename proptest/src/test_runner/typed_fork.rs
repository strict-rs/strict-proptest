// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Process supervision and finalization for the typed native runner.

use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
#[cfg(feature = "timeout")]
use std::time::Duration;

use rusty_fork::ChildWrapper;
use rusty_fork::fork_test;
use rusty_fork::rusty_fork_id;
use tempfile::NamedTempFile;

use super::ExecutionError;
use super::Interruption;
use super::PropertyCause;
use super::PropertyFailure;
use super::PropertyResult;
use super::PropertyRun;
use super::PropertyTransport;
use super::ReplayError;
use super::TestRunner;
use super::typed_replay::ForkChannel;
use super::typed_replay::Journal;
use crate::std_facade::Box;
use crate::std_facade::Vec;
use crate::strategy::Strategy;

/// Path to the parent-owned, versioned typed replay stream.
const FORK_FILE: &str = "_PROPTEST_TYPED_FORKFILE";

/// Observed process termination and the last size seen by a timeout monitor.
pub(super) struct ChildOutcome {
  /// Failure to return normally, without inventing a callback error.
  pub(super) interruption: Option<Interruption>,
  /// Stream size at the timeout decision, used to recognize a late write.
  pub(super) last_size:    Option<u64>,
}

/// Describe an actual child exit without formatting away its code or signal.
fn exited(status: rusty_fork::ExitStatusWrapper) -> Interruption {
  Interruption::ChildExited {
    code:   status.code(),
    signal: status.unix_signal(),
  }
}

/// Wait with the native progress-based timeout policy.
#[allow(
  clippy::single_call_fn,
  reason = "child supervision owns the progress-based timeout policy"
)]
pub(super) fn wait(child: &mut ChildWrapper, file: &File, timeout: u32) -> io::Result<ChildOutcome> {
  #[cfg(feature = "timeout")]
  if timeout != 0 {
    let mut last_size = file.metadata()?.len();
    loop {
      if let Some(status) = child.wait_timeout(Duration::from_millis(u64::from(timeout)))? {
        return Ok(ChildOutcome {
          interruption: (!status.success()).then(|| exited(status)),
          last_size:    None,
        });
      }
      let size = file.metadata()?.len();
      if size <= last_size {
        return Ok(ChildOutcome {
          interruption: Some(Interruption::TimedOut {
            timeout_ms: timeout
          }),
          last_size:    Some(size),
        });
      }
      last_size = size;
    }
  }
  #[cfg(not(feature = "timeout"))]
  let _: (&File, u32) = (file, timeout);
  let status = child.wait()?;
  Ok(ChildOutcome {
    interruption: (!status.success()).then(|| exited(status)),
    last_size:    None,
  })
}

/// Construct an infrastructure failure before any evaluation could be decoded.
fn before_run<V, A, E, T>(runner: &TestRunner, error: ExecutionError<T>) -> PropertyResult<V, A, E, T> {
  Err(Box::new(PropertyFailure {
    context:             "property",
    run:                 PropertyRun {
      statistics: runner.statistics(),
      ..PropertyRun::default()
    },
    cause:               PropertyCause::Engine(error),
    established_failure: None,
    finalization:        Vec::new(),
  }))
}

/// Preserve a primary run outcome when temporary-file finalization fails.
fn finalize<V, A, E, T>(result: PropertyResult<V, A, E, T>, file: NamedTempFile) -> PropertyResult<V, A, E, T> {
  let path = file.path().to_path_buf();
  let Err(error) = file.close() else {
    return result;
  };
  let failure = ExecutionError::Cleanup {
    path,
    error,
  };
  match result {
    Ok(run) => Err(Box::new(PropertyFailure {
      context: run.context,
      run,
      cause: PropertyCause::Engine(failure),
      established_failure: None,
      finalization: Vec::new(),
    })),
    Err(mut report) => {
      report.finalization.push(failure);
      Err(report)
    }
  }
}

/// Execute and restart children, reconstruct the native run, then finalize its file.
#[allow(
  clippy::single_call_fn,
  reason = "fork entry owns temporary-file lifetime around child supervision and reconstruction"
)]
pub(super) fn run<S, F, C, A, E>(runner: &mut TestRunner, strategy: &S, property: F, codec: C) -> PropertyResult<S::Value, A, E, C::Error>
where
  S: Strategy,
  F: Fn(S::Value) -> Result<A, E>,
  C: PropertyTransport<S::Value, A, E>,
{
  run_supervised(runner, strategy, property, codec, |child, file, timeout| {
    wait(child, file, timeout).map_err(ExecutionError::Io)
  })
}

/// Keep child supervision subordinate to runner-owned replay and finalization.
#[allow(
  clippy::single_call_fn,
  reason = "supervision shares the native replay driver between ordinary waits and the controlled child-termination fixture"
)]
pub(super) fn run_supervised<S, F, C, A, E>(
  runner: &mut TestRunner,
  strategy: &S,
  property: F,
  mut codec: C,
  mut supervise: impl FnMut(&mut ChildWrapper, &File, u32) -> Result<ChildOutcome, ExecutionError<C::Error>>,
) -> PropertyResult<S::Value, A, E, C::Error>
where
  S: Strategy,
  F: Fn(S::Value) -> Result<A, E>,
  C: PropertyTransport<S::Value, A, E>,
{
  let Some(configured_name) = runner.config().test_name else {
    return before_run(runner, ExecutionError::TestNameRequired);
  };
  let test_name = fork_test::fix_module_path(configured_name);
  // Re-executed children already have the parent's file. Allocating another
  // NamedTempFile here would leak it when rusty-fork exits the child process.
  let mut transport_file = if env::var_os(FORK_FILE).is_some() {
    None
  } else {
    let mut file = match NamedTempFile::new() {
      Ok(file) => file,
      Err(error) => return before_run(runner, ExecutionError::Io(error)),
    };
    let seed = runner.rng().new_rng_seed();
    if let Err(error) = Journal::create(file.as_file_mut(), &seed) {
      return finalize(before_run(runner, error.into()), file);
    }
    Some(file)
  };
  let result = drive(
    runner, strategy, &property, &mut codec, test_name, &mut transport_file, &mut supervise,
  );
  match transport_file {
    Some(file) => finalize(result, file),
    None => result,
  }
}

/// Parent restart loop; the actual generation and shrink walk stays in `TestRunner`.
#[allow(
  clippy::single_call_fn,
  reason = "child restart supervision remains separate from transport-file finalization"
)]
fn drive<S, F, C, A, E>(
  runner: &mut TestRunner,
  strategy: &S,
  property: &F,
  codec: &mut C,
  test_name: &str,
  transport_file: &mut Option<NamedTempFile>,
  supervise: &mut impl FnMut(&mut ChildWrapper, &File, u32) -> Result<ChildOutcome, ExecutionError<C::Error>>,
) -> PropertyResult<S::Value, A, E, C::Error>
where
  S: Strategy,
  F: Fn(S::Value) -> Result<A, E>,
  C: PropertyTransport<S::Value, A, E>,
{
  let timeout = runner.config().timeout();
  let mut children = 0_u32;
  let (completed_journal, terminal) = loop {
    let launched = rusty_fork::fork(
      test_name,
      rusty_fork_id!(),
      |command| {
        if let Some(file) = transport_file.as_ref() {
          let _configured = command.env(FORK_FILE, file.path());
        }
      },
      |child, _| {
        transport_file.as_ref().map_or_else(
          || Err(ExecutionError::Protocol(ReplayError::Header)),
          |file| supervise(child, file.as_file(), timeout),
        )
      },
      || {
        let _child_outcome = run_child(runner, strategy, property, codec);
      },
    );
    let Some(file) = transport_file.as_mut() else {
      return before_run(runner, ExecutionError::Protocol(ReplayError::Header));
    };
    let journal = match Journal::read(file.as_file_mut()) {
      Ok(journal) => journal,
      Err(error) => return before_run(runner, error.into()),
    };
    if journal.stopped() {
      break (journal, None);
    }
    let child = match launched {
      Ok(Ok(child)) => child,
      Ok(Err(error)) => break (journal, Some(error)),
      Err(error) => break (journal, Some(ExecutionError::Fork(error))),
    };
    let current_size = match file.as_file().metadata() {
      Ok(metadata) => metadata.len(),
      Err(error) => break (journal, Some(ExecutionError::Io(error))),
    };
    let interruption = child.interruption.unwrap_or(Interruption::ChildExited {
      code:   Some(0),
      signal: None,
    });
    if child.last_size.is_none_or(|observed| observed == current_size) {
      match journal.interrupt(file.as_file_mut(), &interruption) {
        Ok(true) => (),
        Ok(false) => break (journal, Some(ExecutionError::ChildBeforeCase(interruption))),
        Err(error) => break (journal, Some(error.into())),
      }
    }
    children = children.saturating_add(1);
    if children >= 10000 {
      let final_journal = match Journal::read(file.as_file_mut()) {
        Ok(decoded) => decoded,
        Err(error) => return before_run(runner, error.into()),
      };
      break (
        final_journal,
        Some(ExecutionError::RestartLimit {
          children,
        }),
      );
    }
  };
  runner.rng().set_seed(completed_journal.seed.clone());
  let channel = ForkChannel::new(codec, completed_journal, None, terminal);
  // The parent channel refuses to admit a new evaluation when replay ends.
  // Keeping the callback here imposes no extra bounds; it is never invoked.
  runner.run_with_channel(strategy, property, channel)
}

/// Execute the re-entered child against the validated parent-owned replay file.
#[allow(
  clippy::single_call_fn,
  reason = "child entry validates its inherited stream before native execution"
)]
fn run_child<S, F, C, A, E>(runner: &mut TestRunner, strategy: &S, property: &F, codec: &mut C) -> PropertyResult<S::Value, A, E, C::Error>
where
  S: Strategy,
  F: Fn(S::Value) -> Result<A, E>,
  C: PropertyTransport<S::Value, A, E>,
{
  let Some(path) = env::var_os(FORK_FILE) else {
    return before_run(runner, ExecutionError::Protocol(ReplayError::Header));
  };
  let mut file = match OpenOptions::new().read(true).append(true).open(path) {
    Ok(file) => file,
    Err(error) => return before_run(runner, ExecutionError::Io(error)),
  };
  let journal = match Journal::read(&mut file) {
    Ok(journal) => journal,
    Err(error) => return before_run(runner, error.into()),
  };
  runner.rng().set_seed(journal.seed.clone());
  runner.run_with_channel(strategy, property, ForkChannel::new(codec, journal, Some(file), None))
}

#[cfg(test)]
mod finalization_tests;

#[cfg(test)]
pub(super) mod tests {
  use std::cell::Cell;
  use std::panic::resume_unwind;
  use std::process::id;
  #[cfg(feature = "timeout")]
  use std::thread::sleep;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::num::u32::BinarySearch;
  use crate::std_facade::vec;
  use crate::strategy::NewTree;
  use crate::test_runner::Config;
  use crate::test_runner::EvaluationOutcome;
  use crate::test_runner::RngSeed;

  /// A non-Clone payload identifies the process which actually evaluated it.
  #[derive(Debug)]
  pub(in crate::test_runner) struct Subject {
    /// Native evaluated subject.
    pub(in crate::test_runner) value:   u32,
    /// Process which produced the evidence.
    pub(in crate::test_runner) process: u32,
  }
  impl Subject {
    /// Capture an actual callback's value and process identity.
    fn new(value: u32) -> Self {
      Self {
        value,
        process: id(),
      }
    }
  }

  /// Distinct native codec failures used by the transport integration tests.
  #[derive(Debug, PartialEq, thiserror::Error)]
  pub(in crate::test_runner) enum CodecFailure {
    /// Original malformed bytes.
    #[error("malformed payload: {0:?}")]
    Malformed(Vec<u8>),
    /// Deliberate value-encoding rejection.
    #[error("encoding rejected {0}")]
    Encode(u32),
    /// Deliberate value-decoding rejection.
    #[error("decoding rejected {0}")]
    Decode(u32),
  }

  /// A small faithful codec with independently injectable encode/decode failures.
  #[derive(Default)]
  pub(in crate::test_runner) struct NumberCodec {
    /// Value rejected by result encoding.
    pub(in crate::test_runner) reject_encode: Option<u32>,
    /// Value rejected by result decoding.
    pub(in crate::test_runner) reject_decode: Option<u32>,
  }
  impl PropertyTransport<u32, Subject, Subject> for NumberCodec {
    type Error = CodecFailure;
    fn encode_case(&mut self, case: &u32) -> Result<Vec<u8>, CodecFailure> {
      Ok(case.to_le_bytes().to_vec())
    }
    fn decode_case(&mut self, bytes: &[u8]) -> Result<u32, CodecFailure> {
      let (&[encoded], &[]) = bytes.as_chunks::<4>() else {
        return Err(CodecFailure::Malformed(bytes.to_vec()));
      };
      Ok(u32::from_le_bytes(encoded))
    }
    fn encode(&mut self, case: &u32, result: &Result<Subject, Subject>) -> Result<Vec<u8>, CodecFailure> {
      if self.reject_encode == Some(*case) {
        return Err(CodecFailure::Encode(*case));
      }
      let (tag, subject) = match *result {
        Ok(ref subject) => (0, subject),
        Err(ref subject) => (1, subject),
      };
      let mut bytes = vec![tag];
      for value in [*case, subject.value, subject.process] {
        bytes.extend_from_slice(&value.to_le_bytes());
      }
      Ok(bytes)
    }
    fn decode(&mut self, bytes: &[u8]) -> Result<(u32, Result<Subject, Subject>), CodecFailure> {
      let Some((&tag, payload)) = bytes.split_first() else {
        return Err(CodecFailure::Malformed(bytes.to_vec()));
      };
      let (numbers, remainder) = payload.as_chunks::<4>();
      if !remainder.is_empty() {
        return Err(CodecFailure::Malformed(bytes.to_vec()));
      }
      let [encoded_case, value, process] = *numbers else {
        return Err(CodecFailure::Malformed(bytes.to_vec()));
      };
      let case = u32::from_le_bytes(encoded_case);
      if self.reject_decode == Some(case) {
        return Err(CodecFailure::Decode(case));
      }
      let subject = Subject {
        value:   u32::from_le_bytes(value),
        process: u32::from_le_bytes(process),
      };
      let result = match tag {
        0 => Ok(subject),
        1 => Err(subject),
        _ => return Err(CodecFailure::Malformed(bytes.to_vec())),
      };
      Ok((case, result))
    }
    fn restore(&mut self, _: &u32, _: &u32) -> Result<(), CodecFailure> {
      Ok(())
    }
    fn restore_interrupted(&mut self, _: &u32, _: &u32) -> Result<(), CodecFailure> {
      Ok(())
    }
    fn encode_error(&mut self, error: &CodecFailure) -> Vec<u8> {
      match *error {
        CodecFailure::Malformed(ref payload) => {
          let mut bytes = vec![0];
          bytes.extend_from_slice(payload);
          bytes
        }
        CodecFailure::Encode(value) | CodecFailure::Decode(value) => {
          let tag = u8::from(matches!(*error, CodecFailure::Decode(_))).saturating_add(1);
          let mut bytes = vec![tag];
          bytes.extend_from_slice(&value.to_le_bytes());
          bytes
        }
      }
    }
    fn decode_error(&mut self, bytes: &[u8]) -> Result<CodecFailure, CodecFailure> {
      let Some((&tag, payload)) = bytes.split_first() else {
        return Err(CodecFailure::Malformed(bytes.to_vec()));
      };
      if tag == 0 {
        return Ok(CodecFailure::Malformed(payload.to_vec()));
      }
      let (&[encoded], &[]) = payload.as_chunks::<4>() else {
        return Err(CodecFailure::Malformed(bytes.to_vec()));
      };
      let value = u32::from_le_bytes(encoded);
      match tag {
        1 => Ok(CodecFailure::Encode(value)),
        2 => Ok(CodecFailure::Decode(value)),
        _ => Err(CodecFailure::Malformed(bytes.to_vec())),
      }
    }
  }

  /// Deterministic numeric case with the ordinary numeric shrink algorithm.
  #[derive(Debug)]
  struct Nine;
  impl Strategy for Nine {
    type Tree = BinarySearch;
    type Value = u32;
    fn new_tree(&self, _: &mut TestRunner) -> NewTree<Self> {
      Ok(Self::Tree::new(9))
    }
  }

  /// The complete transported native report.
  pub(in crate::test_runner) type Report = PropertyResult<u32, Subject, Subject, CodecFailure>;
  /// Full report and callback count observed in the parent.
  type Observation = (Report, u32);
  /// Terminal adaptation retains the complete report in a concrete allocation.
  type Check = Result<(), Box<PredicateFailure<Observation>>>;

  /// Select a reproducible, persistence-free real child execution.
  fn configuration(name: &'static str) -> Config {
    Config {
      cases: 1,
      fork: true,
      test_name: Some(name),
      rng_seed: RngSeed::Fixed(0x5EED),
      failure_persistence: None,
      max_shrink_iters: 64,
      ..Config::default()
    }
  }

  /// Retain the parent callback count alongside the real transported outcome.
  fn observe(config: Config, codec: NumberCodec, property: impl Fn(u32) -> Result<Subject, Subject>) -> Observation {
    let calls = Cell::new(0_u32);
    let result = TestRunner::new(config).run_typed_with_transport(
      &Nine,
      |value| {
        calls.set(calls.get().saturating_add(1));
        property(value)
      },
      codec,
    );
    (result, calls.get())
  }

  /// Fail at the threshold while preserving a process-specific subject in both branches.
  fn threshold(value: u32) -> Result<Subject, Subject> {
    if value >= 5 {
      Err(Subject::new(value))
    } else {
      Ok(Subject::new(value))
    }
  }

  /// Check a codec failure against the complete child prefix and retained pair.
  fn check_codec_failure(
    config: Config,
    codec: NumberCodec,
    expected: &CodecFailure,
  ) -> Result<Observation, Box<PredicateFailure<Observation>>> {
    ensure_that(
      observe(config, codec, threshold),
      "a codec failure preserves passing and failing child evidence, the established pair, and successful finalization",
      |observed| {
        let Err(ref report) = observed.0 else {
          return false;
        };
        let Some(ref established) = report.established_failure else {
          return false;
        };
        observed.1 == 0
          && matches!(report.cause, PropertyCause::Engine(ExecutionError::Transport(ref actual)) if actual == expected)
          && report.finalization.is_empty()
          && established.counterexample > 5
          && report.run.evaluations.iter().any(
            |record| matches!(record.outcome, EvaluationOutcome::Returned(Ok(ref subject)) if subject.value < 5 && subject.process != id()),
          )
          && report.run.evaluations.get(established.evaluation.0).is_some_and(|record| {
            matches!(record.outcome, EvaluationOutcome::Returned(Err(ref subject))
              if subject.value == established.counterexample && subject.process != id())
          })
      },
    )
    .map_err(Box::new)
  }

  /// Check an interrupted child against the complete transported prefix.
  fn check_interruption(
    observation: Observation,
    context: &'static str,
    expected: impl Fn(&Interruption) -> bool,
  ) -> Result<Observation, Box<PredicateFailure<Observation>>> {
    ensure_that(observation, context, |observed| {
      let Err(ref report) = observed.0 else {
        return false;
      };
      observed.1 == 0
        && matches!(report.cause, PropertyCause::Interrupted {
            counterexample: 5,
            ref interruption,
            ..
          } if expected(interruption))
        && report.established_failure.is_none()
        && report.finalization.is_empty()
        && report.run.evaluations.iter().any(
          |record| matches!(record.outcome, EvaluationOutcome::Returned(Ok(ref subject)) if subject.value < 5 && subject.process != id()),
        )
        && report
          .run
          .evaluations
          .iter()
          .all(|record| !matches!(record.outcome, EvaluationOutcome::Returned(Err(_))))
    })
    .map_err(Box::new)
  }

  #[test]
  fn transports_successes_without_parent_invocation() -> Check {
    let config = Config {
      cases: 3,
      ..configuration(concat!(module_path!(), "::transports_successes_without_parent_invocation"))
    };
    ensure_that(
      observe(config, NumberCodec::default(), |value| Ok(Subject::new(value))),
      "every success comes from the child and the parent never calls the property",
      |observed| {
        let Ok(ref run) = observed.0 else {
          return false;
        };
        observed.1 == 0
          && run.statistics.successes == 3
          && run.evaluations.len() == 3
          && run.evaluations.iter().all(|evaluation| {
            matches!(evaluation.outcome, EvaluationOutcome::Returned(Ok(ref subject))
              if subject.value == 9 && subject.process != id())
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn transports_matching_minimized_failure() -> Check {
    ensure_that(
      observe(
        configuration(concat!(module_path!(), "::transports_matching_minimized_failure")),
        NumberCodec::default(),
        threshold,
      ),
      "native counterexample and failure shrink together across transport",
      |observed| {
        let Err(ref report) = observed.0 else {
          return false;
        };
        observed.1 == 0
          && matches!(report.cause,
            PropertyCause::Falsified { counterexample: 5, ref failure, .. }
              if failure.value == 5 && failure.process != id())
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  #[cfg(feature = "timeout")]
  fn preserves_timeout_as_an_engine_interruption() -> Check {
    let config = Config {
      timeout: 200,
      ..configuration(concat!(module_path!(), "::preserves_timeout_as_an_engine_interruption"))
    };
    let result = observe(config, NumberCodec::default(), |value| {
      if value >= 5 {
        sleep(Duration::from_secs(2));
      }
      Ok(Subject::new(value))
    });
    check_interruption(
      result,
      "timeouts retain the minimized case without an invented assertion error",
      |interruption| {
        matches!(*interruption, Interruption::TimedOut {
          timeout_ms: 200
        })
      },
    )
    .map(drop)
  }

  /// Exercise both codec directions with the same retained-prefix contract.
  macro_rules! codec_failure_case {
    ($name:ident, $field:ident, $failure:ident) => {
      #[test]
      fn $name() -> Check {
        let codec = NumberCodec {
          $field: Some(5),
          ..NumberCodec::default()
        };
        check_codec_failure(
          configuration(concat!(module_path!(), "::", stringify!($name))),
          codec,
          &CodecFailure::$failure(5),
        )
        .map(drop)
      }
    };
  }

  codec_failure_case!(retains_prefix_and_established_pair_on_codec_failure, reject_encode, Encode);
  codec_failure_case!(decoding_failure_retains_the_established_native_pair, reject_decode, Decode);

  #[test]
  fn missing_test_name_rejects_fork_before_callback_admission() -> Check {
    let config = Config {
      test_name: None,
      ..configuration("unused")
    };
    ensure_that(
      observe(config, NumberCodec::default(), |value| Ok(Subject::new(value))),
      "fork execution requires an explicit harness name before generating callback evidence",
      |observed| {
        observed.1 == 0
          && observed.0.as_ref().is_err_and(|report| {
            matches!(report.cause, PropertyCause::Engine(ExecutionError::TestNameRequired))
              && report.run.evaluations.is_empty()
              && report.run.events.is_empty()
              && report.established_failure.is_none()
              && report.finalization.is_empty()
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn transports_panic_interruption_without_an_assertion_failure() -> Check {
    let result = observe(
      configuration(concat!(
        module_path!(),
        "::transports_panic_interruption_without_an_assertion_failure"
      )),
      NumberCodec::default(),
      |value| {
        if value >= 5 {
          resume_unwind(Box::new("transported panic"));
        }
        Ok(Subject::new(value))
      },
    );
    check_interruption(
      result,
      "caught child panics retain the minimized case and diagnostic while the parent only replays",
      |interruption| matches!(*interruption, Interruption::Panicked(ref reason) if reason.message() == "transported panic"),
    )
    .map(drop)
  }
}
