//-
// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Central emission seam for the runner's stderr diagnostics.
//!
//! Every warning or hint the runner prints during normal operation is a
//! [`RunnerDiagnostic`] variant rendered through one best-effort writer.
//! The typed catalog keeps each message's exact text unit-testable, and
//! the single seam replaces scattered `eprintln!` calls, whose macro
//! panics when stderr is closed — a diagnostic must never abort the run
//! it is trying to describe.

use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;
use std::string::String;

/// One structured runner diagnostic, rendered to the exact text the
/// runner has historically printed for that situation.
pub(crate) enum RunnerDiagnostic {
    /// A `PROPTEST_*` env-var value failed to parse as its target type.
    EnvVarUnparsable {
        /// The `PROPTEST_*` variable name whose value was rejected.
        var: &'static str,
        /// The raw string that could not be parsed.
        value: String,
        /// The human name of the type parsing expected.
        typ: &'static str,
        /// The default value, kept in place of the bad input.
        default: String,
    },
    /// A `PROPTEST_*` env-var value was not valid Unicode.
    EnvVarNotUnicode {
        /// The `PROPTEST_*` variable name with the non-Unicode value.
        var: &'static str,
        /// The default value, kept in place of the bad input.
        default: String,
    },
    /// An unrecognized `PROPTEST_*` env-var was encountered.
    EnvVarUnknown {
        /// The unrecognized `PROPTEST_*` variable name.
        var: String,
    },
    /// A persistence file exists but could not be opened for reading.
    PersistenceOpenFailed {
        /// The persistence file that could not be opened, if known.
        path: Option<PathBuf>,
        /// The underlying I/O error from the failed open.
        error: io::Error,
    },
    /// Appending a new seed record to the persistence file failed.
    PersistenceAppendFailed {
        /// The persistence file the append targeted.
        path: PathBuf,
        /// The underlying I/O error from the failed append.
        error: io::Error,
    },
    /// A failing seed was persisted; tells the user where, and how to
    /// replicate the record on a CI copy of the file.
    PersistenceSaved {
        /// The persistence file the seed was written to.
        path: PathBuf,
        /// Whether this save created the file (vs. appending).
        created: bool,
        /// The replayable seed line to add to a CI copy of the file.
        seed: String,
    },
    /// A relative source path could not be made absolute by walking up
    /// from the current directory.
    SourceNotAbsolutizable {
        /// The relative source path that could not be absolutized.
        source: PathBuf,
    },
    /// The current directory itself could not be determined.
    CwdUnresolvable {
        /// The relative source path that was being resolved.
        source: PathBuf,
        /// The I/O error from querying the current directory.
        error: io::Error,
    },
    /// A persistence-file line did not parse as a seed record. `line`
    /// is 1-based, ready for display.
    UnparsableSeedLine {
        /// The persistence file containing the bad line.
        path: PathBuf,
        /// The 1-based line number of the unparsable record.
        line: usize,
    },
    /// `SourceParallel` persistence found no `lib.rs`/`main.rs` root.
    SourceParallelRootless,
    /// `SourceParallel` persistence was configured without a source.
    SourceParallelSourceless,
    /// `WithSource` persistence was configured without a source.
    WithSourceSourceless,
    /// Closure-style `proptest!` invocations cannot fork or time out.
    ClosureForkUnsupported,
}

impl fmt::Display for RunnerDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::RunnerDiagnostic::*;
        match self {
            EnvVarUnparsable {
                var,
                value: raw_value,
                typ,
                default,
            } => write!(
                f,
                "proptest: The env-var {}={} can't be parsed as {}, \
                 using default of {}.",
                var, raw_value, typ, default
            ),
            EnvVarNotUnicode { var, default } => write!(
                f,
                "proptest: The env-var {} is not valid, using \
                 default of {}.",
                var, default
            ),
            EnvVarUnknown { var } => {
                write!(f, "proptest: Ignoring unknown env-var {}.", var)
            }
            PersistenceOpenFailed { path, error } => write!(
                f,
                "proptest: failed to open {}: {}",
                path.as_deref()
                    .unwrap_or_else(|| std::path::Path::new("??"))
                    .display(),
                error
            ),
            PersistenceAppendFailed { path, error } => write!(
                f,
                "proptest: failed to append to {}: {}",
                path.display(),
                error
            ),
            PersistenceSaved {
                path,
                created,
                seed,
            } => write!(
                f,
                "proptest: Saving this and future failures in {}\n\
                 proptest: If this test was run on a CI system, you may \
                 wish to add the following line to your copy of the \
                 file.{}\n\
                 {}",
                path.display(),
                if *created {
                    " (You may need to create it.)"
                } else {
                    ""
                },
                seed
            ),
            SourceNotAbsolutizable { source } => write!(
                f,
                "proptest: Failed to find absolute path of \
                 source file '{:?}'. Ensure the test is \
                 being run from somewhere within the crate \
                 directory hierarchy.",
                source
            ),
            CwdUnresolvable { source, error } => write!(
                f,
                "proptest: Failed to determine current \
                 directory, so the relative source path \
                 '{:?}' cannot be resolved: {}",
                source, error
            ),
            UnparsableSeedLine { path, line } => write!(
                f,
                "proptest: {}:{}: unparsable line, ignoring",
                path.display(),
                line
            ),
            SourceParallelRootless => write!(
                f,
                "proptest: FileFailurePersistence::SourceParallel set, \
                 but failed to find lib.rs or main.rs"
            ),
            SourceParallelSourceless => write!(
                f,
                "proptest: FileFailurePersistence::SourceParallel set, \
                 but no source file known"
            ),
            WithSourceSourceless => write!(
                f,
                "proptest: FileFailurePersistence::WithSource set, \
                 but no source file known"
            ),
            ClosureForkUnsupported => write!(
                f,
                "proptest: Forking/timeout not supported in \
                 closure-style invocations; ignoring"
            ),
        }
    }
}

