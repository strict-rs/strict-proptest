//! A faithful codec for the counting fixture exercises native forked shrinking.

use std::cell::Cell;
use std::iter;
use std::num::TryFromIntError;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use proptest::strict;
use proptest::test_runner::Config;
use proptest::test_runner::EvaluationOutcome;
use proptest::test_runner::PropertyTransport;
use strict_test_support::ConditionFailure;
use strict_test_support::PredicateFailure;
use strict_test_support::ensure_that;
use thiserror::Error;

use super::COUNTER_CONTEXT;
use super::CountingModel;
use super::FailsAtThree;
use super::Tick;
use super::is_minimal_counter_failure;
use crate::ReferenceStateMachine as _;
use crate::SequentialEvidence;
use crate::SequentialFailure;
use crate::SequentialResult;
use crate::SequentialStage;
use crate::StateMachineCase;
use crate::StateMachineEvidence;
use crate::StateMachineFailure;
use crate::StateMachinePropertyResult;
use crate::StateMachineTest as _;
use crate::TransitionEvidence;
use crate::strict_state_machine_config;

/// Native case transmitted before and after an evaluation.
type Case = StateMachineCase<FailsAtThree>;
/// Every returned value and failure has a declared representation.
type Returned = SequentialResult<FailsAtThree>;
/// Report reconstructed by the parent process.
type Report = StateMachinePropertyResult<FailsAtThree, CodecError>;
/// The complete paired execution results and parent callback count.
type Comparison = (StateMachinePropertyResult<FailsAtThree>, Report, usize);

/// Failures of this fixture's declared integer wire schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
enum CodecError {
  /// A record is malformed or contains an unknown tag.
  #[error("malformed counting record")]
  Malformed,
  /// A native length could not be encoded in a wire word.
  #[error("counting length {value} cannot fit in a wire word: {source}")]
  Length { value: usize, source: TryFromIntError },
  /// A wire word could not be decoded as a model state.
  #[error("counting state {value} is out of range: {source}")]
  State { value: u64, source: TryFromIntError },
  /// A wire count could not be decoded in the receiving native type.
  #[error("counting record length {value} is out of range: {source}")]
  Count { value: u64, source: TryFromIntError },
  /// Generated and transported inputs do not describe the same model case.
  #[error("counting replay case differs from the generated case")]
  CaseMismatch,
  /// The concrete failure lies outside this codec's declared context domain.
  #[error("unsupported counting assertion context")]
  Context,
}

/// Explicit codec; serialization is required only at this fork boundary.
struct CountingCodec;

/// Append a length without truncating its native integer.
fn length(words: &mut Vec<u64>, value: usize) -> Result<(), CodecError> {
  words.push(u64::try_from(value).map_err(|source| CodecError::Length {
    value,
    source,
  })?);
  Ok(())
}

/// Read one framed integer from the codec's remaining payload.
fn word(words: &mut &[u64]) -> Result<u64, CodecError> {
  let (first, rest) = words.split_first().ok_or(CodecError::Malformed)?;
  *words = rest;
  Ok(*first)
}

/// Read a concrete counter state with checked narrowing.
fn state(words: &mut &[u64]) -> Result<u32, CodecError> {
  let value = word(words)?;
  u32::try_from(value).map_err(|source| CodecError::State {
    value,
    source,
  })
}

/// Decode a wire length without losing its original value or conversion failure.
fn count(words: &mut &[u64]) -> Result<usize, CodecError> {
  let value = word(words)?;
  usize::try_from(value).map_err(|source| CodecError::Count {
    value,
    source,
  })
}

/// Encode an optional observed state; absence is distinct from zero.
fn put_optional(words: &mut Vec<u64>, value: Option<u32>) {
  match value {
    None => words.push(0),
    Some(present) => {
      words.push(1);
      words.push(u64::from(present));
    }
  }
}

