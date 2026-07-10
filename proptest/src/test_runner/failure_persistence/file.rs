//-
// Copyright 2017, 2018, 2019 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::any::Any;
use core::fmt::Debug;
use std::borrow::{Cow, ToOwned};
use std::boxed::Box;
use std::env;
use std::format;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::string::{String, ToString};
use std::vec;
use std::vec::Vec;

use self::FileFailurePersistence::*;
use crate::test_runner::diagnostics::{self, RunnerDiagnostic};
use crate::test_runner::failure_persistence::{
    FailurePersistence, PersistedSeed,
};

/// Describes how failing test cases are persisted.
///
/// Note that file names in this enum are `&str` rather than `&Path` since
/// constant functions are not yet in Rust stable as of 2017-12-16.
///
/// In all cases, if a derived path references a directory which does not yet
/// exist, proptest will attempt to create all necessary parent directories.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum FileFailurePersistence {
    /// Completely disables persistence of failing test cases.
    ///
    /// This is semantically equivalent to `Direct("/dev/null")` on Unix and
    /// `Direct("NUL")` on Windows (though it is internally handled by simply
    /// not doing any I/O).
    Off,
    /// The path of the source file under test is traversed up the directory tree
    /// until a directory containing a file named `lib.rs` or `main.rs` is found.
    /// A sibling to that directory with the name given by the string in this
    /// configuration is created, and a file with the same name and path relative
    /// to the source directory, but with the extension changed to `.txt`, is used.
    ///
    /// For example, given a source path of
    /// `/home/jsmith/code/project/src/foo/bar.rs` and a configuration of
    /// `SourceParallel("proptest-regressions")` (the default), assuming the
    /// `src` directory has a `lib.rs` or `main.rs`, the resulting file would
    /// be `/home/jsmith/code/project/proptest-regressions/foo/bar.txt`.
    ///
    /// If no `lib.rs` or `main.rs` can be found, a warning is printed and this
    /// behaves like `WithSource`.
    ///
    /// If no source file has been configured, a warning is printed and this
    /// behaves like `Off`.
    SourceParallel(&'static str),
    /// Failures are persisted in a file with the same path as the source file
    /// under test, but the extension is changed to the string given in this
    /// configuration.
    ///
    /// For example, given a source path of
    /// `/home/jsmith/code/project/src/foo/bar.rs` and a configuration of
    /// `WithSource("regressions")`, the resulting path would be
    /// `/home/jsmith/code/project/src/foo/bar.regressions`.
    WithSource(&'static str),
    /// The string given in this option is directly used as a file path without
    /// any further processing.
    Direct(&'static str),
}

impl Default for FileFailurePersistence {
    fn default() -> Self {
        SourceParallel("proptest-regressions")
    }
}

impl FailurePersistence for FileFailurePersistence {
    fn load_persisted_failures2(
        &self,
        source_file: Option<&'static str>,
    ) -> Vec<PersistedSeed> {
        let source = source_file.and_then(|source_path| {
            absolutize_source_file(Path::new(source_path))
        });
        let resolved = self.resolve(source.as_deref());

        let path: Option<&PathBuf> = resolved.as_ref();
        let result: io::Result<Vec<PersistedSeed>> = path.map_or_else(
            || Ok(vec![]),
            |path| {
                // Reads run unserialized against concurrent appends: every
                // record is appended as one whole line, and a torn or
                // in-flight trailing line is skipped by `parse_seed_line`
                // (with a warning), so the worst case is missing a seed that
                // was persisted mid-load.
                io::BufReader::new(fs::File::open(path)?)
                    .lines()
                    .enumerate()
                    .filter_map(|(lineno, line)| match line {
                        Err(err) => Some(Err(err)),
                        Ok(line) => parse_seed_line(line, path, lineno).map(Ok),
                    })
                    .collect()
            },
        );

        unwrap_or!(result, err => {
            if io::ErrorKind::NotFound != err.kind() {
                diagnostics::emit(RunnerDiagnostic::PersistenceOpenFailed {
                    path: path.cloned(),
                    error: err,
                });
            }
            vec![]
        })
    }

    fn save_persisted_failure2(
        &mut self,
        source_file: Option<&'static str>,
        seed: PersistedSeed,
        shrunken_value: &dyn Debug,
    ) {
        let path = self.resolve(source_file.map(Path::new));
        if let Some(path) = path {
            let line = seed_line(&seed, shrunken_value);

            match write_seed_data_to_file(&path, line.as_bytes()) {
                Err(error) => {
                    diagnostics::emit(
                        RunnerDiagnostic::PersistenceAppendFailed {
                            path,
                            error,
                        },
                    );
                }
                Ok(is_new) => {
                    diagnostics::emit(RunnerDiagnostic::PersistenceSaved {
                        path,
                        created: is_new,
                        seed: seed.to_string(),
                    });
                }
            }
        }
    }

    fn box_clone(&self) -> Box<dyn FailurePersistence> {
        Box::new(*self)
    }

    fn eq(&self, other: &dyn FailurePersistence) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|x| x == self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Ensure that the source file to use for resolving the location of the persisted
/// failing cases file is absolute.
///
/// The source location can only be used if it is absolute. If `source` is
/// not an absolute path, an attempt will be made to determine the absolute
/// path based on the current working directory and its parents. If no
/// absolute path can be determined, a warning will be printed and proptest
/// will continue as if this function had never been called.
///
/// See [`FileFailurePersistence`](enum.FileFailurePersistence.html) for details on
/// how this value is used once it is made absolute.
///
/// This is normally called automatically by the `proptest!` macro, which
/// passes `file!()`.
///
fn absolutize_source_file<'a>(source: &'a Path) -> Option<Cow<'a, Path>> {
    absolutize_source_file_with_cwd(env::current_dir, source)
}

/// `absolutize_source_file` with the cwd lookup injected, so tests can
/// drive the Windows-style upward walk deterministically.
///
/// An absolute `source` is returned borrowed unchanged. A relative one
/// is joined onto the working directory, popping parents until the join
/// names an existing file; emits a diagnostic and returns `None` if the
/// walk is exhausted or the cwd cannot be read.
#[allow(
    clippy::single_call_fn,
    reason = "absolutize a relative source path by walking cwd upward with an injectable getcwd for tests"
)]
fn absolutize_source_file_with_cwd<'a>(
    getcwd: impl FnOnce() -> io::Result<PathBuf>,
    source: &'a Path,
) -> Option<Cow<'a, Path>> {
    if source.is_absolute() {
        // On Unix, `file!()` is absolute. In these cases, we can use
        // that path directly.
        Some(Cow::Borrowed(source))
    } else {
        // On Windows, `file!()` is relative to the crate root, but the
        // test is not generally run with the crate root as the working
        // directory, so the path is not directly usable. However, the
        // working directory is almost always a subdirectory of the crate
        // root, so pop directories off until pushing the source onto the
        // directory results in a path that refers to an existing file.
        // Once we find such a path, we can use that.
        //
        // If we can't figure out an absolute path, print a warning and act
        // as if no source had been given.
        match getcwd() {
            Ok(mut cwd) => loop {
                let joined = cwd.join(source);
                if joined.is_file() {
                    break Some(Cow::Owned(joined));
                }

                if !cwd.pop() {
                    diagnostics::emit(
                        RunnerDiagnostic::SourceNotAbsolutizable {
                            source: source.to_path_buf(),
                        },
                    );
                    break None;
                }
            },

            Err(error) => {
                diagnostics::emit(RunnerDiagnostic::CwdUnresolvable {
                    source: source.to_path_buf(),
                    error,
                });
                None
            }
        }
    }
}

