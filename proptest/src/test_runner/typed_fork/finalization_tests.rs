// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Observe runner-owned file finalization together with complete native run evidence.

use std::fs;
use std::fs::Metadata;
use std::io;
use std::path::PathBuf;
use std::process::id;

use strict_test_support::PredicateFailure;
use strict_test_support::ensure_that;
use tempfile::NamedTempFile;
use tempfile::TempDir;

use super::finalize;
use super::tests::NumberCodec;
use super::tests::Report;
use super::tests::Subject;
use crate::std_facade::Box;
use crate::strategy::Just;
use crate::test_runner::Config;
use crate::test_runner::Evaluation;
use crate::test_runner::EvaluationOutcome;
use crate::test_runner::ExecutionError;
use crate::test_runner::PropertyCause;
use crate::test_runner::TestRunner;

/// Keep the directory and open file together through fixture setup failures.
#[derive(Debug)]
struct TransportFixture {
  /// Own every created or moved fixture artifact.
  directory: TempDir,
  /// The same file owner that runner finalization consumes.
  file:      NamedTempFile,
}

/// Native fixture failures retain allocated owners and the failing operation.
#[derive(Debug, thiserror::Error)]
enum FixtureFailure {
  /// No fixture directory could be allocated.
  #[error("cannot allocate finalization fixture: {0}")]
  Directory(io::Error),
  /// Directory allocation succeeded but file creation failed.
  #[error("cannot create transport file in {directory:?}: {source}")]
  File {
    /// The successfully allocated directory.
    directory: TempDir,
    /// The native file creation failure.
    source:    io::Error,
  },
  /// The real filesystem could not establish the requested obstruction.
  #[error("cannot obstruct transport cleanup for {fixture:?}: {source}")]
  Obstruction {
    /// Both resource owners, including any already-moved artifact.
    fixture: TransportFixture,
    /// The original rename or directory creation failure.
    source:  io::Error,
  },
}

/// A completed finalization attempt with native evidence and remaining filesystem state.
#[derive(Debug)]
struct Observation {
  /// Keeps the obstruction and moved file available until this check is consumed.
  directory:  TempDir,
  /// The exact path whose cleanup the runner attempted.
  path:       PathBuf,
  /// Whether a directory replaced the temporary file before finalization.
  obstructed: bool,
  /// The complete native run, including any cleanup error.
  result:     Report,
  /// Native metadata observation after finalization, including absence errors.
  remaining:  io::Result<Metadata>,
}

/// Both unobstructed and obstructed attempts, including partial fixture setup.
type Observations = [Result<Observation, FixtureFailure>; 2];
/// Terminal assertion failure preserves every observation and its resource owners.
type Check = Result<(), Box<PredicateFailure<Observations>>>;

/// Use an actual directory obstruction to exercise the runner's file-removal boundary.
fn observe_finalization(obstructed: bool, property: impl Fn(u32) -> Result<Subject, Subject>) -> Result<Observation, FixtureFailure> {
  let directory = tempfile::tempdir().map_err(FixtureFailure::Directory)?;
  let file = match NamedTempFile::new_in(directory.path()) {
    Ok(file) => file,
    Err(source) => {
      return Err(FixtureFailure::File {
        directory,
        source,
      });
    }
  };
  let fixture = TransportFixture {
    directory,
    file,
  };
  let path = fixture.file.path().to_path_buf();
  if obstructed {
    let obstruct = fs::rename(&path, fixture.directory.path().join("retained-transport")).and_then(|()| fs::create_dir_all(&path));
    if let Err(source) = obstruct {
      return Err(FixtureFailure::Obstruction {
        fixture,
        source,
      });
    }
  }
  let config = Config {
    cases: 1,
    fork: false,
    #[cfg(feature = "timeout")]
    timeout: 0,
    max_shrink_iters: 0,
    failure_persistence: None,
    ..Config::default()
  };
  let executed = TestRunner::new(config).run_typed_with_transport(&Just(9_u32), property, NumberCodec::default());
  let result = finalize(executed, fixture.file);
  let remaining = fs::symlink_metadata(&path);
  Ok(Observation {
    directory: fixture.directory,
    path,
    obstructed,
    result,
    remaining,
  })
}

#[test]
fn finalization_preserves_successful_evidence_when_cleanup_fails() -> Check {
  let property = |value| {
    Ok(Subject {
      value,
      process: id(),
    })
  };
  let observations = [observe_finalization(false, property), observe_finalization(true, property)];
  let preserves_success = |attempt: &Result<Observation, FixtureFailure>| {
    let Ok(ref observed) = *attempt else {
      return false;
    };
    let run = match observed.result {
      Ok(ref run) if !observed.obstructed => {
        if !observed
          .remaining
          .as_ref()
          .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
        {
          return false;
        }
        run
      }
      Err(ref report) if observed.obstructed => {
        if !matches!(report.cause, PropertyCause::Engine(ExecutionError::Cleanup { ref path, ref error })
            if *path == observed.path && matches!(error.kind(), io::ErrorKind::IsADirectory | io::ErrorKind::PermissionDenied))
          || !observed.remaining.as_ref().is_ok_and(Metadata::is_dir)
          || report.established_failure.is_some()
          || !report.finalization.is_empty()
        {
          return false;
        }
        &report.run
      }
      Ok(_) | Err(_) => return false,
    };
    observed.path.parent() == Some(observed.directory.path())
      && run.statistics.successes == 1
      && matches!(*run.evaluations.as_slice(), [Evaluation {
          outcome: EvaluationOutcome::Returned(Ok(ref subject)), ..
        }] if subject.value == 9 && subject.process == id())
  };
  ensure_that(
    observations,
    "successful cleanup removes the owned file; failed cleanup retains the complete successful run and its native filesystem error",
    |attempts| attempts.iter().all(preserves_success),
  )
  .map(drop)
  .map_err(Box::new)
}

#[test]
fn finalization_preserves_falsification_and_appends_cleanup_failure() -> Check {
  let property = |value| {
    Err(Subject {
      value,
      process: id(),
    })
  };
  let observations = [observe_finalization(false, property), observe_finalization(true, property)];
  ensure_that(
    observations,
    "cleanup never replaces the original native assertion failure and only appends an error when removal actually fails",
    |attempts| {
      attempts.iter().all(|attempt| {
        let Ok(ref observed) = *attempt else {
          return false;
        };
        let Err(ref report) = observed.result else {
          return false;
        };
        let cleanup_matches = if observed.obstructed {
          observed.remaining.as_ref().is_ok_and(Metadata::is_dir)
            && matches!(*report.finalization.as_slice(), [ExecutionError::Cleanup { ref path, ref error }]
            if *path == observed.path && matches!(error.kind(), io::ErrorKind::IsADirectory | io::ErrorKind::PermissionDenied))
        } else {
          report.finalization.is_empty()
            && observed
              .remaining
              .as_ref()
              .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
        };
        cleanup_matches
          && observed.path.parent() == Some(observed.directory.path())
          && report.established_failure.is_none()
          && report.run.statistics.successes == 0
          && matches!(*report.run.evaluations.as_slice(), [Evaluation {
            outcome: EvaluationOutcome::RetainedFailure,
            ..
          }])
          && matches!(report.cause, PropertyCause::Falsified { counterexample: 9, ref failure, evaluation }
          if failure.value == 9 && failure.process == id() && evaluation.0 == 0)
      })
    },
  )
  .map(drop)
  .map_err(Box::new)
}