/// Recover an optional state, rejecting unknown option tags.
fn optional(words: &mut &[u64]) -> Result<Option<u32>, CodecError> {
  match word(words)? {
    0 => Ok(None),
    1 => state(words).map(Some),
    _ => Err(CodecError::Malformed),
  }
}

/// Encode each native transition, including its variant tag.
fn put_ticks(words: &mut Vec<u64>, ticks: &[Tick]) -> Result<(), CodecError> {
  length(words, ticks.len())?;
  words.extend(ticks.iter().map(|tick| match *tick {
    Tick::Increment => 0,
  }));
  Ok(())
}

/// Recover transitions only when the complete declared payload exists.
fn ticks(words: &mut &[u64]) -> Result<Vec<Tick>, CodecError> {
  let length = count(words)?;
  let (encoded, rest) = words.split_at_checked(length).ok_or(CodecError::Malformed)?;
  let transitions = encoded
    .iter()
    .map(|tag| match *tag {
      0 => Ok(Tick::Increment),
      _ => Err(CodecError::Malformed),
    })
    .collect();
  *words = rest;
  transitions
}

/// Keep the shared progress value distinct from whether a counter is attached.
fn put_case(words: &mut Vec<u64>, case: &Case) -> Result<(), CodecError> {
  words.push(u64::from(case.0));
  put_ticks(words, &case.1)?;
  match case.2.as_ref() {
    None => words.push(0),
    Some(counter) => {
      words.push(1);
      length(words, counter.load(Ordering::SeqCst))?;
    }
  }
  Ok(())
}

/// Build the remote counter as a native shared value for the returned report.
fn case(words: &mut &[u64]) -> Result<Case, CodecError> {
  let initial = state(words)?;
  let transitions = ticks(words)?;
  let counter = match word(words)? {
    0 => None,
    1 => Some(Arc::new(AtomicUsize::new(count(words)?))),
    _ => return Err(CodecError::Malformed),
  };
  Ok((initial, transitions, counter))
}

/// Encode every successful check, including partial transition evidence.
fn put_evidence(words: &mut Vec<u64>, evidence: &StateMachineEvidence<FailsAtThree>) -> Result<(), CodecError> {
  put_optional(words, evidence.initial_invariant);
  length(words, evidence.transitions.len())?;
  for step in &evidence.transitions {
    words.push(match step.transition {
      Tick::Increment => 0,
    });
    match step.application {
      None => words.push(0),
      Some((before, reference)) => {
        words.extend([1, u64::from(before), u64::from(reference)]);
      }
    }
    put_optional(words, step.invariant);
  }
  Ok(())
}

/// Decode complete ordered evidence without manufacturing a successful check.
fn evidence(words: &mut &[u64]) -> Result<StateMachineEvidence<FailsAtThree>, CodecError> {
  let initial_invariant = optional(words)?;
  let length = count(words)?;
  let transitions = (0..length)
    .map(|_| {
      let transition = match word(words)? {
        0 => Tick::Increment,
        _ => return Err(CodecError::Malformed),
      };
      let application = match word(words)? {
        0 => None,
        1 => Some((state(words)?, state(words)?)),
        _ => return Err(CodecError::Malformed),
      };
      let invariant = optional(words)?;
      Ok(TransitionEvidence {
        transition,
        application,
        invariant,
      })
    })
    .collect::<Result<_, _>>()?;
  Ok(SequentialEvidence {
    initial_invariant,
    transitions,
  })
}

