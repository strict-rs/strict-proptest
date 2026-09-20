// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Versioned frames for typed evaluation replay across native runner children.

use core::str::from_utf8;
use std::fs::File;
use std::io;
use std::io::BufRead as _;
use std::io::Read as _;
use std::io::Write as _;

use thiserror::Error;

use super::CaseOrigin;
use super::EvaluationId;
use super::EvaluationOutcome;
use super::ExecutionError;
use super::Interruption;
use super::PropertyTransport;
use super::ReplayError;
use super::Seed;
use super::transport::EvaluationChannel;
use super::transport::ReplayOutcome;
use crate::std_facade::Box;
use crate::std_facade::String;
use crate::std_facade::Vec;
use crate::std_facade::VecDeque;

/// Versioned header, checked before any appends to a child-provided path.
const HEADER: &str = "proptest-typed-fork-v1";
/// Beginning of an actual callback evaluation.
const START: u8 = 1;
/// Complete returned callback payload.
const RETURN: u8 = 2;
/// Complete interruption evidence.
const INTERRUPTED: u8 = 3;
/// Complication performed without a property invocation.
const BACKTRACK: u8 = 4;
/// Completion of the entire native run.
const COMPLETE: u8 = 5;
/// A concrete caller-codec failure.
const CODEC_ERROR: u8 = 6;
/// A shrink-budget decision, independent of replay wall-clock speed.
const BUDGET: u8 = 7;
/// Returned values from a callback that exceeded its timeout.
const TIMED_RETURN: u8 = 8;

/// One complete frame. Incomplete frames never enter the replay queue.
#[derive(Debug)]
struct Frame {
  /// Protocol record kind.
  kind:    u8,
  /// Evaluation identity (or next identity for traversal-only frames).
  id:      EvaluationId,
  /// Native traversal origin.
  origin:  CaseOrigin,
  /// Caller-encoded or protocol-owned bytes.
  payload: Vec<u8>,
}

/// An I/O or framing failure independent of the caller's codec.
#[derive(Debug, Error)]
pub(super) enum WireError {
  /// Native stream I/O failure.
  #[error(transparent)]
  Io(Box<io::Error>),
  /// Invalid or incomplete framing.
  #[error(transparent)]
  Protocol(Box<ReplayError>),
}

impl From<io::Error> for WireError {
  fn from(error: io::Error) -> Self {
    Self::Io(Box::new(error))
  }
}

impl From<ReplayError> for WireError {
  fn from(error: ReplayError) -> Self {
    Self::Protocol(Box::new(error))
  }
}

impl<T> From<WireError> for ExecutionError<T> {
  fn from(error: WireError) -> Self {
    match error {
      WireError::Io(source) => Self::Io(*source),
      WireError::Protocol(source) => Self::Protocol(*source),
    }
  }
}

/// The complete frame prefix, retaining an error encountered after that prefix.
#[derive(Debug)]
pub(super) struct Journal {
  /// Seed from the versioned header.
  pub(super) seed: Seed,
  /// Complete frames in traversal order.
  frames:          VecDeque<Frame>,
  /// The read/framing failure after the complete prefix, if any.
  tail_errors:     VecDeque<WireError>,
}

