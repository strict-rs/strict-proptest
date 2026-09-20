// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Native protocol checks for completed timeouts and rejected replay records.

#[cfg(feature = "timeout")]
use std::cell::Cell;
#[cfg(feature = "timeout")]
use std::thread;
#[cfg(feature = "timeout")]
use std::time::Duration;

use strict_test_support::PredicateFailure;
use strict_test_support::ensure_that;

use super::*;
use crate::std_facade::vec;
#[cfg(feature = "timeout")]
use crate::strategy::Just;
#[cfg(feature = "timeout")]
use crate::test_runner::Config;
#[cfg(feature = "timeout")]
use crate::test_runner::PropertyCause;
#[cfg(feature = "timeout")]
use crate::test_runner::TestRunner;
use crate::test_runner::typed_fork::tests::CodecFailure;
use crate::test_runner::typed_fork::tests::NumberCodec;
#[cfg(feature = "timeout")]
use crate::test_runner::typed_fork::tests::Report;
use crate::test_runner::typed_fork::tests::Subject;

/// A concrete channel failure, including native codec and I/O errors.
type ChannelError = ExecutionError<CodecFailure>;
/// A replayed evaluation or the original channel failure.
type Replayed = ReplayOutcome<Subject, Subject, CodecFailure>;
/// Terminal assertion adaptation preserving the complete observed subject.
type Check<S> = Result<(), Box<PredicateFailure<S>>>;
/// Original interruption and its complete decoded payload.
type InterruptionRoundTrip = (Interruption, DecodedInterruption);
/// Native interruption decoding with an owned copy of its case bytes.
type DecodedInterruption = Result<(Vec<u8>, Interruption), ReplayError>;
/// Rejected wire bytes, the expected protocol error, and the actual native decode.
type RejectedInterruption = (Vec<u8>, ReplayError, DecodedInterruption);
/// Control-record results and all remaining finalization failures.
type Controls = (Result<bool, ChannelError>, Result<(), ChannelError>, Vec<ChannelError>);
/// Ordered control observations and any failures left for finalization.
type ControlSequence = (Vec<Result<(), ChannelError>>, Vec<ChannelError>);
/// Rejected frames paired with their replay result and unchanged input.
type RejectedEvaluation = (Vec<Frame>, Replayed, u32);