/// Encode a complete result, including the unattempted remainder and owned states.
#[allow(
  clippy::single_call_fn,
  reason = "the return-value codec preserves complete sequential evidence separately from case framing"
)]
fn put_returned(words: &mut Vec<u64>, returned: &Returned) -> Result<(), CodecError> {
  match *returned {
    Ok(ref evidence) => {
      words.push(0);
      put_evidence(words, evidence)?;
    }
    Err(ref failure) => {
      if failure.failure.source.context != COUNTER_CONTEXT {
        return Err(CodecError::Context);
      }
      words.push(1);
      words.push(match failure.stage {
        SequentialStage::InitialInvariant => 0,
        SequentialStage::Application => 1,
        SequentialStage::Invariant => 2,
        SequentialStage::Teardown => 3,
      });
      words.push(u64::from(failure.failure.subject));
      words.push(u64::from(failure.failure.source.condition));
      put_evidence(words, &failure.evidence)?;
      put_optional(words, failure.system);
      put_optional(words, failure.reference);
      put_ticks(words, failure.remaining.as_slice())?;
    }
  }
  Ok(())
}

/// Reconstruct the concrete fixture failure, never a formatted substitute.
#[allow(
  clippy::single_call_fn,
  reason = "the return-value decoder reconstructs complete sequential failures separately from case framing"
)]
fn returned(words: &mut &[u64]) -> Result<Returned, CodecError> {
  match word(words)? {
    0 => evidence(words).map(Ok),
    1 => {
      let stage = match word(words)? {
        0 => SequentialStage::InitialInvariant,
        1 => SequentialStage::Application,
        2 => SequentialStage::Invariant,
        3 => SequentialStage::Teardown,
        _ => return Err(CodecError::Malformed),
      };
      let subject = state(words)?;
      let condition = match word(words)? {
        0 => false,
        1 => true,
        _ => return Err(CodecError::Malformed),
      };
      let failure = PredicateFailure {
        subject,
        source: ConditionFailure {
          condition,
          context: COUNTER_CONTEXT,
        },
      };
      let evidence = evidence(words)?;
      let system = optional(words)?;
      let reference = optional(words)?;
      let remaining = ticks(words)?.into_iter();
      Ok(Err(Box::new(SequentialFailure {
        stage,
        failure,
        evidence,
        system,
        reference,
        remaining,
      })))
    }
    _ => Err(CodecError::Malformed),
  }
}

/// Produce the fixed-width little-endian representation.
fn bytes(words: Vec<u64>) -> Vec<u8> {
  words.into_iter().flat_map(u64::to_le_bytes).collect()
}

/// Decode only whole words, rejecting partial trailing bytes.
fn words(payload: &[u8]) -> Result<Vec<u64>, CodecError> {
  let (chunks, remainder) = payload.as_chunks::<8>();
  if !remainder.is_empty() {
    return Err(CodecError::Malformed);
  }
  Ok(chunks.iter().copied().map(u64::from_le_bytes).collect())
}

/// Reconstruct an encoding failure from the exact native length that caused it.
#[allow(
  clippy::single_call_fn,
  reason = "length-error decoding validates the original conversion independently of record decoding"
)]
fn decode_length_error(payload: &[u8]) -> Result<CodecError, CodecError> {
  let (&[encoded], &[]) = payload.as_chunks::<{ size_of::<usize>() }>() else {
    return Err(CodecError::Malformed);
  };
  let value = usize::from_le_bytes(encoded);
  u64::try_from(value).map_or_else(
    |source| {
      Ok(CodecError::Length {
        value,
        source,
      })
    },
    |_| Err(CodecError::Malformed),
  )
}

/// Reconstruct the native narrowing error from its original model-state word.
#[allow(
  clippy::single_call_fn,
  reason = "state-error decoding replays the exact failed checked conversion"
)]
fn decode_state_error(payload: &[u8]) -> Result<CodecError, CodecError> {
  let (&[encoded], &[]) = payload.as_chunks::<8>() else {
    return Err(CodecError::Malformed);
  };
  let value = u64::from_le_bytes(encoded);
  u32::try_from(value).map_or_else(
    |source| {
      Ok(CodecError::State {
        value,
        source,
      })
    },
    |_| Err(CodecError::Malformed),
  )
}