impl Journal {
  /// Start a new parent-owned stream.
  #[allow(
    clippy::single_call_fn,
    reason = "stream initialization owns header publication and flushing"
  )]
  pub(super) fn create(file: &mut impl io::Write, seed: &Seed) -> Result<(), WireError> {
    writeln!(file, "{HEADER}\n{}", seed.to_persistence())?;
    file.flush()?;
    Ok(())
  }

  /// Load only complete frames, preserving the prefix on a later failure.
  pub(super) fn read(file: &mut (impl io::Read + io::Seek)) -> Result<Self, WireError> {
    file.rewind()?;
    let mut reader = io::BufReader::new(file);
    let mut header = String::new();
    let _header_bytes = reader.read_line(&mut header)?;
    if header.trim_end() != HEADER {
      return Err(ReplayError::Header.into());
    }
    let mut seed_text = String::new();
    let _seed_bytes = reader.read_line(&mut seed_text)?;
    let seed = Seed::from_persistence(&seed_text).ok_or(ReplayError::Header)?;
    let mut bytes = Vec::new();
    let read_error = reader.read_to_end(&mut bytes).err().map(WireError::from);
    let mut remaining = bytes.as_slice();
    let mut frames = VecDeque::new();
    let mut tail_errors: VecDeque<_> = read_error.into_iter().collect();
    while !remaining.is_empty() {
      match Frame::decode(&mut remaining) {
        Ok(frame) => frames.push_back(frame),
        Err(error) => {
          tail_errors.push_back(error.into());
          break;
        }
      }
    }
    Ok(Self {
      seed,
      frames,
      tail_errors,
    })
  }

  /// Whether the child closed the stream or encountered a protocol/codec error.
  pub(super) fn stopped(&self) -> bool {
    !self.tail_errors.is_empty()
      || self
        .frames
        .back()
        .is_some_and(|frame| frame.kind == COMPLETE || frame.kind == CODEC_ERROR)
  }

  /// Publish an observed child interruption against its unfinished start record.
  ///
  /// A process exit never creates an assertion failure or a successful subject.
  pub(super) fn interrupt(&self, file: &mut impl io::Write, interruption: &Interruption) -> Result<bool, WireError> {
    let Some(start) = self.frames.back().filter(|frame| frame.kind == START) else {
      return Ok(false);
    };
    let payload = interruption_payload(&start.payload, interruption)?;
    Frame {
      kind: INTERRUPTED,
      id: start.id,
      origin: start.origin,
      payload,
    }
    .write(file)?;
    Ok(true)
  }
}

impl Frame {
  /// Decode a frame only after all declared bytes have arrived.
  #[allow(
    clippy::single_call_fn,
    reason = "frame decoding separates protocol validation from journal prefix retention"
  )]
  fn decode(bytes: &mut &[u8]) -> Result<Self, ReplayError> {
    let kind = *take(bytes, 1)?.first().ok_or(ReplayError::Truncated)?;
    if !matches!(
      kind,
      START | RETURN | INTERRUPTED | BACKTRACK | COMPLETE | CODEC_ERROR | BUDGET | TIMED_RETURN
    ) {
      return Err(ReplayError::RecordKind(kind));
    }
    let id = EvaluationId(usize::try_from(number(bytes)?).map_err(ReplayError::Length)?);
    let origin = match *take(bytes, 1)? {
      [0] => CaseOrigin::Generated,
      [1] => CaseOrigin::Persisted,
      [2] => CaseOrigin::Shrink,
      _ => return Err(ReplayError::Identity),
    };
    let length = usize::try_from(number(bytes)?).map_err(ReplayError::Length)?;
    let payload = take(bytes, length)?.to_vec();
    Ok(Self {
      kind,
      id,
      origin,
      payload,
    })
  }

  /// Append a whole frame; the reader still validates completeness after a crash.
  fn write(&self, file: &mut impl io::Write) -> Result<(), WireError> {
    let id = u64::try_from(self.id.0).map_err(ReplayError::Length)?;
    let length = u64::try_from(self.payload.len()).map_err(ReplayError::Length)?;
    let origin = match self.origin {
      CaseOrigin::Generated => 0,
      CaseOrigin::Persisted => 1,
      CaseOrigin::Shrink => 2,
    };
    // A parent never parses concurrently with a live child. Framing, rather
    // than filesystem write atomicity, defines when an evaluation is complete.
    file.write_all(&[self.kind])?;
    file.write_all(&id.to_le_bytes())?;
    file.write_all(&[origin])?;
    file.write_all(&length.to_le_bytes())?;
    file.write_all(&self.payload)?;
    file.flush()?;
    Ok(())
  }
}

/// Consume a checked byte range without indexing or trusting encoded lengths.
fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8], ReplayError> {
  let (prefix, rest) = bytes.split_at_checked(length).ok_or(ReplayError::Truncated)?;
  *bytes = rest;
  Ok(prefix)
}

/// Decode one fixed-width wire integer.
fn number(bytes: &mut &[u8]) -> Result<u64, ReplayError> {
  fixed(bytes).map(u64::from_le_bytes)
}

/// Consume an integer's exact byte width without a fallible slice conversion.
fn fixed<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], ReplayError> {
  let (encoded, remaining) = bytes.split_first_chunk().ok_or(ReplayError::Truncated)?;
  *bytes = remaining;
  Ok(*encoded)
}