/// Parse one persistence-file line into a `PersistedSeed`, or `None`.
///
/// Everything from the first `#` is a comment and dropped; a blank
/// remainder yields `None`, and a non-blank remainder that fails to
/// parse emits a warning (naming `path` and the 1-based `lineno + 1`)
/// before being skipped.
#[allow(
    clippy::single_call_fn,
    reason = "parse one persistence-file line into a PersistedSeed, warning on unparsable lines"
)]
fn parse_seed_line(
    line: String,
    path: &Path,
    lineno: usize,
) -> Option<PersistedSeed> {
    // Everything from the first '#' on is a comment:
    let seed_text = line
        .split_once('#')
        .map_or(line.as_str(), |(seed_text, _comment)| seed_text);

    if !seed_text.is_empty() {
        let ret = seed_text.parse::<PersistedSeed>().ok();
        if ret.is_none() {
            diagnostics::emit(RunnerDiagnostic::UnparsableSeedLine {
                path: path.to_path_buf(),
                line: lineno + 1,
            });
        }
        return ret;
    }

    None
}

/// Render one persistence record: the seed, a `#` comment carrying the
/// minimized value's `Debug` (newlines flattened to spaces so the record
/// stays a single line), and the trailing newline.
#[allow(
    clippy::single_call_fn,
    reason = "render one persistence record as the seed plus a single-line shrunk-value comment"
)]
fn seed_line(seed: &PersistedSeed, shrunken_value: &dyn Debug) -> String {
    let comment = format!(" # shrinks to {:?}", shrunken_value)
        .replace(['\n', '\r'], " ");
    format!("{}{}\n", seed, comment)
}