/// Reconstruct the native receiving-platform length error from its wire word.
#[allow(
  clippy::single_call_fn,
  reason = "count-error decoding preserves the original native-width conversion failure"
)]
fn decode_count_error(payload: &[u8]) -> Result<CodecError, CodecError> {
  let (&[encoded], &[]) = payload.as_chunks::<8>() else {
    return Err(CodecError::Malformed);
  };
  let value = u64::from_le_bytes(encoded);
  usize::try_from(value).map_or_else(
    |source| {
      Ok(CodecError::Count {
        value,
        source,
      })
    },
    |_| Err(CodecError::Malformed),
  )
}

impl PropertyTransport<Case, StateMachineEvidence<FailsAtThree>, Box<StateMachineFailure<FailsAtThree>>> for CountingCodec {
  type Error = CodecError;
  fn encode_case(&mut self, case: &Case) -> Result<Vec<u8>, CodecError> {
    let mut words = Vec::new();
    put_case(&mut words, case)?;
    Ok(bytes(words))
  }
  fn decode_case(&mut self, payload: &[u8]) -> Result<Case, CodecError> {
    let words = words(payload)?;
    let mut remaining = words.as_slice();
    let result = case(&mut remaining)?;
    if !remaining.is_empty() {
      return Err(CodecError::Malformed);
    }
    Ok(result)
  }
  fn encode(&mut self, case: &Case, result: &Returned) -> Result<Vec<u8>, CodecError> {
    let mut words = Vec::new();
    put_case(&mut words, case)?;
    put_returned(&mut words, result)?;
    Ok(bytes(words))
  }
  fn decode(&mut self, payload: &[u8]) -> Result<(Case, Returned), CodecError> {
    let words = words(payload)?;
    let mut remaining = words.as_slice();
    let case = case(&mut remaining)?;
    let result = returned(&mut remaining)?;
    if !remaining.is_empty() {
      return Err(CodecError::Malformed);
    }
    Ok((case, result))
  }
  fn restore(&mut self, generated: &Case, transported: &Case) -> Result<(), CodecError> {
    if generated.0 != transported.0 || generated.1 != transported.1 {
      return Err(CodecError::CaseMismatch);
    }
    match (generated.2.as_ref(), transported.2.as_ref()) {
      (Some(target), Some(source)) => target.store(source.load(Ordering::SeqCst), Ordering::SeqCst),
      (None, None) => (),
      (Some(_), None) | (None, Some(_)) => return Err(CodecError::CaseMismatch),
    }
    Ok(())
  }
  fn restore_interrupted(&mut self, generated: &Case, transported: &Case) -> Result<(), CodecError> {
    self.restore(generated, transported)?;
    if let Some(counter) = generated.2.as_ref() {
      counter.store(generated.1.len(), Ordering::SeqCst);
    }
    Ok(())
  }
  fn encode_error(&mut self, error: &CodecError) -> Vec<u8> {
    match *error {
      CodecError::Malformed => vec![0],
      CodecError::CaseMismatch => vec![2],
      CodecError::Context => vec![3],
      CodecError::Length {
        value, ..
      } => iter::once(1).chain(value.to_le_bytes()).collect(),
      CodecError::State {
        value, ..
      } => iter::once(4).chain(value.to_le_bytes()).collect(),
      CodecError::Count {
        value, ..
      } => iter::once(5).chain(value.to_le_bytes()).collect(),
    }
  }
  fn decode_error(&mut self, payload: &[u8]) -> Result<CodecError, CodecError> {
    match *payload {
      [0] => Ok(CodecError::Malformed),
      [2] => Ok(CodecError::CaseMismatch),
      [3] => Ok(CodecError::Context),
      [1, ref encoded @ ..] => decode_length_error(encoded),
      [4, ref encoded @ ..] => decode_state_error(encoded),
      [5, ref encoded @ ..] => decode_count_error(encoded),
      _ => Err(CodecError::Malformed),
    }
  }
}