/// Encode interruption metadata after the caller's independently encoded case.
fn interruption_payload(case: &[u8], interruption: &Interruption) -> Result<Vec<u8>, ReplayError> {
  let length = u64::try_from(case.len()).map_err(ReplayError::Length)?;
  let mut bytes = Vec::from(length.to_le_bytes());
  bytes.extend_from_slice(case);
  match *interruption {
    Interruption::Panicked(ref reason) => {
      bytes.push(0);
      bytes.extend_from_slice(reason.message().as_bytes());
    }
    Interruption::TimedOut {
      timeout_ms,
    } => {
      bytes.push(1);
      bytes.extend_from_slice(&timeout_ms.to_le_bytes());
    }
    Interruption::ChildExited {
      code,
      signal,
    } => {
      bytes.push(2);
      for status in [code, signal] {
        match status {
          Some(number) => {
            bytes.push(1);
            bytes.extend_from_slice(&number.to_le_bytes());
          }
          None => bytes.push(0),
        }
      }
    }
  }
  Ok(bytes)
}

/// Decode interruption metadata without manufacturing a callback result.
#[allow(
  clippy::single_call_fn,
  reason = "interruption decoding owns its payload shape and trailing-byte validation"
)]
fn decode_interruption(mut payload: &[u8]) -> Result<(&[u8], Interruption), ReplayError> {
  let length = usize::try_from(number(&mut payload)?).map_err(ReplayError::Length)?;
  let case = take(&mut payload, length)?;
  let kind = take(&mut payload, 1)?;
  let interruption = match *kind {
    [0] => {
      let reason = from_utf8(payload).map_err(ReplayError::Encoding)?;
      payload = &[];
      Interruption::Panicked(String::from(reason).into())
    }
    [1] => {
      let bytes = fixed(&mut payload)?;
      Interruption::TimedOut {
        timeout_ms: u32::from_le_bytes(bytes),
      }
    }
    [2] => Interruption::ChildExited {
      code:   optional_status(&mut payload)?,
      signal: optional_status(&mut payload)?,
    },
    _ => return Err(ReplayError::Identity),
  };
  if !payload.is_empty() {
    return Err(ReplayError::Trailing);
  }
  Ok((case, interruption))
}

/// Decode an optional native exit code or signal.
fn optional_status(bytes: &mut &[u8]) -> Result<Option<i32>, ReplayError> {
  match *take(bytes, 1)? {
    [0] => Ok(None),
    [1] => fixed(bytes).map(i32::from_le_bytes).map(Some),
    _ => Err(ReplayError::Identity),
  }
}

/// Replays complete frames and appends newly evaluated records only in a child.
pub(super) struct ForkChannel<'a, C, T> {
  /// Caller-selected typed codec, borrowed through this execution.
  codec:    &'a mut C,
  /// Complete replay prefix and any failure encountered after it.
  journal:  Journal,
  /// An append handle in a child; absent during parent reconstruction.
  output:   Option<File>,
  /// Next evaluation identity, also used for traversal-only records.
  next:     EvaluationId,
  /// Parent-observed infrastructure failure after the decoded prefix.
  terminal: Option<ExecutionError<T>>,
}

impl<'a, C, T> ForkChannel<'a, C, T> {
  /// Assemble a parent or child replay channel from a validated journal.
  pub(super) const fn new(codec: &'a mut C, journal: Journal, output: Option<File>, terminal: Option<ExecutionError<T>>) -> Self {
    Self {
      codec,
      journal,
      output,
      next: EvaluationId(0),
      terminal,
    }
  }

  /// Emit or replay one control record without creating evaluation evidence.
  fn control(&mut self, kind: u8) -> Result<(), ExecutionError<T>> {
    if let Some(frame) = self.journal.frames.pop_front() {
      if frame.kind != kind || frame.id != self.next || !frame.payload.is_empty() {
        return Err(ExecutionError::Protocol(ReplayError::Identity));
      }
      return Ok(());
    }
    if let Some(error) = self.journal.tail_errors.pop_front() {
      return Err(error.into());
    }
    if let Some(error) = self.terminal.take() {
      return Err(error);
    }
    let output = self.output.as_mut().ok_or(ExecutionError::Protocol(ReplayError::Exhausted))?;
    Frame {
      kind,
      id: self.next,
      origin: CaseOrigin::Shrink,
      payload: Vec::new(),
    }
    .write(output)?;
    Ok(())
  }