/// The explanatory comment block written once at the top of a new
/// persistence file. Every line starts with `#`, so `parse_seed_line`
/// skips it on read.
const FILE_HEADER: &str = "\
# Seeds for failure cases proptest has generated in the past. It is
# automatically read and these particular cases re-run before any
# novel cases are generated.
#
# It is recommended to check this file in to source control so that
# everyone who runs the test benefits from these saved cases.
";

/// Append one whole record to the persistence file, writing the header
/// first when this call creates the file.
///
/// Returns whether the file was newly created.
///
/// Concurrency design (this replaces an in-process lock): the header is
/// claimed via `create_new`, which is atomic at the OS level, so exactly
/// one writer — in this process or any other — writes it. Every append is
/// a single `write_all` of one or two whole lines on an append-mode
/// handle, and the read side skips torn or foreign trailing lines, so
/// concurrent appends need no further serialization.
#[allow(
    clippy::single_call_fn,
    reason = "atomically claim or append the persistence file, writing the header on first creation"
)]
fn write_seed_data_to_file(dst: &Path, seed_data: &[u8]) -> io::Result<bool> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)
    {
        Ok(mut out) => {
            let mut record =
                Vec::with_capacity(FILE_HEADER.len() + seed_data.len());
            record.extend_from_slice(FILE_HEADER.as_bytes());
            record.extend_from_slice(seed_data);
            out.write_all(&record)?;
            Ok(true)
        }
        Err(error) if io::ErrorKind::AlreadyExists == error.kind() => {
            let mut out = fs::OpenOptions::new().append(true).open(dst)?;
            out.write_all(seed_data)?;
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Walk upward from a source file to the directory that contains the crate
/// root (`lib.rs` or `main.rs`) — the anchor `SourceParallel` mirrors its
/// sibling tree against. `None` when no crate root exists above the file.
#[allow(
    clippy::single_call_fn,
    reason = "walk upward from a source file to the crate root the SourceParallel layout mirrors"
)]
fn crate_root_dir_above(source_path: &Path) -> Option<PathBuf> {
    let mut dir = source_path.to_path_buf();
    while dir.pop() {
        if dir.join("lib.rs").is_file() || dir.join("main.rs").is_file() {
            return Some(dir);
        }
    }
    None
}