/// Replay the same seeded model in-process and in children with one codec.
fn compare(name: &'static str, transition_count: usize) -> Comparison {
  let strategy = CountingModel::sequential_strategy(transition_count);
  let config = Config {
    cases: 4,
    test_name: Some(name),
    ..strict::strict_default_config()
  };
  let local = strict::ensure_property_with_config(&strategy, COUNTER_CONTEXT, config.clone(), |(state, transitions, seen)| {
    FailsAtThree::test_sequential(strict_state_machine_config(), state, transitions, seen)
  });
  let calls = Cell::new(0_usize);
  let transported = strict::ensure_property_with_transport(
    &strategy,
    COUNTER_CONTEXT,
    Config {
      fork: true,
      ..config
    },
    CountingCodec,
    |(state, transitions, seen)| {
      calls.set(calls.get().saturating_add(1));
      FailsAtThree::test_sequential(strict_state_machine_config(), state, transitions, seen)
    },
  );
  (local, transported, calls.get())
}

#[test]
fn transported_shrinking_restores_seen_transitions() -> Result<(), Box<PredicateFailure<Comparison>>> {
  let observation = compare(concat!(module_path!(), "::transported_shrinking_restores_seen_transitions"), 8);
  ensure_that(
    observation,
    "fork replay prunes the unseen tail and reports the same three-transition failure without parent evaluation",
    |observed| {
      observed.2 == 0
        && is_minimal_counter_failure(&observed.0)
        && is_minimal_counter_failure(&observed.1)
        && observed.1.as_ref().is_err_and(|report| {
          report.run.evaluations.first().is_some_and(|evaluation| {
            matches!(evaluation.outcome, EvaluationOutcome::Returned(Err(ref failure)) if failure.remaining.len() == 5
          && failure.system == Some(3) && failure.reference == Some(3) && failure.failure.subject == 3)
          })
        })
    },
  )
  .map(drop)
  .map_err(Box::new)
}

/// A successful two-transition case carries every before/after observation.
#[allow(
  clippy::single_call_fn,
  reason = "complete sequential evidence is checked independently of parent and child run bookkeeping"
)]
fn two_ticks_match(evidence: &StateMachineEvidence<FailsAtThree>) -> bool {
  evidence.initial_invariant == Some(0)
    && evidence.transitions.len() == 2
    && evidence
      .transitions
      .iter()
      .zip([(0, 1), (1, 2)])
      .all(|(step, (before, after))| {
        step.transition == Tick::Increment && step.application == Some((before, after)) && step.invariant == Some(after)
      })
}

#[test]
fn transported_success_keeps_each_transition_observation() -> Result<(), Box<PredicateFailure<Comparison>>> {
  let observation = compare(
    concat!(module_path!(), "::transported_success_keeps_each_transition_observation"),
    2,
  );
  ensure_that(
    observation,
    "successful child cases return all invariant and application evidence without parent evaluation",
    |observed| {
      observed.2 == 0
        && observed.0.as_ref().is_ok_and(|run| run.evaluations.len() == 4)
        && observed.1.as_ref().is_ok_and(|run| {
          run.evaluations.len() == 4
            && run
              .evaluations
              .iter()
              .all(|evaluation| matches!(evaluation.outcome, EvaluationOutcome::Returned(Ok(ref evidence)) if two_ticks_match(evidence)))
        })
    },
  )
  .map(drop)
  .map_err(Box::new)
}

/// Original codec failures alongside their complete decoded representations.
type ErrorRoundTrips = Vec<(CodecError, Result<CodecError, CodecError>)>;