  /// Publish a codec's concrete error through its infallible error encoder.
  fn codec_failure<V, A, E>(&mut self, id: EvaluationId, origin: CaseOrigin, error: C::Error) -> ExecutionError<C::Error>
  where
    C: PropertyTransport<V, A, E, Error = T>,
  {
    if let Some(output) = self.output.as_mut() {
      let payload = self.codec.encode_error(&error);
      if let Err(wire_error) = (Frame {
        kind: CODEC_ERROR,
        id,
        origin,
        payload,
      })
      .write(output)
      {
        self.journal.tail_errors.push_back(wire_error);
      }
    }
    ExecutionError::Transport(error)
  }
  /// Restore one complete returned evaluation, preserving the codec's native outcome.
  #[allow(
    clippy::single_call_fn,
    reason = "completed evaluation decoding owns payload restoration separately from frame admission"
  )]
  fn decode_returned<V, A, E>(&mut self, case: &mut V, returned: &Frame) -> Result<EvaluationOutcome<A, E>, ExecutionError<T>>
  where
    C: PropertyTransport<V, A, E, Error = T>,
  {
    let outcome = match returned.kind {
      RETURN => {
        let (transported, result) = self.codec.decode(&returned.payload).map_err(ExecutionError::Transport)?;
        self.codec.restore(case, &transported).map_err(ExecutionError::Transport)?;
        *case = transported;
        EvaluationOutcome::Returned(result)
      }
      TIMED_RETURN => {
        let mut payload = returned.payload.as_slice();
        let timeout_ms = u32::try_from(number(&mut payload).map_err(ExecutionError::Protocol)?)
          .map_err(ReplayError::Length)
          .map_err(ExecutionError::Protocol)?;
        let elapsed_ms = number(&mut payload).map_err(ExecutionError::Protocol)?;
        let (transported, result) = self.codec.decode(payload).map_err(ExecutionError::Transport)?;
        self.codec.restore(case, &transported).map_err(ExecutionError::Transport)?;
        *case = transported;
        EvaluationOutcome::TimedOut {
          result,
          timeout_ms,
          elapsed_ms,
        }
      }
      INTERRUPTED => {
        let (payload, interruption) = decode_interruption(&returned.payload).map_err(ExecutionError::Protocol)?;
        let transported = self.codec.decode_case(payload).map_err(ExecutionError::Transport)?;
        self
          .codec
          .restore_interrupted(case, &transported)
          .map_err(ExecutionError::Transport)?;
        *case = transported;
        EvaluationOutcome::Interrupted(interruption)
      }
      CODEC_ERROR => {
        let error = self.codec.decode_error(&returned.payload).unwrap_or_else(|source| source);
        return Err(ExecutionError::Transport(error));
      }
      _ => return Err(ExecutionError::Protocol(ReplayError::Identity)),
    };
    Ok(outcome)
  }
}