/// Resolve the `SourceParallel` path strategy: mirror the source file's
/// relative path into the `sibling` directory beside the crate root, with
/// the extension changed to `.txt`; fall back to `WithSource` when no crate
/// root is found above the source file.
#[allow(
    clippy::single_call_fn,
    reason = "compute the SourceParallel persistence path, falling back to WithSource without a crate root"
)]
fn resolve_source_parallel(
    sibling: &'static str,
    source_path: &Cow<'_, Path>,
) -> Option<PathBuf> {
    let Some(dir) = crate_root_dir_above(source_path) else {
        diagnostics::emit(RunnerDiagnostic::SourceParallelRootless);
        return WithSource(sibling).resolve(Some(source_path.as_ref()));
    };
    let suffix = source_path
        .strip_prefix(&dir)
        .expect("parent of source is not a prefix of it?")
        .to_owned();
    let mut result = dir;
    // If we've somehow reached the root, or someone gave us a relative path
    // that we've exhausted, just accept creating a subdirectory instead.
    let _ = result.pop();
    result.push(sibling);
    result.push(&suffix);
    Some(set_extension_best_effort(result, "txt"))
}

/// Change a path extension when the path shape supports it, otherwise keep the
/// path unchanged.
fn set_extension_best_effort(mut path: PathBuf, extension: &str) -> PathBuf {
    let _changed = path.set_extension(extension);
    path
}