#[test]
fn transport_errors_preserve_native_integer_conversion_failures() -> Result<(), PredicateFailure<ErrorRoundTrips>> {
  let mut errors = vec![CodecError::Malformed, CodecError::CaseMismatch, CodecError::Context];
  let value = u64::from(u32::MAX).saturating_add(1);
  if let Err(source) = u32::try_from(value) {
    errors.push(CodecError::State {
      value,
      source,
    });
  }
  if let Err(source) = usize::try_from(u64::MAX) {
    errors.push(CodecError::Count {
      value: u64::MAX,
      source,
    });
  }
  if let Err(source) = u64::try_from(usize::MAX) {
    errors.push(CodecError::Length {
      value: usize::MAX,
      source,
    });
  }
  let mut codec = CountingCodec;
  let decoded_errors = errors
    .into_iter()
    .map(|original| {
      let encoded = codec.encode_error(&original);
      (original, codec.decode_error(&encoded))
    })
    .collect();
  ensure_that(
    decoded_errors,
    "transported errors retain the original failed integer and native conversion diagnostic",
    |observed: &ErrorRoundTrips| {
      observed.len() >= 4
        && observed
          .iter()
          .all(|&(original, decoded)| decoded.is_ok_and(|error| error == original))
    },
  )
  .map(drop)
}

/// Malformed error frames and their declared decoding outcomes.
type ErrorFrames = Vec<(Vec<u8>, Result<CodecError, CodecError>)>;

#[test]
fn malformed_error_frames_cannot_fabricate_a_failed_conversion() -> Result<(), PredicateFailure<ErrorFrames>> {
  let mut payloads = vec![vec![], vec![6], vec![0, 0], vec![1], vec![4], vec![5]];
  payloads.push(iter::once(1).chain(0_usize.to_le_bytes()).collect());
  for tag in [4, 5] {
    payloads.push(iter::once(tag).chain(0_u64.to_le_bytes()).collect());
    payloads.push(iter::once(tag).chain([0; 9]).collect());
  }
  let mut codec = CountingCodec;
  let decoded_frames = payloads
    .into_iter()
    .map(|payload| {
      let decoded = codec.decode_error(&payload);
      (payload, decoded)
    })
    .collect();
  ensure_that(
    decoded_frames,
    "bad framing and successful conversions are rejected as malformed error records",
    |observed: &ErrorFrames| observed.iter().all(|&(_, result)| result == Err(CodecError::Malformed)),
  )
  .map(drop)
}

/// A replay target, actual counter observations, and mismatched source cases.
type Restoration = (
  Case,
  Case,
  Vec<(Case, Result<(), CodecError>)>,
  [Result<(), CodecError>; 2],
  [usize; 2],
);

#[test]
fn restoration_copies_progress_and_rejects_a_different_case() -> Result<(), Box<PredicateFailure<Restoration>>> {
  let target_counter = Arc::new(AtomicUsize::new(0));
  let target = (0, vec![Tick::Increment; 4], Some(Arc::clone(&target_counter)));
  let source = (0, vec![Tick::Increment; 4], Some(Arc::new(AtomicUsize::new(2))));
  let mut codec = CountingCodec;
  let restored = codec.restore(&target, &source);
  let after_restore = target_counter.load(Ordering::SeqCst);
  let interrupted = codec.restore_interrupted(&target, &source);
  let after_interrupt = target_counter.load(Ordering::SeqCst);
  let mismatches = [
    (1, vec![Tick::Increment; 4], None),
    (0, vec![Tick::Increment; 3], None),
    (0, vec![Tick::Increment; 4], None),
  ]
  .into_iter()
  .map(|case| {
    let result = codec.restore(&target, &case);
    (case, result)
  })
  .collect();
  ensure_that(
    (target, source, mismatches, [restored, interrupted], [
      after_restore, after_interrupt,
    ]),
    "replay restores observed progress, interruption retains the full potentially executed sequence, and mismatches never alter it",
    |observed: &Restoration| {
      observed.3 == [Ok(()), Ok(())]
        && observed.4 == [2, 4]
        && observed.2.iter().all(|&(_, result)| result == Err(CodecError::CaseMismatch))
        && observed.0.2.as_ref().is_some_and(|counter| counter.load(Ordering::SeqCst) == 4)
        && observed.1.2.as_ref().is_some_and(|counter| counter.load(Ordering::SeqCst) == 2)
    },
  )
  .map(drop)
  .map_err(Box::new)
}