impl<V, A, E, C, T> EvaluationChannel<V, A, E> for ForkChannel<'_, C, T>
where
  C: PropertyTransport<V, A, E, Error = T>,
{
  type Error = C::Error;

  fn replay(&mut self, id: EvaluationId, case: &mut V, origin: CaseOrigin) -> ReplayOutcome<A, E, C::Error> {
    self.next = id;
    let Some(start) = self.journal.frames.pop_front() else {
      if let Some(error) = self.journal.tail_errors.pop_front() {
        return Err(error.into());
      }
      if let Some(error) = self.terminal.take() {
        return Err(error);
      }
      if self.output.is_none() {
        return Err(ExecutionError::Protocol(ReplayError::Exhausted));
      }
      let payload = self
        .codec
        .encode_case(case)
        .map_err(|error| self.codec_failure::<V, A, E>(id, origin, error))?;
      if let Some(output) = self.output.as_mut() {
        Frame {
          kind: START,
          id,
          origin,
          payload,
        }
        .write(output)?;
      }
      return Ok(None);
    };
    if start.id != id || start.origin != origin {
      return Err(ExecutionError::Protocol(ReplayError::Identity));
    }
    if start.kind == CODEC_ERROR {
      let error = self.codec.decode_error(&start.payload).unwrap_or_else(|source| source);
      return Err(ExecutionError::Transport(error));
    }
    if start.kind != START {
      return Err(ExecutionError::Protocol(ReplayError::Identity));
    }
    let Some(returned) = self.journal.frames.pop_front() else {
      if let Some(error) = self.journal.tail_errors.pop_front() {
        return Err(error.into());
      }
      if let Some(error) = self.terminal.take() {
        return Err(error);
      }
      if self.output.is_none() {
        return Err(ExecutionError::Protocol(ReplayError::Exhausted));
      }
      // A late start published during timeout termination can be retried by
      // the next child. No completed evaluation is repeated.
      let transported = self.codec.decode_case(&start.payload).map_err(ExecutionError::Transport)?;
      self
        .codec
        .restore_interrupted(case, &transported)
        .map_err(ExecutionError::Transport)?;
      return Ok(None);
    };
    if returned.id != id || returned.origin != origin {
      return Err(ExecutionError::Protocol(ReplayError::Identity));
    }
    let outcome = self.decode_returned(case, &returned)?;
    self.next = EvaluationId(id.0.saturating_add(1));
    Ok(Some(outcome))
  }

  fn record(
    &mut self,
    id: EvaluationId,
    case: &V,
    origin: CaseOrigin,
    outcome: &EvaluationOutcome<A, E>,
  ) -> Result<(), ExecutionError<C::Error>> {
    let (kind, payload) = match *outcome {
      EvaluationOutcome::Returned(ref result) => (
        RETURN,
        self
          .codec
          .encode(case, result)
          .map_err(|error| self.codec_failure::<V, A, E>(id, origin, error))?,
      ),
      EvaluationOutcome::TimedOut {
        ref result,
        timeout_ms,
        elapsed_ms,
      } => {
        let mut payload = Vec::from(u64::from(timeout_ms).to_le_bytes());
        payload.extend_from_slice(&elapsed_ms.to_le_bytes());
        payload.extend(
          self
            .codec
            .encode(case, result)
            .map_err(|error| self.codec_failure::<V, A, E>(id, origin, error))?,
        );
        (TIMED_RETURN, payload)
      }
      EvaluationOutcome::Interrupted(ref interruption) => {
        let encoded_case = self
          .codec
          .encode_case(case)
          .map_err(|error| self.codec_failure::<V, A, E>(id, origin, error))?;
        (
          INTERRUPTED,
          interruption_payload(&encoded_case, interruption).map_err(ExecutionError::Protocol)?,
        )
      }
      EvaluationOutcome::RetainedFailure => return Err(ExecutionError::Protocol(ReplayError::Identity)),
    };
    let output = self.output.as_mut().ok_or(ExecutionError::Protocol(ReplayError::Exhausted))?;
    Frame {
      kind,
      id,
      origin,
      payload,
    }
    .write(output)?;
    self.next = EvaluationId(id.0.saturating_add(1));
    Ok(())
  }

  fn backtrack(&mut self) -> Result<(), ExecutionError<C::Error>> {
    self.control(BACKTRACK)
  }

  fn shrink_budget(&mut self, exhausted: bool) -> Result<bool, ExecutionError<C::Error>> {
    if let Some(frame) = self.journal.frames.front() {
      if frame.kind == BUDGET {
        self.control(BUDGET)?;
        return Ok(true);
      }
      return Ok(false);
    }
    if exhausted {
      self.control(BUDGET)?;
    }
    Ok(exhausted)
  }

  fn finish(&mut self, completed: bool) -> Vec<ExecutionError<C::Error>> {
    let mut failures = Vec::new();
    if completed {
      if let Err(error) = self.control(COMPLETE) {
        failures.push(error);
      }
      if !self.journal.frames.is_empty() {
        failures.push(ExecutionError::Protocol(ReplayError::Trailing));
      }
    }
    failures.extend(self.journal.tail_errors.drain(..).map(ExecutionError::from));
    if let Some(error) = self.terminal.take() {
      failures.push(error);
    }
    if let Some(mut output) = self.output.take()
      && let Err(error) = output.flush()
    {
      failures.push(ExecutionError::Io(error));
    }
    failures
  }

  fn is_in_fork(&self) -> bool {
    self.output.is_some()
  }
}