impl FileFailurePersistence {
    /// Given the nominal source path, determine the location of the failure
    /// persistence file, if any.
    pub(super) fn resolve(&self, source: Option<&Path>) -> Option<PathBuf> {
        let source = source.and_then(absolutize_source_file);

        match *self {
            Off => None,

            SourceParallel(sibling) => match source {
                Some(source_path) => {
                    resolve_source_parallel(sibling, &source_path)
                }
                None => {
                    diagnostics::emit(
                        RunnerDiagnostic::SourceParallelSourceless,
                    );
                    None
                }
            },

            WithSource(extension) => match source {
                Some(source_path) => Some(set_extension_best_effort(
                    Cow::into_owned(source_path),
                    extension,
                )),

                None => {
                    diagnostics::emit(RunnerDiagnostic::WithSourceSourceless);
                    None
                }
            },

            Direct(path) => Some(Path::new(path).to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strict_test_support::{
        TempDir, TestFailure, ensure, ensure_ok, ensure_some,
    };

    struct TestPaths {
        crate_root: &'static Path,
        src_file: PathBuf,
        subdir_file: PathBuf,
        misplaced_file: PathBuf,
    }

    static TEST_PATHS: std::sync::LazyLock<TestPaths> =
        std::sync::LazyLock::new(|| {
            let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let lib_root = crate_root.join("src");
            let src_subdir = lib_root.join("strategy");
            let src_file = lib_root.join("foo.rs");
            let subdir_file = src_subdir.join("foo.rs");
            let misplaced_file = crate_root.join("foo.rs");
            TestPaths {
                crate_root,
                src_file,
                subdir_file,
                misplaced_file,
            }
        });

    #[test]
    fn persistence_file_location_resolved_correctly() -> Result<(), TestFailure>
    {
        // If off, there is never a file
        ensure(Off.resolve(None).is_none(), "Off resolves no path")?;
        ensure(
            Off.resolve(Some(&TEST_PATHS.subdir_file)).is_none(),
            "Off resolves no path even with a source file",
        )?;

        // For direct, we don't care about the source file, and instead always
        // use whatever is in the config.
        ensure(
            Direct("bar.txt").resolve(None)
                == Some(Path::new("bar.txt").to_owned()),
            "Direct uses the configured path without a source",
        )?;
        ensure(
            Direct("bar.txt").resolve(Some(&TEST_PATHS.subdir_file))
                == Some(Path::new("bar.txt").to_owned()),
            "Direct ignores the source file",
        )?;

        // For WithSource, only the extension changes, but we get nothing if no
        // source file was configured.
        // Accounting for the way absolute paths work on Windows would be more
        // complex, so for now don't test that case.
        #[cfg(unix)]
        fn absolute_path_case() -> Result<(), TestFailure> {
            ensure(
                WithSource("ext").resolve(Some(Path::new("/foo/bar.rs")))
                    == Some(Path::new("/foo/bar.ext").to_owned()),
                "WithSource swaps only the extension",
            )
        }
        #[cfg(not(unix))]
        fn absolute_path_case() -> Result<(), TestFailure> {
            Ok(())
        }
        absolute_path_case()?;
        #[cfg(unix)]
        ensure(
            WithSource("ext").resolve(Some(Path::new("/")))
                == Some(Path::new("/").to_owned()),
            "WithSource leaves a filename-free root path unchanged",
        )?;
        ensure(
            WithSource("ext").resolve(None).is_none(),
            "WithSource resolves no path without a source",
        )?;

        // For SourceParallel, we make a sibling directory tree and change the
        // extensions to .txt ...
        ensure(
            SourceParallel("sib").resolve(Some(&TEST_PATHS.src_file))
                == Some(TEST_PATHS.crate_root.join("sib").join("foo.txt")),
            "SourceParallel mirrors a src file into the sibling tree",
        )?;
        ensure(
            SourceParallel("sib").resolve(Some(&TEST_PATHS.subdir_file))
                == Some(
                    TEST_PATHS
                        .crate_root
                        .join("sib")
                        .join("strategy")
                        .join("foo.txt"),
                ),
            "SourceParallel preserves the source-relative subtree",
        )?;
        // ... but if we can't find lib.rs / main.rs, give up and set the
        // extension instead ...
        ensure(
            SourceParallel("sib").resolve(Some(&TEST_PATHS.misplaced_file))
                == Some(TEST_PATHS.crate_root.join("foo.sib")),
            "SourceParallel falls back to WithSource without a crate root",
        )?;
        // ... and if no source is configured, we do nothing
        ensure(
            SourceParallel("ext").resolve(None).is_none(),
            "SourceParallel resolves no path without a source",
        )
    }

    #[test]
    fn relative_source_files_absolutified() -> Result<(), TestFailure> {
        const TEST_RUNNER_PATH: &[&str] = &["src", "test_runner", "mod.rs"];
        static TEST_RUNNER_RELATIVE: std::sync::LazyLock<PathBuf> =
            std::sync::LazyLock::new(|| TEST_RUNNER_PATH.iter().collect());
        const CARGO_DIR: &str = env!("CARGO_MANIFEST_DIR");

        let expected = ::std::iter::once(CARGO_DIR)
            .chain(TEST_RUNNER_PATH.iter().copied())
            .collect::<PathBuf>();

        // Running from crate root
        let from_root = ensure_some(
            absolutize_source_file_with_cwd(
                || Ok(Path::new(CARGO_DIR).to_owned()),
                TEST_RUNNER_RELATIVE.as_path(),
            ),
            "absolutizing from the crate root succeeds",
        )?;
        ensure(
            expected.as_path() == from_root.as_ref(),
            "the crate-root cwd absolutizes to the manifest path",
        )?;

        // Running from test subdirectory
        let from_subdir = ensure_some(
            absolutize_source_file_with_cwd(
                || Ok(Path::new(CARGO_DIR).join("target")),
                TEST_RUNNER_RELATIVE.as_path(),
            ),
            "absolutizing from a subdirectory succeeds",
        )?;
        ensure(
            expected.as_path() == from_subdir.as_ref(),
            "a subdirectory cwd pops up to the manifest path",
        )
    }

    /// Parse every seed line in the file at `path`, skipping header and
    /// unparsable lines exactly as the load path does.
    fn read_persisted_seeds(
        path: &Path,
    ) -> Result<Vec<PersistedSeed>, TestFailure> {
        let contents = ensure_ok(
            fs::read_to_string(path),
            "the persistence file is readable",
        )?;
        Ok(contents
            .lines()
            .enumerate()
            .filter_map(|(lineno, line)| {
                parse_seed_line(line.to_owned(), path, lineno)
            })
            .collect())
    }

    fn sample_seed(wire: &'static str) -> Result<PersistedSeed, TestFailure> {
        ensure_some(
            wire.parse::<PersistedSeed>().ok(),
            "the sample wire seed parses",
        )
    }

    #[test]
    fn new_file_gets_exactly_one_header_and_appends_stay_headerless()
    -> Result<(), TestFailure> {
        let dir = TempDir::new("persistence-header")?;
        let path = dir.child("regressions.txt");

        let first = sample_seed("xs 1 2 3 4")?;
        let second = sample_seed("xs 5 6 7 8")?;

        let created = ensure_ok(
            write_seed_data_to_file(
                &path,
                seed_line(&first, &"first").as_bytes(),
            ),
            "the first save succeeds",
        )?;
        ensure(created, "the first save reports the file as new")?;

        let appended = ensure_ok(
            write_seed_data_to_file(
                &path,
                seed_line(&second, &"second").as_bytes(),
            ),
            "the second save succeeds",
        )?;
        ensure(!appended, "the second save appends to the existing file")?;

        let contents = ensure_ok(
            fs::read_to_string(&path),
            "the persistence file is readable",
        )?;
        ensure(
            contents.matches("# Seeds for failure cases").count() == 1,
            "the header is written exactly once",
        )?;

        let seeds = read_persisted_seeds(&path)?;
        ensure(
            seeds == vec![first, second],
            "both persisted seeds read back in order",
        )
    }

    #[test]
    fn preexisting_file_is_never_reheadered() -> Result<(), TestFailure> {
        let dir = TempDir::new("persistence-existing")?;
        let path = dir.child("regressions.txt");
        ensure_ok(
            fs::write(&path, ""),
            "pre-creating the persistence file succeeds",
        )?;

        let seed = sample_seed("xs 9 10 11 12")?;
        let created = ensure_ok(
            write_seed_data_to_file(
                &path,
                seed_line(&seed, &"value").as_bytes(),
            ),
            "saving into the pre-existing file succeeds",
        )?;
        ensure(!created, "a pre-existing file is not treated as new")?;

        let contents = ensure_ok(
            fs::read_to_string(&path),
            "the persistence file is readable",
        )?;
        ensure(
            !contents.contains("# Seeds for failure cases"),
            "no header is added to a file this save did not create",
        )?;
        let seeds = read_persisted_seeds(&path)?;
        ensure(seeds == vec![seed], "the appended seed reads back")
    }

    #[test]
    fn torn_or_garbage_lines_are_skipped_on_read() -> Result<(), TestFailure> {
        let dir = TempDir::new("persistence-torn")?;
        let path = dir.child("regressions.txt");

        let seed = sample_seed("xs 13 14 15 16")?;
        let created = ensure_ok(
            write_seed_data_to_file(
                &path,
                seed_line(&seed, &"value").as_bytes(),
            ),
            "the initial save succeeds",
        )?;
        ensure(created, "the initial save creates the file")?;
        // Simulate a torn concurrent append: a trailing half-record with
        // no terminating newline.
        ensure_ok(
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .and_then(|mut f| f.write_all(b"cc deadbe")),
            "appending the torn suffix succeeds",
        )?;

        let seeds = read_persisted_seeds(&path)?;
        ensure(
            seeds == vec![seed.clone()],
            "the valid seed survives and the torn line is skipped",
        )?;

        let flattened = seed_line(&seed, &"multi\nline\rdebug");
        ensure(
            !flattened.trim_end_matches('\n').contains(['\n', '\r']),
            "seed_line flattens newlines so a record stays one line",
        )
    }
}
