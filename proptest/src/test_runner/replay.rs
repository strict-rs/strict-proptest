//-
// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fs;
use std::io;
use std::io::BufRead;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::Path;
use std::string::String;
use std::vec::Vec;

use crate::test_runner::Seed;
use crate::test_runner::TestCaseError;
use crate::test_runner::TestCaseResult;

/// The magic first line every fork replay file must start with; its
/// presence lets `parse_from` reject a swapped-in file it did not write.
const SENTINEL: &str = "proptest-forkfile";

/// A "replay" of a `TestRunner` invocation.
///
/// The replay mechanism is used to support forking. When a child process
/// exits, the parent can read the replay to reproduce the state the child had;
/// similarly, if a child crashes, a new one can be started and given a replay
/// which steps it one complication past the input that caused the crash.
///
/// The replay system is tightly coupled to the `TestRunner` itself. It does
/// not carry enough information to be used in different builds of the same
/// application, or even two different runs of the test process since changes
/// to the persistence file will perturb the replay.
///
/// `Replay` has a special string format for being stored in files. It starts
/// with a line just containing the text in `SENTINEL`, then 16 lines
/// containing the values of `seed`, then an unterminated line consisting of
/// `+`, `-`, and `!` characters to indicate test case passes/failures/rejects,
/// `.` to indicate termination of the test run, or ` ` as a dummy "I'm alive"
/// signal. This format makes it easy for the child process to blindly append
/// to the file without having to worry about the possibility of appends being
/// non-atomic.
#[derive(Clone, Debug)]
pub(super) struct Replay {
  /// The seed of the RNG used to start running the test cases.
  pub(super) seed:  Seed,
  /// A log of whether certain test cases passed or failed. The runner will
  /// assume the same results occur without actually running the test cases.
  pub(super) steps: Vec<TestCaseResult>,
}

/// Result of loading a replay file.
#[derive(Clone, Debug)]
pub(super) enum ReplayFileStatus {
  /// The file is valid and represents a currently-in-progress test.
  InProgress(Replay),
  /// The file is valid, but indicates that all testing has completed.
  Terminated(Replay),
  /// The file is not parsable.
  Corrupt,
}

/// Open the file in the usual read+append+create mode.
#[allow(
  clippy::single_call_fn,
  reason = "open the fork replay file in the append-create-without-truncate mode the log format needs"
)]
pub(super) fn open_file(path: impl AsRef<Path>) -> io::Result<fs::File> {
  fs::OpenOptions::new()
    .read(true)
    .append(true)
    .create(true)
    .truncate(false)
    .open(path)
}

/// Encode one case outcome as its single replay-log character: `+`
/// pass, `-` fail, `!` reject.
const fn step_to_char(step: &TestCaseResult) -> char {
  match *step {
    Ok(()) => '+',
    Err(TestCaseError::Reject(_)) => '!',
    Err(TestCaseError::Fail(_)) => '-',
  }
}

/// Append the given step to the given output.
pub(super) fn append(mut file: impl Write, step: &TestCaseResult) -> io::Result<()> {
  write!(file, "{}", step_to_char(step))
}

/// Read one line required by the replay header, returning `false` on EOF.
fn read_required_line(reader: &mut impl BufRead, line: &mut String) -> io::Result<bool> {
  line.clear();
  let bytes = reader.read_line(line)?;
  Ok(bytes != 0)
}

/// Append a no-op step to the given output.
#[allow(
  clippy::single_call_fn,
  reason = "append a no-op ping character marking that the fork child is still alive"
)]
pub(super) fn ping(mut file: impl Write) -> io::Result<()> {
  write!(file, " ")
}

/// Append a termination mark to the given output.
#[allow(
  clippy::single_call_fn,
  reason = "append the termination marker closing out a fork replay log"
)]
pub(super) fn terminate(mut file: impl Write) -> io::Result<()> {
  write!(file, ".")
}

impl Replay {
  /// Write the full state of this `Replay` to the given output.
  pub(super) fn init_file(&self, mut file: impl Write) -> io::Result<()> {
    writeln!(file, "{SENTINEL}")?;
    let seed = self.seed.to_persistence();
    writeln!(file, "{seed}")?;

    let mut step_data = Vec::<u8>::new();
    for step in &self.steps {
      step_data.push(u8::try_from(u32::from(step_to_char(step))).unwrap_or(b'?'));
    }

    file.write_all(&step_data)?;

    Ok(())
  }