#[cfg(test)]
mod protocol_tests;

#[cfg(test)]
mod tests {
  use std::cell::Cell;
  use std::process::id;
  use std::thread;
  use std::time::Duration;
  use std::time::Instant;

  use rusty_fork::ChildWrapper;
  use rusty_fork::ExitStatusWrapper;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::vec;
  use crate::strategy::Just;
  use crate::test_runner::Config;
  use crate::test_runner::PropertyCause;
  use crate::test_runner::RngSeed;
  use crate::test_runner::TestRunner;
  use crate::test_runner::typed_fork::ChildOutcome;
  use crate::test_runner::typed_fork::run_supervised;
  use crate::test_runner::typed_fork::tests::CodecFailure;
  use crate::test_runner::typed_fork::tests::NumberCodec;
  use crate::test_runner::typed_fork::tests::Report;
  use crate::test_runner::typed_fork::tests::Subject;

  /// Native wire reads compared across invalid protocol inputs.
  type JournalReads = Vec<Result<Journal, WireError>>;
  /// Encoded frame subjects paired with their decoded results.
  type FrameRoundTrips = Vec<FrameRoundTrip>;
  /// The original frame and native decode result.
  type FrameRoundTrip = (Frame, Result<Frame, ReplayError>);

  /// Preserve native setup errors separately from complete assertion subjects.
  #[derive(Debug, thiserror::Error)]
  enum TestFailure<S> {
    /// Wire fixture construction or observation failed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// The fixture codec failed.
    #[error(transparent)]
    Codec(#[from] CodecFailure),
    /// Full observed subject rejected by the behavior check.
    #[error(transparent)]
    Observation(Box<PredicateFailure<S>>),
  }

  /// Encode a real completed evaluation in the typed protocol.
  fn prefix() -> Result<Vec<u8>, TestFailure<()>> {
    let mut bytes = Vec::new();
    Journal::create(&mut bytes, &Seed::XorShift([1; 16]))?;
    let mut codec = NumberCodec::default();
    Frame {
      kind:    START,
      id:      EvaluationId(0),
      origin:  CaseOrigin::Generated,
      payload: codec.encode_case(&9)?,
    }
    .write(&mut bytes)?;
    Frame {
      kind:    RETURN,
      id:      EvaluationId(0),
      origin:  CaseOrigin::Generated,
      payload: codec.encode(
        &9,
        &Ok(Subject {
          value: 9, process: 73
        }),
      )?,
    }
    .write(&mut bytes)?;
    Ok(bytes)
  }

  /// Include prefix construction errors without converting their payloads.
  #[derive(Debug, thiserror::Error)]
  enum ReplayTestFailure<S> {
    /// Native codec and protocol fixture failures.
    #[error(transparent)]
    Fixture(#[from] TestFailure<()>),
    /// Native read or framing failure before replay begins.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// Complete returned report and parent invocation count.
    #[error(transparent)]
    Observation(Box<PredicateFailure<S>>),
  }

  /// Replay a two-case run without allowing a callback in this process.
  fn replay(journal: Journal) -> (Report, u32) {
    let calls = Cell::new(0_u32);
    let mut codec = NumberCodec::default();
    let mut runner = TestRunner::new(Config {
      cases: 2,
      failure_persistence: None,
      ..Config::default()
    });
    let report = runner.run_with_channel(
      &Just(9_u32),
      |value| {
        calls.set(calls.get().saturating_add(1));
        Ok(Subject {
          value,
          process: id(),
        })
      },
      ForkChannel::new(&mut codec, journal, None, None),
    );
    (report, calls.get())
  }