/// Whether a message at `level` passes the runner's verbosity gate.
///
/// Routing the comparison through a function (instead of inlining it in
/// the `verbose_message!` macro) keeps `level == ALWAYS` (`0`) from
/// tripping `unused_comparisons` at every expansion site.
pub(crate) fn verbose_at_least(verbose: u32, level: u32) -> bool {
    verbose >= level
}

/// Emit one structured diagnostic to stderr, best-effort.
pub(crate) fn emit(diagnostic: RunnerDiagnostic) {
    write_best_effort(format_args!("{}", diagnostic));
}

/// Emit one pre-formatted verbose-channel message to stderr,
/// best-effort, with the historical `proptest: ` prefix.
pub(crate) fn emit_verbose(args: fmt::Arguments<'_>) {
    write_best_effort(format_args!("proptest: {}", args));
}

/// Write `args` plus a trailing newline to stderr, consciously
/// dropping write errors: with stderr gone there is nowhere left to
/// report to, and a diagnostic must never panic the run it describes
/// (which is exactly what `eprintln!` does on a closed stderr).
fn write_best_effort(args: fmt::Arguments<'_>) {
    drop(write_line(&mut io::stderr().lock(), args));
}

/// The seam's writer core, injectable so tests can capture the exact
/// bytes and prove the error path stays panic-free.
#[allow(
    clippy::single_call_fn,
    reason = "injectable writer core for the diagnostics seam so tests can capture the exact stderr bytes"
)]
fn write_line(
    writer: &mut dyn Write,
    args: fmt::Arguments<'_>,
) -> io::Result<()> {
    writer.write_fmt(args)?;
    writer.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use std::borrow::ToOwned;
    use std::io;
    use std::path::PathBuf;
    use std::string::{String, ToString};
    use std::vec::Vec;

    use strict_test_support::{TestFailure, ensure, ensure_all, ensure_eq};

    use super::{RunnerDiagnostic, verbose_at_least, write_line};

    #[test]
    fn rendered_texts_match_the_legacy_messages() -> Result<(), TestFailure> {
        let unparsable = RunnerDiagnostic::EnvVarUnparsable {
            var: "PROPTEST_CASES",
            value: "many".to_owned(),
            typ: "u32",
            default: "256".to_owned(),
        }
        .to_string();
        ensure_eq(
            &unparsable,
            &"proptest: The env-var PROPTEST_CASES=many can't be parsed \
              as u32, using default of 256."
                .to_owned(),
            "env-var parse warning keeps its legacy text",
        )?;

        let unknown_path = RunnerDiagnostic::PersistenceOpenFailed {
            path: None,
            error: io::Error::from(io::ErrorKind::PermissionDenied),
        }
        .to_string();
        ensure(
            unknown_path.starts_with("proptest: failed to open ??: "),
            "an unknown persistence path renders as the legacy ?? marker",
        )?;

        let line = RunnerDiagnostic::UnparsableSeedLine {
            path: PathBuf::from("proptest-regressions/demo.txt"),
            line: 4,
        }
        .to_string();
        ensure_eq(
            &line,
            &"proptest: proptest-regressions/demo.txt:4: unparsable \
              line, ignoring"
                .to_owned(),
            "seed-line warnings render the display-ready line number",
        )
    }

    #[test]
    fn save_hint_gates_the_creation_suffix_on_created()
    -> Result<(), TestFailure> {
        let render = |created: bool| {
            RunnerDiagnostic::PersistenceSaved {
                path: PathBuf::from("proptest-regressions/demo.txt"),
                created,
                seed: "cc demoseed".to_owned(),
            }
            .to_string()
        };
        let fresh = render(true);
        let existing = render(false);
        ensure_all(&[
            (
                fresh.contains(" (You may need to create it.)"),
                "a newly created file advises creating the CI copy",
            ),
            (
                !existing.contains("(You may need to create it.)"),
                "an existing file omits the creation advice",
            ),
            (
                fresh.ends_with("cc demoseed")
                    && existing.ends_with("cc demoseed"),
                "both renderings end with the replayable seed line",
            ),
            (
                fresh.starts_with(
                    "proptest: Saving this and future failures in \
                     proptest-regressions/demo.txt\n",
                ),
                "the hint names the persistence file on its first line",
            ),
        ])
    }

    #[test]
    fn verbose_gate_admits_at_and_above_the_level() -> Result<(), TestFailure> {
        ensure_all(&[
            (verbose_at_least(0, 0), "ALWAYS-level messages always pass"),
            (verbose_at_least(2, 1), "higher verbosity admits INFO_LOG"),
            (!verbose_at_least(0, 1), "silent runs suppress INFO_LOG"),
            (
                !verbose_at_least(1, 2),
                "INFO_LOG verbosity suppresses TRACE",
            ),
        ])
    }

    #[test]
    fn writer_core_appends_newline_and_survives_write_failure()
    -> Result<(), TestFailure> {
        let mut captured = Vec::new();
        let written = write_line(&mut captured, format_args!("proptest: x"));
        ensure(written.is_ok(), "writing into a buffer succeeds")?;
        ensure_eq(
            &String::from_utf8_lossy(&captured).into_owned(),
            &"proptest: x\n".to_owned(),
            "the seam writes the message plus exactly one newline",
        )?;

        struct BrokenPipe;
        impl io::Write for BrokenPipe {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
        }
        let failed = write_line(&mut BrokenPipe, format_args!("dropped"));
        ensure(
            failed.is_err(),
            "a broken writer reports the error instead of panicking",
        )
    }
}