/// Own fixture failures separately from observed protocol failures.
#[derive(Debug, thiserror::Error)]
enum FixtureFailure<S> {
  /// Native stream fixture construction failed.
  #[error(transparent)]
  Io(#[from] io::Error),
  /// Header or payload fixture encoding failed.
  #[error(transparent)]
  Wire(#[from] WireError),
  /// Complete observation rejected by its contract.
  #[error(transparent)]
  Observation(Box<PredicateFailure<S>>),
}

/// Assemble an ordered journal for the same channel used by the native runner.
fn journal(frames: impl IntoIterator<Item = Frame>) -> Journal {
  Journal {
    seed:        Seed::XorShift([1; 16]),
    frames:      frames.into_iter().collect(),
    tail_errors: VecDeque::new(),
  }
}

/// A control record carries the next evaluation identity and no payload.
fn control_frame(kind: u8) -> Frame {
  Frame {
    kind,
    id: EvaluationId(0),
    origin: CaseOrigin::Shrink,
    payload: Vec::new(),
  }
}

#[test]
fn interruption_payloads_preserve_case_and_native_status() -> Result<(), FixtureFailure<Vec<InterruptionRoundTrip>>> {
  let case = [0, 255, 17, 8];
  let mut observations = Vec::new();
  for interruption in [
    Interruption::Panicked("original panic text".into()),
    Interruption::TimedOut {
      timeout_ms: 713
    },
    Interruption::ChildExited {
      code:   Some(23),
      signal: None,
    },
    Interruption::ChildExited {
      code:   None,
      signal: Some(9),
    },
    Interruption::ChildExited {
      code:   None,
      signal: None,
    },
  ] {
    let encoded = interruption_payload(&case, &interruption).map_err(WireError::from)?;
    let decoded = decode_interruption(&encoded).map(|(decoded_case, status)| (decoded_case.to_vec(), status));
    observations.push((interruption, decoded));
  }
  ensure_that(
    observations,
    "interruption transport retains the case, panic text, timeout, and optional native process status",
    |observed| {
      observed.iter().all(|round_trip| {
        let Ok((ref returned_case, ref returned_status)) = round_trip.1 else {
          return false;
        };
        returned_case == &case
          && match round_trip.0 {
            Interruption::Panicked(ref expected) => matches!(*returned_status, Interruption::Panicked(ref actual) if expected == actual),
            Interruption::TimedOut { timeout_ms: expected } => matches!(*returned_status, Interruption::TimedOut { timeout_ms: actual } if expected == actual),
            Interruption::ChildExited { code: expected_code, signal: expected_signal } => matches!(*returned_status, Interruption::ChildExited { code: actual_code, signal: actual_signal } if (expected_code, expected_signal) == (actual_code, actual_signal)),
          }
      })
    },
  )
  .map(drop)
  .map_err(|failure| FixtureFailure::Observation(Box::new(failure)))
}

#[test]
fn malformed_interruption_payloads_keep_distinct_protocol_errors() -> Check<Vec<RejectedInterruption>> {
  let inputs = [
    (vec![], ReplayError::Truncated),
    (vec![255], ReplayError::Identity),
    (vec![1, 0], ReplayError::Truncated),
    (vec![1, 0, 0, 0, 0, 99], ReplayError::Trailing),
    (vec![2, 2, 0], ReplayError::Identity),
    (vec![2, 1, 0], ReplayError::Truncated),
    (vec![2, 0, 0, 99], ReplayError::Trailing),
  ];
  let mut observations = Vec::new();
  for (suffix, expected) in inputs {
    let mut encoded = Vec::from(0_u64.to_le_bytes());
    encoded.extend(suffix);
    let decoded = decode_interruption(&encoded).map(|(case, status)| (case.to_vec(), status));
    observations.push((encoded, expected, decoded));
  }
  ensure_that(
    observations,
    "truncation, invalid tags, and trailing bytes retain their distinct protocol errors",
    |observed| {
      observed
        .iter()
        .all(|rejected| rejected.2.as_ref().is_err_and(|actual| *actual == rejected.1))
    },
  )
  .map(drop)
  .map_err(Box::new)
}

#[test]
fn invalid_panic_text_preserves_the_native_utf8_failure() -> Check<DecodedInterruption> {
  let mut encoded = Vec::from(0_u64.to_le_bytes());
  encoded.extend([0, 255]);
  let decoded = decode_interruption(&encoded).map(|(case, status)| (case.to_vec(), status));
  ensure_that(
    decoded,
    "invalid transported panic text retains the native UTF-8 error",
    |observed| matches!(observed, Err(ReplayError::Encoding(error)) if error.valid_up_to() == 0 && error.error_len() == Some(1)),
  )
  .map(drop)
  .map_err(Box::new)
}

#[test]
fn replay_uses_recorded_budget_and_consumes_backtracking_and_completion() -> Check<Controls> {
  let mut codec = NumberCodec::default();
  let mut channel = ForkChannel::new(
    &mut codec,
    journal([control_frame(BUDGET), control_frame(BACKTRACK), control_frame(COMPLETE)]),
    None,
    None,
  );
  let budget = channel.shrink_budget(false);
  let backtrack = channel.backtrack();
  let completed = channel.finish(true);
  ensure_that(
    (budget, backtrack, completed),
    "replay uses the child's budget decision and consumes traversal-only records without fabricating evaluations",
    |observed| matches!(observed.0, Ok(true)) && observed.1.is_ok() && observed.2.is_empty(),
  )
  .map(drop)
  .map_err(Box::new)
}

#[test]
fn complete_prefix_precedes_tail_and_terminal_failures() -> Check<ControlSequence> {
  let mut input = journal([control_frame(BACKTRACK)]);
  input.tail_errors.extend([
    WireError::Io(Box::new(io::Error::new(io::ErrorKind::BrokenPipe, "the replay stream closed"))),
    WireError::Protocol(Box::new(ReplayError::Truncated)),
  ]);
  let mut codec = NumberCodec::default();
  let mut channel = ForkChannel::new(&mut codec, input, None, Some(ExecutionError::Transport(CodecFailure::Decode(17))));
  let observations = (0..5).map(|_| channel.backtrack()).collect();
  ensure_that(
    (observations, channel.finish(false)),
    "complete replay records precede all tail errors, the terminal failure, and parent exhaustion without losing or repeating a failure",
    |observed: &ControlSequence| {
      observed.1.is_empty()
        && matches!(*observed.0.as_slice(), [
          Ok(()),
          Err(ExecutionError::Io(ref error)),
          Err(ExecutionError::Protocol(ReplayError::Truncated)),
          Err(ExecutionError::Transport(CodecFailure::Decode(17))),
          Err(ExecutionError::Protocol(ReplayError::Exhausted)),
        ] if error.kind() == io::ErrorKind::BrokenPipe)
    },
  )
  .map(drop)
  .map_err(Box::new)
}

#[test]
fn completion_preserves_trailing_records_and_independent_terminal_failures() -> Check<Vec<ChannelError>> {
  let mut input = journal([control_frame(COMPLETE), control_frame(BACKTRACK)]);
  input
    .tail_errors
    .push_back(WireError::Protocol(Box::new(ReplayError::Truncated)));
  let mut codec = NumberCodec::default();
  let mut channel = ForkChannel::new(&mut codec, input, None, Some(ExecutionError::Transport(CodecFailure::Decode(17))));
  ensure_that(
    channel.finish(true),
    "completion retains trailing-frame, truncated-tail, and native codec failures in their original order",
    |observed| {
      matches!(*observed.as_slice(), [
        ExecutionError::Protocol(ReplayError::Trailing),
        ExecutionError::Protocol(ReplayError::Truncated),
        ExecutionError::Transport(CodecFailure::Decode(17)),
      ])
    },
  )
  .map(drop)
  .map_err(Box::new)
}

#[test]
fn replay_rejects_changed_identity_origin_or_record_kind() -> Check<Vec<RejectedEvaluation>> {
  let mut observations = Vec::new();
  for frames in [
    vec![Frame {
      id: EvaluationId(1),
      ..control_frame(START)
    }],
    vec![control_frame(START)],
    vec![Frame {
      origin: CaseOrigin::Generated,
      ..control_frame(RETURN)
    }],
    vec![
      Frame {
        origin: CaseOrigin::Generated,
        ..control_frame(START)
      },
      Frame {
        id: EvaluationId(1),
        origin: CaseOrigin::Generated,
        ..control_frame(RETURN)
      },
    ],
    vec![
      Frame {
        origin: CaseOrigin::Generated,
        ..control_frame(START)
      },
      control_frame(RETURN),
    ],
  ] {
    let recorded = frames
      .iter()
      .map(|frame| Frame {
        kind:    frame.kind,
        id:      frame.id,
        origin:  frame.origin,
        payload: frame.payload.clone(),
      })
      .collect::<Vec<_>>();
    let mut codec = NumberCodec::default();
    let mut channel = ForkChannel::new(&mut codec, journal(frames), None, None);
    let mut case = 9;
    let replayed = channel.replay(EvaluationId(0), &mut case, CaseOrigin::Generated);
    observations.push((recorded, replayed, case));
  }
  ensure_that(
    observations,
    "invalid evaluation identity, origin, and ordering cannot restore a candidate or admit a parent callback",
    |observed| {
      observed
        .iter()
        .all(|rejected| rejected.2 == 9 && matches!(rejected.1, Err(ExecutionError::Protocol(ReplayError::Identity))))
    },
  )
  .map(drop)
  .map_err(Box::new)
}

#[test]
fn incomplete_evaluation_never_admits_a_parent_callback() -> Check<(Replayed, Replayed, u32)> {
  let mut codec = NumberCodec::default();
  let mut channel = ForkChannel::new(
    &mut codec,
    journal([Frame {
      origin: CaseOrigin::Generated,
      ..control_frame(START)
    }]),
    None,
    None,
  );
  let mut case = 9;
  let missing_return = channel.replay(EvaluationId(0), &mut case, CaseOrigin::Generated);
  let exhausted = channel.replay(EvaluationId(0), &mut case, CaseOrigin::Generated);
  ensure_that(
    (missing_return, exhausted, case),
    "an unfinished start and an exhausted parent journal report protocol exhaustion without an evaluation",
    |observed| {
      observed.2 == 9
        && matches!(observed.0, Err(ExecutionError::Protocol(ReplayError::Exhausted)))
        && matches!(observed.1, Err(ExecutionError::Protocol(ReplayError::Exhausted)))
    },
  )
  .map(drop)
  .map_err(Box::new)
}

/// File owner, original child result, parent reconstruction, and parent callback count.
#[cfg(feature = "timeout")]
type TimedReplay = (File, Report, Result<Report, WireError>, u32);

#[test]
#[cfg(feature = "timeout")]
fn completed_timeout_retains_returned_subject_through_recording_and_replay() -> Result<(), FixtureFailure<TimedReplay>> {
  let mut file = tempfile::tempfile()?;
  Journal::create(&mut file, &Seed::XorShift([1; 16]))?;
  let config = Config {
    cases: 1,
    timeout: 1,
    max_shrink_iters: 0,
    failure_persistence: None,
    ..Config::default()
  };
  let mut codec = NumberCodec::default();
  let child_channel = ForkChannel::new(&mut codec, Journal::read(&mut file)?, Some(file.try_clone()?), None);
  let child = TestRunner::new(config.clone()).run_with_channel(
    &Just(9_u32),
    |value| {
      thread::sleep(Duration::from_millis(20));
      Ok(Subject {
        value,
        process: 73,
      })
    },
    child_channel,
  );
  let calls = Cell::new(0_u32);
  let parent = Journal::read(&mut file).map(|recorded| {
    TestRunner::new(config).run_with_channel(
      &Just(9_u32),
      |value| {
        calls.set(calls.get().saturating_add(1));
        Ok(Subject {
          value,
          process: 99,
        })
      },
      ForkChannel::new(&mut codec, recorded, None, None),
    )
  });
  ensure_that(
    (file, child, parent, calls.get()),
    "a completed timeout keeps the returned subject and elapsed time while parent reconstruction never invokes the callback",
    |observed| {
      let Err(ref child_report) = observed.1 else {
        return false;
      };
      let Ok(Err(ref parent_report)) = observed.2 else {
        return false;
      };
      let [ref child_evaluation] = *child_report.run.evaluations.as_slice() else {
        return false;
      };
      let [ref parent_evaluation] = *parent_report.run.evaluations.as_slice() else {
        return false;
      };
      let (
        &EvaluationOutcome::TimedOut {
          result: Ok(ref original),
          timeout_ms: child_timeout,
          elapsed_ms: child_elapsed,
        },
        &EvaluationOutcome::TimedOut {
          result: Ok(ref returned),
          timeout_ms: parent_timeout,
          elapsed_ms: parent_elapsed,
        },
      ) = (&child_evaluation.outcome, &parent_evaluation.outcome)
      else {
        return false;
      };
      observed.3 == 0
        && child_timeout == parent_timeout
        && parent_timeout == 1
        && child_elapsed == parent_elapsed
        && parent_elapsed > 1
        && (original.value, original.process) == (returned.value, returned.process)
        && (returned.value, returned.process) == (9, 73)
        && child_report.finalization.is_empty()
        && parent_report.finalization.is_empty()
        && matches!(parent_report.cause, PropertyCause::Interrupted {
          counterexample: 9,
          interruption: Interruption::TimedOut {
            timeout_ms: 1
          },
          ..
        })
    },
  )
  .map(drop)
  .map_err(|failure| FixtureFailure::Observation(Box::new(failure)))
}