  #[test]
  fn truncated_record_retains_only_complete_evaluations() -> Result<(), ReplayTestFailure<(Report, u32)>> {
    let mut bytes = prefix()?;
    bytes.extend_from_slice(&[START, 1]);
    let observed = replay(Journal::read(&mut io::Cursor::new(bytes))?);
    ensure_that(
      observed,
      "a truncated frame preserves the decoded prefix without completing or invoking another evaluation",
      |subject| {
        let Err(ref failure) = subject.0 else {
          return false;
        };
        subject.1 == 0
          && failure.run.evaluations.len() == 1
          && failure.finalization.is_empty()
          && matches!(
            failure.cause,
            PropertyCause::Engine(ExecutionError::Protocol(ReplayError::Truncated))
          )
          && failure.run.evaluations.first().is_some_and(
            |record| matches!(record.outcome, EvaluationOutcome::Returned(Ok(ref payload)) if payload.value == 9 && payload.process == 73),
          )
      },
    )
    .map(drop)
    .map_err(|failure| ReplayTestFailure::Observation(Box::new(failure)))
  }

  /// A stream that returns all complete bytes and then fails instead of reporting EOF.
  struct InterruptedRead(io::Cursor<Vec<u8>>);
  impl io::Read for InterruptedRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
      if self.0.position() >= u64::try_from(self.0.get_ref().len()).unwrap_or(u64::MAX) {
        Err(io::Error::other("fixture read failure after the complete prefix"))
      } else {
        self.0.read(buffer)
      }
    }
  }
  impl io::Seek for InterruptedRead {
    fn seek(&mut self, from: io::SeekFrom) -> io::Result<u64> {
      self.0.seek(from)
    }
  }

  #[test]
  fn native_io_failure_preserves_the_decoded_prefix() -> Result<(), ReplayTestFailure<(Report, u32)>> {
    let journal = Journal::read(&mut InterruptedRead(io::Cursor::new(prefix()?)))?;
    ensure_that(
      replay(journal),
      "native I/O failure remains distinct from framing and assertion failures",
      |subject| {
        subject.1 == 0
          && subject.0.as_ref().is_err_and(|failure| {
            failure.run.evaluations.len() == 1
              && matches!(failure.cause, PropertyCause::Engine(ExecutionError::Io(ref error)) if error.kind() == io::ErrorKind::Other)
          })
      },
    )
    .map(drop)
    .map_err(|failure| ReplayTestFailure::Observation(Box::new(failure)))
  }

  #[test]
  fn rejects_unknown_versions_and_record_kinds() -> Result<(), TestFailure<JournalReads>> {
    let invalid_version = Journal::read(&mut io::Cursor::new(b"proptest-typed-fork-v2\n"));
    let mut bytes = Vec::new();
    Journal::create(&mut bytes, &Seed::XorShift([1; 16]))?;
    bytes.push(255);
    let invalid_record = Journal::read(&mut io::Cursor::new(bytes));
    ensure_that(vec![invalid_version, invalid_record], "versions and malformed records have distinct protocol diagnostics", |results| {
      matches!(*results.as_slice(), [Err(WireError::Protocol(ref header_error)), Ok(ref journal)] if *header_error.as_ref() == ReplayError::Header && journal.frames.is_empty()
        && journal.tail_errors.front().is_some_and(|error| matches!(*error, WireError::Protocol(ref source) if *source.as_ref() == ReplayError::RecordKind(255))))
    }).map(drop).map_err(|failure| TestFailure::Observation(Box::new(failure)))
  }

  #[test]
  fn complete_frames_preserve_identity_origin_and_payload() -> Result<(), TestFailure<FrameRoundTrips>> {
    let mut observations = Vec::new();
    for origin in [CaseOrigin::Generated, CaseOrigin::Persisted, CaseOrigin::Shrink] {
      let frame = Frame {
        kind: RETURN,
        id: EvaluationId(713),
        origin,
        payload: vec![0, 255, 17, 0, 38],
      };
      let mut encoded = Vec::new();
      frame.write(&mut encoded)?;
      let decoded = Frame::decode(&mut encoded.as_slice());
      observations.push((frame, decoded));
    }
    let preserves_frame = |round_trip: &FrameRoundTrip| {
      let Ok(ref decoded) = round_trip.1 else {
        return false;
      };
      round_trip.0.kind == decoded.kind
        && round_trip.0.id == decoded.id
        && round_trip.0.origin == decoded.origin
        && round_trip.0.payload == decoded.payload
    };
    ensure_that(
      observations,
      "framing round-trips execution identity, origin, and arbitrary payload bytes",
      |subjects| subjects.iter().all(preserves_frame),
    )
    .map(drop)
    .map_err(|failure| TestFailure::Observation(Box::new(failure)))
  }