  /// Parse a `Replay` out of the given file.
  ///
  /// The reader is implicitly seeked to the beginning before reading.
  pub(super) fn parse_from(mut file: impl Read + Seek) -> io::Result<ReplayFileStatus> {
    file.rewind()?;

    let mut reader = io::BufReader::new(&mut file);
    let mut line = String::new();

    // Ensure it starts with the sentinel. We do this since we rely on a
    // named temporary file which could be in a location where another
    // actor could replace it with, eg, a symlink to a location they don't
    // control but we do. By rejecting a read from a file missing the
    // sentinel, and not doing any writes if we can't read the file, we
    // won't risk overwriting another file since the prospective attacker
    // would need to be able to change the file to start with the sentinel
    // themselves.
    //
    // There are still some possible symlink attacks that can work by
    // tricking us into reading, but those are non-destructive things like
    // interfering with a FIFO or Unix socket.
    if !read_required_line(&mut reader, &mut line)? {
      return Ok(ReplayFileStatus::Corrupt);
    }
    if SENTINEL != line.trim() {
      return Ok(ReplayFileStatus::Corrupt);
    }

    if !read_required_line(&mut reader, &mut line)? {
      return Ok(ReplayFileStatus::Corrupt);
    }
    let Some(seed) = Seed::from_persistence(&line) else {
      return Ok(ReplayFileStatus::Corrupt);
    };

    line.clear();
    let _step_bytes = reader.read_line(&mut line)?;

    let mut steps = Vec::new();
    for ch in line.chars() {
      match ch {
        '+' => steps.push(Ok(())),
        '-' => steps.push(Err(TestCaseError::fail("failed in other process"))),
        '!' => steps.push(Err(TestCaseError::reject("rejected in other process"))),
        '.' => {
          return Ok(ReplayFileStatus::Terminated(Self {
            seed,
            steps,
          }));
        }
        ' ' => (),
        _ => return Ok(ReplayFileStatus::Corrupt),
      }
    }

    Ok(ReplayFileStatus::InProgress(Self {
      seed,
      steps,
    }))
  }
}

#[cfg(test)]
mod tests {
  use std::boxed::Box;
  /// A concrete allocation retains each complete assertion subject.
  type Check<S> = Result<(), Box<PredicateFailure<S>>>;
  use std::io::Cursor;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;

  /// Incomplete replay bytes and the native parser outcome.
  type PartialReplay = (Vec<u8>, io::Result<ReplayFileStatus>);

  /// Source replay, serialized bytes, write result, and complete native parse.
  type ReplayRoundTrip = (Replay, Vec<u8>, io::Result<()>, io::Result<ReplayFileStatus>);

  /// Initialize a replay and retain every outcome after adding the requested steps.
  fn replay_with_suffix(suffix: &[u8]) -> ReplayRoundTrip {
    let replay = Replay {
      seed:  Seed::XorShift([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]),
      steps: Vec::new(),
    };
    let mut bytes = Vec::new();
    let written = replay.init_file(&mut bytes);
    bytes.extend_from_slice(suffix);
    let parsed = Replay::parse_from(Cursor::new(&bytes));
    (replay, bytes, written, parsed)
  }

  #[test]
  fn valid_empty_replay_is_in_progress() -> Check<ReplayRoundTrip> {
    ensure_that(
      replay_with_suffix(b""),
      "an empty replay preserves its seed and remains in progress",
      |observed| {
        observed.2.is_ok()
          && matches!(observed.3, Ok(ReplayFileStatus::InProgress(ref replay)) if replay.seed == observed.0.seed && replay.steps.is_empty())
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn valid_replay_with_terminator_is_terminated() -> Check<ReplayRoundTrip> {
    ensure_that(
      replay_with_suffix(b"+-!."),
      "termination preserves the seed and ordered pass, fail, reject steps",
      |observed| {
        observed.2.is_ok()
          && matches!(observed.3, Ok(ReplayFileStatus::Terminated(ref replay)) if replay.seed == observed.0.seed
        && matches!(*replay.steps.as_slice(), [Ok(()), Err(TestCaseError::Fail(_)), Err(TestCaseError::Reject(_))]))
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn wrong_sentinel_is_corrupt() -> Check<io::Result<ReplayFileStatus>> {
    ensure_that(
      Replay::parse_from(Cursor::new(b"not-proptest\nxs 1 2 3 4\n")),
      "a replay without the sentinel is corrupt",
      |parsed| matches!(*parsed, Ok(ReplayFileStatus::Corrupt)),
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn missing_seed_is_corrupt() -> Check<PartialReplay> {
    let mut bytes = SENTINEL.as_bytes().to_vec();
    bytes.push(b'\n');
    let parsed = Replay::parse_from(Cursor::new(&bytes));
    ensure_that((bytes, parsed), "a replay without a seed line is corrupt", |observed| {
      matches!(observed.1, Ok(ReplayFileStatus::Corrupt))
    })
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn invalid_step_character_is_corrupt() -> Check<ReplayRoundTrip> {
    ensure_that(
      replay_with_suffix(b"x"),
      "an unknown replay step character is corrupt",
      |observed| observed.2.is_ok() && matches!(observed.3, Ok(ReplayFileStatus::Corrupt)),
    )
    .map(drop)
    .map_err(Box::new)
  }
}