  /// Process statuses paired with the candidate whose real child was terminated.
  type TerminatedCases = Vec<(u32, ExitStatusWrapper)>;
  /// The reconstructed report, parent callback count, and native killed-child statuses.
  type RestartObservation = (Report, u32, TerminatedCases);

  /// Kill a live child only after its complete start frame names an interrupted case.
  #[allow(
    clippy::single_call_fn,
    reason = "the parent fixture controls real child termination independently of the callback"
  )]
  fn terminate_failing_case(
    child: &mut ChildWrapper,
    file: &File,
    terminated: &mut TerminatedCases,
  ) -> Result<ChildOutcome, ExecutionError<CodecFailure>> {
    let started = Instant::now();
    loop {
      if let Some(status) = child.try_wait().map_err(ExecutionError::Io)? {
        return Ok(ChildOutcome {
          interruption: (!status.success()).then(|| Interruption::ChildExited {
            code:   status.code(),
            signal: status.unix_signal(),
          }),
          last_size:    None,
        });
      }
      if started.elapsed() > Duration::from_secs(4) {
        return Ok(ChildOutcome {
          interruption: Some(Interruption::TimedOut {
            timeout_ms: 4000
          }),
          last_size:    Some(file.metadata().map_err(ExecutionError::Io)?.len()),
        });
      }
      let journal = Journal::read(&mut file.try_clone().map_err(ExecutionError::Io)?)?;
      let Some(frame) = journal.frames.back().filter(|frame| frame.kind == START) else {
        thread::sleep(Duration::from_millis(1));
        continue;
      };
      let candidate = NumberCodec::default()
        .decode_case(&frame.payload)
        .map_err(ExecutionError::Transport)?;
      if candidate < 5 {
        thread::sleep(Duration::from_millis(1));
        continue;
      }
      child.kill().map_err(ExecutionError::Io)?;
      let status = child.wait().map_err(ExecutionError::Io)?;
      terminated.push((candidate, status));
      return Ok(ChildOutcome {
        interruption: Some(Interruption::ChildExited {
          code:   status.code(),
          signal: status.unix_signal(),
        }),
        last_size:    None,
      });
    }
  }

  /// Remain inside the selected child callback until its parent terminates it.
  #[allow(
    clippy::single_call_fn,
    reason = "a parked child is the real process-termination fixture rather than a returned property failure"
  )]
  fn await_parent_termination() -> ! {
    loop {
      thread::park();
    }
  }

  #[test]
  fn restarts_after_child_exit_without_fabricating_failure() -> Result<(), Box<PredicateFailure<RestartObservation>>> {
    let mut terminated = Vec::new();
    let calls = Cell::new(0_u32);
    let config = Config {
      cases: 256,
      fork: true,
      test_name: Some(concat!(module_path!(), "::restarts_after_child_exit_without_fabricating_failure")),
      failure_persistence: None,
      rng_seed: RngSeed::Fixed(0x5EED),
      max_shrink_iters: 64,
      ..Config::default()
    };
    let report = run_supervised(
      &mut TestRunner::new(config),
      &(0_u32..10),
      |candidate| {
        calls.set(calls.get().saturating_add(1));
        if candidate >= 5 {
          await_parent_termination();
        }
        Ok(Subject {
          value:   candidate,
          process: id(),
        })
      },
      NumberCodec::default(),
      |child, file, _| terminate_failing_case(child, file, &mut terminated),
    );
    ensure_that(
      (report, calls.get(), terminated),
      "real child termination restarts shrinking, preserves its native status, and never fabricates an assertion failure",
      |observed| {
        let Err(ref failure) = observed.0 else {
          return false;
        };
        let PropertyCause::Interrupted {
          counterexample: 5,
          interruption: Interruption::ChildExited {
            code,
            signal,
          },
          ..
        } = failure.cause
        else {
          return false;
        };
        observed.1 == 0
          && observed
            .2
            .iter()
            .any(|termination| termination.0 == 5 && termination.1.code() == code && termination.1.unix_signal() == signal)
          && failure
            .run
            .evaluations
            .iter()
            .all(|evaluation| !matches!(evaluation.outcome, EvaluationOutcome::Returned(Err(_))))
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}
