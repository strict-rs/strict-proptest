// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The `compiletest_rs` driver for the derive's `compile-fail/` UI cases.
//!
//! Runs each file under `tests/compile-fail/` through raw `rustc` and
//! asserts it fails with the expected diagnostics. Because Cargo does not
//! build these cases, the harness hand-assembles the `--extern`/`-L`/
//! `--edition` flags, selecting the freshly built `proptest` and
//! `proptest_derive` artifacts by matching their Cargo fingerprints in
//! shared or per-build-unit output directories. Self-tests guard native
//! parsing failures, exact feature matching, layout discovery, and the path
//! restrictions imposed by the rustc flag format.

#[cfg(test)]
mod tests {
  extern crate compiletest_rs as ct;

  use std::collections::BTreeSet;
  use std::env;
  use std::fmt;
  use std::fs;
  use std::io;
  use std::path::Path;
  use std::path::PathBuf;
  use std::process::Command;
  use std::process::Output;
  use std::str;
  use std::string::FromUtf8Error;
  use std::time::SystemTime;

  use serde_json::Value;
  use strict_test_support::OptionFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ResultFailure;
  use strict_test_support::TempDir;
  use strict_test_support::TestFailure;
  use strict_test_support::ensure_ok;
  use strict_test_support::ensure_some;
  use strict_test_support::ensure_that;

  use self::ct::common::Mode;

  /// Original decoded fingerprints and their complete parse results.
  type FingerprintObservations = Vec<(Value, Result<CargoFingerprint, FingerprintFailure>)>;
  /// A path conversion that keeps the original path on failure.
  type ArtifactPath = Result<String, PredicateFailure<PathBuf>>;
  /// Cargo's package identity, dependency name, public flag, and fingerprint hash.
  type CargoDependency = (u64, String, bool, u64);

  /// The original malformed input or decoded JSON is retained on every failure.
  #[derive(Debug, thiserror::Error)]
  enum FingerprintFailure {
    /// The outer fingerprint is not JSON.
    #[error("invalid Cargo fingerprint JSON {input:?}: {source}")]
    Json {
      input:  String,
      source: serde_json::Error,
    },
    /// Required fields are absent or have the wrong native JSON type.
    #[error("Cargo fingerprint requires numeric rustc/config hashes, a feature string, and dependencies: {value:?}")]
    Fields { value: Value },
    /// Cargo's nested feature string is not an array of strings.
    #[error("invalid Cargo fingerprint feature list in {value:?}: {source}")]
    Features { value: Value, source: serde_json::Error },
    /// Dependency entries do not have Cargo's native tuple representation.
    #[error("invalid Cargo dependency identities in {value:?}: {source}")]
    Dependencies { value: Value, source: serde_json::Error },
  }

  /// Harness setup failures retain their native path, I/O, and assertion subjects.
  #[derive(Debug, thiserror::Error)]
  enum HarnessFailure {
    /// Resolving the currently running binary failed.
    #[error("cannot locate the compiletest binary: {0}")]
    CurrentExe(#[source] ResultFailure<io::Error>),
    /// The binary is outside Cargo's supported artifact layouts.
    #[error("cannot discover the Cargo artifact layout: {0}")]
    Layout(#[source] PredicateFailure<PathBuf>),
    /// The binary filename lacked its Cargo fingerprint hash.
    #[error("missing Cargo hash in {path:?}: {source}")]
    Hash {
      path:   PathBuf,
      source: OptionFailure<String>,
    },
    /// A required filesystem operation failed.
    #[error("cannot read {path:?}: {source}")]
    Read {
      path:   PathBuf,
      source: ResultFailure<io::Error>,
    },
    /// The active binary's fingerprint is invalid.
    #[error("invalid fingerprint at {path:?}: {source}")]
    Fingerprint {
      path:   PathBuf,
      source: FingerprintFailure,
    },
    /// No matching artifact was found after scanning the dependency directory.
    #[error("no matching artifact in {directory:?}: {source}")]
    Artifact {
      directory: PathBuf,
      source:    OptionFailure<PathBuf>,
    },
    /// The executing binary did not declare the requested direct dependency.
    #[error("missing dependency fingerprint for {library}: {source}")]
    Dependency {
      library: String,
      source:  OptionFailure<u64>,
    },
    /// Compiletest flags cannot represent this original filesystem path.
    #[error(transparent)]
    Path(#[from] PredicateFailure<PathBuf>),
    /// The compiler version command could not be launched.
    #[error("rustc --version could not execute: {0}")]
    VersionCommand(#[source] ResultFailure<io::Error>),
    /// The complete process output is retained by the enclosing progress record.
    #[error("rustc --version output is not UTF-8: {0}")]
    VersionUtf8(#[source] str::Utf8Error),
    /// The owned temporary suite could not be allocated.
    #[error(transparent)]
    Fixture(#[from] TestFailure),
    /// A materialized fixture could not be written.
    #[error("cannot materialize {path:?}: {source}")]
    Write { path: PathBuf, source: io::Error },
    /// Rust fixture source is not UTF-8; the conversion retains its bytes.
    #[error("invalid Rust fixture text at {path:?}: {source}")]
    SourceUtf8 { path: PathBuf, source: FromUtf8Error },
    /// A revisioned fixture does not declare the active compiler revision.
    #[error("cannot select the compiler revision in {path:?}: {source}")]
    Revision {
      path:   PathBuf,
      source: PredicateFailure<String>,
    },
  }

  /// Keep the complete native compiletest configuration after its suite returns.
  struct CompletedSuite(ct::Config);

  impl fmt::Debug for CompletedSuite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      formatter
        .debug_struct("CompletedSuite")
        .field("source", &self.0.src_base)
        .field("mode", &self.0.mode)
        .field("flags", &self.0.target_rustcflags)
        .finish_non_exhaustive()
    }
  }

  /// Completed suites and process output survive failures in later stages.
  #[derive(Debug)]
  struct CompileProgress {
    suites:   Vec<CompletedSuite>,
    rustc:    String,
    version:  Option<Output>,
    fixtures: Option<TempDir>,
  }

  /// Setup failure together with every completed part of the compiler workflow.
  #[derive(Debug, thiserror::Error)]
  #[error("compiletest setup failed after {progress:?}: {source}")]
  struct CompileFailure {
    progress: CompileProgress,
    source:   HarnessFailure,
  }

  /// Cargo can share dependency outputs or isolate every build unit.
  #[derive(Debug, PartialEq, Eq)]
  enum CargoLayout {
    /// Libraries share a profile's `deps` directory.
    Shared { profile: PathBuf },
    /// Each package and hash owns its `out` and `fingerprint` directories.
    Isolated { build: PathBuf },
  }

  struct CargoArtifacts {
    layout:              CargoLayout,
    dependency_dirs:     Vec<PathBuf>,
    current_fingerprint: CargoFingerprint,
  }

  #[derive(Debug, PartialEq, Eq)]
  struct CargoFingerprint {
    rustc:        u64,
    config:       u64,
    features:     BTreeSet<String>,
    dependencies: Vec<CargoDependency>,
  }

  impl CargoFingerprint {
    fn parse(json_text: &str) -> Result<Self, FingerprintFailure> {
      let value: Value = serde_json::from_str(json_text).map_err(|source| FingerprintFailure::Json {
        input: json_text.to_owned(),
        source,
      })?;
      let (Some(rustc), Some(config), Some(feature_text), Some(dependencies)) = (
        value.get("rustc").and_then(Value::as_u64),
        value.get("config").and_then(Value::as_u64),
        value.get("features").and_then(Value::as_str),
        value.get("deps"),
      ) else {
        return Err(FingerprintFailure::Fields {
          value,
        });
      };
      let features = serde_json::from_str::<Vec<String>>(feature_text).map_err(|source| FingerprintFailure::Features {
        value: value.clone(),
        source,
      })?;
      let parsed_dependencies = serde_json::from_value(dependencies.clone()).map_err(|source| FingerprintFailure::Dependencies {
        value,
        source,
      })?;
      Ok(Self {
        rustc,
        config,
        features: features.into_iter().collect(),
        dependencies: parsed_dependencies,
      })
    }

    fn matches_current_build(&self, current: &Self, required_features: &[&str]) -> bool {
      self.rustc == current.rustc
        && self.config == current.config
        && required_features.iter().all(|feature| self.features.contains(*feature))
    }

    /// Select the build fingerprint recorded for this binary's direct dependency.
    fn dependency_hash(&self, library: &str) -> Option<u64> {
      self
        .dependencies
        .iter()
        .find(|dependency| dependency.1 == library)
        .map(|dependency| dependency.3)
    }
  }

  impl CargoLayout {
    /// Derive the search root from the running binary, including custom profiles.
    fn from_executable(executable: &Path) -> Result<Self, PredicateFailure<PathBuf>> {
      let layout = executable
        .parent()
        .filter(|output| output.file_name().is_some_and(|name| name == "deps"))
        .and_then(Path::parent)
        .map(|profile| Self::Shared {
          profile: profile.to_path_buf(),
        })
        .or_else(|| {
          let output = executable.parent()?;
          let unit = output.parent()?;
          let package = unit.parent()?;
          let build = package.parent()?;
          (output.file_name()? == "out"
            && build.file_name()? == "build"
            && package.file_name()? == env!("CARGO_PKG_NAME")
            && unit.file_name()?.to_str()? == artifact_hash(executable, "compiletest-", env::consts::EXE_EXTENSION)?)
          .then(|| Self::Isolated {
            build: build.to_path_buf(),
          })
        });
      layout.ok_or_else(|| PredicateFailure {
        subject: executable.to_path_buf(),
        source:  strict_test_support::ConditionFailure {
          condition: false,
          context:   "the test executable belongs to a Cargo shared or isolated output directory",
        },
      })
    }

    /// Locate a target's fingerprint without conflating package names and hashes.
    fn fingerprint_path(&self, package: &str, hash: &str, target: &str) -> PathBuf {
      let directory = match *self {
        Self::Shared {
          ref profile,
        } => profile.join(".fingerprint").join(format!("{package}-{hash}")),
        Self::Isolated {
          ref build,
        } => build.join(package).join(hash).join("fingerprint"),
      };
      directory.join(format!("{target}.json"))
    }

    /// Preserve all dependency search directories needed by raw rustc invocations.
    fn dependency_dirs(&self) -> Result<Vec<PathBuf>, HarnessFailure> {
      let build = match *self {
        Self::Shared {
          ref profile,
        } => return Ok(vec![profile.join("deps")]),
        Self::Isolated {
          ref build,
        } => build,
      };
      let mut directories = Vec::new();
      for package in directory_entries(build)?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
      {
        directories.extend(
          directory_entries(&package)?
            .into_iter()
            .map(|unit| unit.path().join("out"))
            .filter(|output| output.is_dir()),
        );
      }
      directories.sort();
      Ok(directories)
    }
  }

  /// Retain the directory and native I/O failure when traversal is incomplete.
  fn directory_entries(directory: &Path) -> Result<Vec<fs::DirEntry>, HarnessFailure> {
    let read_failure = |source| HarnessFailure::Read {
      path: directory.to_path_buf(),
      source,
    };
    let entries = ensure_ok(fs::read_dir(directory), "the directory is readable").map_err(read_failure)?;
    entries
      .map(|entry| ensure_ok(entry, "the directory entry is readable").map_err(read_failure))
      .collect()
  }

  impl CargoArtifacts {
    #[allow(
      clippy::single_call_fn,
      reason = "locates the compiletest binary's own Cargo fingerprint to match sibling artifacts"
    )]
    fn current() -> Result<Self, HarnessFailure> {
      let current_exe = ensure_ok(env::current_exe(), "the current test binary path resolves").map_err(HarnessFailure::CurrentExe)?;
      let layout = CargoLayout::from_executable(&current_exe).map_err(HarnessFailure::Layout)?;
      let exe_hash = ensure_some(
        artifact_hash(&current_exe, "compiletest-", env::consts::EXE_EXTENSION).map(str::to_owned),
        "the compiletest binary name carries a Cargo hash",
      )
      .map_err(|source| HarnessFailure::Hash {
        path: current_exe,
        source,
      })?;
      let test_fingerprint_path = layout.fingerprint_path(env!("CARGO_PKG_NAME"), &exe_hash, "test-integration-test-compiletest");
      let test_fingerprint = ensure_ok(
        fs::read_to_string(&test_fingerprint_path),
        "the compiletest fingerprint file is readable",
      )
      .map_err(|source| HarnessFailure::Read {
        path: test_fingerprint_path.clone(),
        source,
      })?;

      let current_fingerprint = CargoFingerprint::parse(&test_fingerprint).map_err(|source| HarnessFailure::Fingerprint {
        path: test_fingerprint_path,
        source,
      })?;
      Ok(Self {
        dependency_dirs: layout.dependency_dirs()?,
        layout,
        current_fingerprint,
      })
    }

    fn extern_arg(
      &self,
      package: &str,
      lib_name: &str,
      file_prefix: &str,
      extension: &str,
      required_features: &[&str],
      missing_context: &'static str,
    ) -> Result<String, HarnessFailure> {
      let path = self.resolve_artifact(package, lib_name, file_prefix, extension, required_features, missing_context)?;
      // Cargo may keep full metadata only in the sibling rmeta, leaving the
      // rlib with a stub. Both paths identify the same selected build unit.
      let metadata = (extension == "rlib").then(|| path.with_extension("rmeta"));
      let library = format!("--extern {}={}", lib_name, path_to_string(path)?);
      match metadata {
        Some(metadata_path) => Ok(format!("{} --extern {}={}", library, lib_name, path_to_string(metadata_path)?)),
        None => Ok(library),
      }
    }

    fn resolve_artifact(
      &self,
      package: &str,
      lib_name: &str,
      file_prefix: &str,
      extension: &str,
      required_features: &[&str],
      missing_context: &'static str,
    ) -> Result<PathBuf, HarnessFailure> {
      let dependency_hash = ensure_some(
        self.current_fingerprint.dependency_hash(lib_name),
        "the current binary names this dependency",
      )
      .map_err(|source| HarnessFailure::Dependency {
        library: lib_name.to_owned(),
        source,
      })?;
      // Cargo stores the u64 dependency fingerprint as little-endian hex bytes.
      let dependency_fingerprint = format!("{:016x}", dependency_hash.swap_bytes());
      let entries = self
        .dependency_dirs
        .iter()
        .map(|directory| directory_entries(directory))
        .collect::<Result<Vec<_>, _>>()?;
      let mut candidates = entries
        .into_iter()
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|artifact| {
          let hash = artifact_hash(&artifact, file_prefix, extension)?;
          let fingerprint_path = self.layout.fingerprint_path(package, hash, &format!("lib-{lib_name}"));
          let recorded_fingerprint = fs::read_to_string(fingerprint_path.with_extension("")).ok()?;
          let fingerprint_text = fs::read_to_string(fingerprint_path).ok()?;
          // A stale or malformed candidate fingerprint means "not this
          // artifact", never an abort — skip it and keep scanning.
          let fingerprint = CargoFingerprint::parse(&fingerprint_text).ok()?;
          (recorded_fingerprint == dependency_fingerprint
            && fingerprint.matches_current_build(&self.current_fingerprint, required_features))
          .then_some(artifact)
        })
        .collect::<Vec<_>>();

      candidates.sort_by_key(|path| {
        fs::metadata(path)
          .and_then(|metadata| metadata.modified())
          .unwrap_or(SystemTime::UNIX_EPOCH)
      });
      ensure_some(candidates.pop(), missing_context).map_err(|source| HarnessFailure::Artifact {
        directory: match self.layout {
          CargoLayout::Shared {
            ref profile,
          } => profile.join("deps"),
          CargoLayout::Isolated {
            ref build,
          } => build.clone(),
        },
        source,
      })
    }
  }

  fn artifact_hash<'a>(artifact: &'a Path, prefix: &str, extension: &str) -> Option<&'a str> {
    let artifact_file_name = artifact.file_name()?.to_str()?;
    let file_name = artifact_file_name.strip_prefix(prefix)?;
    if extension.is_empty() {
      return Some(file_name);
    }
    file_name.strip_suffix(&format!(".{extension}"))
  }

  /// Own the native path so a failed flag conversion can retain it intact.
  fn path_to_string(path: PathBuf) -> ArtifactPath {
    match path.to_str() {
      Some(text) if !text.contains(char::is_whitespace) => Ok(text.to_owned()),
      _ => Err(PredicateFailure {
        subject: path,
        source:  strict_test_support::ConditionFailure {
          condition: false,
          context:   "compiletest requires UTF-8 artifact paths without whitespace",
        },
      }),
    }
  }

  #[allow(
    clippy::single_call_fn,
    reason = "assembles the extern, L, and edition rustc flags for the compile-fail harness"
  )]
  fn rustc_flags() -> Result<String, HarnessFailure> {
    let cargo = CargoArtifacts::current()?;
    let proptest = cargo.extern_arg(
      "proptest",
      "proptest",
      "libproptest-",
      "rlib",
      &["std", "strict-test"],
      "a proptest rlib matching the current build exists",
    )?;
    let proptest_derive = cargo.extern_arg(
      "proptest-derive",
      "proptest_derive",
      &format!("{}proptest_derive-", env::consts::DLL_PREFIX),
      env::consts::DLL_EXTENSION,
      &[],
      "a proptest_derive dylib matching the current build exists",
    )?;
    let dependencies = cargo
      .dependency_dirs
      .into_iter()
      .map(|path| path_to_string(path).map(|directory| format!("-L {directory}")))
      .collect::<Result<Vec<_>, _>>()?
      .join(" ");

    Ok(format!("{dependencies} {proptest} {proptest_derive} --edition=2024"))
  }

  /// Select a declared compiletest revision without moving diagnostic lines or
  /// changing program text. Fixtures without revisions pass through unchanged.
  fn select_compiler_revision(source: String, revision: &str) -> SelectedRevision {
    let config = ct::Config::default();
    let mut selected = String::with_capacity(source.len());
    let mut header = true;
    for line in source.split_inclusive('\n') {
      let trimmed = line.trim();
      header &= !trimmed.starts_with("fn") && !trimmed.starts_with("mod");
      let revisions = trimmed
        .strip_prefix("//")
        .filter(|_| header)
        .and_then(|comment| config.parse_name_value_directive(comment.trim_start(), "revisions"));
      let Some(declared) = revisions else {
        selected.push_str(line);
        continue;
      };
      if !declared.split_whitespace().any(|candidate| candidate == revision) {
        return Err(PredicateFailure {
          subject: source,
          source:  strict_test_support::ConditionFailure {
            condition: false,
            context:   "the fixture declares the active compiler revision",
          },
        });
      }
      selected.push_str("// revisions: ");
      selected.push_str(revision);
      let ending = match line {
        text if text.ends_with("\r\n") => "\r\n",
        text if text.ends_with('\n') => "\n",
        _ => "",
      };
      selected.push_str(ending);
    }
    Ok(selected)
  }

  /// Read a fixture without losing its path or the underlying filesystem error.
  fn read_fixture(path: &Path) -> Result<Vec<u8>, HarnessFailure> {
    ensure_ok(fs::read(path), "the fixture is readable").map_err(|source| HarnessFailure::Read {
      path: path.to_path_buf(),
      source,
    })
  }

  /// Write fixture bytes through a path-preserving native I/O boundary.
  fn write_fixture(path: &Path, contents: &[u8]) -> Result<(), HarnessFailure> {
    fs::write(path, contents).map_err(|source| HarnessFailure::Write {
      path: path.to_path_buf(),
      source,
    })
  }

  /// Materialize one suite, retaining auxiliary directory structure and binary
  /// assets while specializing only declared compiler-revision headers.
  fn materialize_fixtures(source_dir: &Path, destination: &Path, revision: &str) -> Result<(), HarnessFailure> {
    fs::create_dir_all(destination).map_err(|source| HarnessFailure::Write {
      path: destination.to_path_buf(),
      source,
    })?;
    for entry in directory_entries(source_dir)? {
      let path = entry.path();
      let target = destination.join(entry.file_name());
      if path.is_dir() {
        materialize_fixtures(&path, &target, revision)?;
        continue;
      }
      let original = read_fixture(&path)?;
      let contents = if path.extension().is_some_and(|extension| extension == "rs") {
        let text = String::from_utf8(original).map_err(|source| HarnessFailure::SourceUtf8 {
          path: path.clone(),
          source,
        })?;
        select_compiler_revision(text, revision)
          .map_err(|source| HarnessFailure::Revision {
            path: path.clone(),
            source,
          })?
          .into_bytes()
      } else {
        original
      };
      write_fixture(&target, &contents)?;
    }
    Ok(())
  }

  #[allow(
    clippy::single_call_fn,
    reason = "configures and runs the compiletest_rs suite against the compile-fail fixtures"
  )]
  fn run_mode(src: &'static str, mode: Mode, compiler: &str, revision: &str, fixtures: &TempDir) -> Result<CompletedSuite, HarnessFailure> {
    let src_base = fixtures.child(src);
    materialize_fixtures(&Path::new("tests").join(src), &src_base, revision)?;
    let mut config = ct::Config {
      mode,
      rustc_path: compiler.into(),
      target_rustcflags: Some(rustc_flags()?),
      src_base,
      build_base: fixtures.child("build").join(src),
      ..ct::Config::default()
    };
    if let Ok(name) = env::var("TESTNAME") {
      config.filters = vec![name];
    }

    // `compiletest_rs` owns the pass/fail verdict of the UI cases; its
    // runner has no Result-returning entry point.
    ct::run_tests(&config);
    Ok(CompletedSuite(config))
  }

  #[test]
  fn compile_test() -> Result<(), Box<CompileFailure>> {
    let mut progress = CompileProgress {
      suites:   Vec::new(),
      rustc:    env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned()),
      version:  None,
      fixtures: None,
    };
    let result = (|| {
      let output = ensure_ok(Command::new(&progress.rustc).arg("--version").output(), "rustc --version executes")
        .map_err(HarnessFailure::VersionCommand)?;
      let nightly = str::from_utf8(&output.stdout).map(|version| version.contains("nightly"));
      progress.version = Some(output);
      let revision = if nightly.map_err(HarnessFailure::VersionUtf8)? {
        "nightly"
      } else {
        "stable"
      };
      let fixtures = progress.fixtures.insert(TempDir::new("derive-compiletest")?);
      progress
        .suites
        .push(run_mode("compile-fail", Mode::CompileFail, &progress.rustc, revision, fixtures)?);
      if revision == "nightly" {
        progress.suites.push(run_mode(
          "compile-fail-nightly",
          Mode::CompileFail,
          &progress.rustc,
          revision,
          fixtures,
        )?);
        progress
          .suites
          .push(run_mode("run-pass-nightly", Mode::RunPass, &progress.rustc, revision, fixtures)?);
      }
      Ok(())
    })();
    match result {
      Ok(()) => Ok(progress),
      Err(source) => Err(Box::new(CompileFailure {
        progress,
        source,
      })),
    }
    .map(drop)
  }

  /// Selected fixture source or the complete source rejected by its declaration.
  type SelectedRevision = Result<String, PredicateFailure<String>>;
  /// Complete selected source and its independently declared expected bytes.
  type RevisionObservation = (SelectedRevision, &'static str);

  #[test]
  fn compiler_revisions_preserve_program_text_and_line_endings() -> Result<(), PredicateFailure<Vec<RevisionObservation>>> {
    let observations = [
      (
        "// revisions: stable nightly\nfn main() {}\n",
        "stable",
        "// revisions: stable\nfn main() {}\n",
      ),
      (
        "// revisions: stable nightly\r\nfn main() {}\r\n",
        "nightly",
        "// revisions: nightly\r\nfn main() {}\r\n",
      ),
      ("// revisions: stable nightly", "nightly", "// revisions: nightly"),
      (
        "fn main() {}\n// revisions: stable nightly\n",
        "nightly",
        "fn main() {}\n// revisions: stable nightly\n",
      ),
      (
        "// a fixture without revisions\nfn main() {}\n",
        "nightly",
        "// a fixture without revisions\nfn main() {}\n",
      ),
    ]
    .into_iter()
    .map(|(source, revision, expected)| (select_compiler_revision(source.to_owned(), revision), expected))
    .collect();
    ensure_that(
      observations,
      "only the declared header changes, retaining diagnostic line positions and program bytes",
      |results: &Vec<RevisionObservation>| {
        results
          .iter()
          .all(|observation| observation.0.as_ref().is_ok_and(|selected| selected == observation.1))
      },
    )
    .map(drop)
  }

  #[test]
  fn compiler_revision_selection_rejects_an_undeclared_revision() -> Result<(), PredicateFailure<SelectedRevision>> {
    let source = "// revisions: stable\nfn main() {}\n";
    ensure_that(
      select_compiler_revision(source.to_owned(), "nightly"),
      "an unsupported revision retains the original annotated fixture",
      |result| {
        result
          .as_ref()
          .is_err_and(|failure| failure.subject == source && !failure.source.condition)
      },
    )
    .map(drop)
  }

  /// Observed source and materialized bytes, with their independent expectations.
  type FixtureCopies = Vec<(PathBuf, Vec<u8>, Vec<u8>)>;
  /// The resource owner survives every preparation or comparison failure.
  type FixtureObservation = (TempDir, Result<FixtureCopies, HarnessFailure>);
  /// Rejected fixture preparation keeps its resource owner and native failure.
  type InvalidFixtureObservation = (TempDir, Result<(), HarnessFailure>);
  /// Relative fixture path, original bytes, and expected materialized bytes.
  type FixtureFile = (&'static str, &'static [u8], &'static [u8]);

  /// Fixture allocation and complete preparation observations have distinct types.
  #[derive(Debug, thiserror::Error)]
  enum FixtureTestFailure {
    /// Allocation failed before there was a directory to preserve.
    #[error(transparent)]
    Fixture(#[from] TestFailure),
    /// Original and prepared filesystem observations remain inspectable.
    #[error(transparent)]
    Observation(Box<PredicateFailure<FixtureObservation>>),
    /// Invalid source retains the directory, original bytes, and native error.
    #[error(transparent)]
    InvalidSource(Box<PredicateFailure<InvalidFixtureObservation>>),
  }

  #[test]
  fn fixture_materialization_preserves_auxiliary_files_and_originals() -> Result<(), FixtureTestFailure> {
    let fixture = TempDir::new("derive-fixture-revisions")?;
    let observations: Result<FixtureCopies, HarnessFailure> = (|| {
      let source_dir = fixture.child("original");
      let auxiliary = source_dir.join("auxiliary");
      fs::create_dir_all(&auxiliary).map_err(|source| HarnessFailure::Write {
        path: auxiliary,
        source,
      })?;
      let files: [FixtureFile; 3] = [
        (
          "case.rs",
          b"// revisions: stable nightly\nfn main() {}\n",
          b"// revisions: nightly\nfn main() {}\n",
        ),
        ("auxiliary/helper.rs", b"pub fn helper() {}\n", b"pub fn helper() {}\n"),
        ("bytes.bin", &[0, 255, 42], &[0, 255, 42]),
      ];
      for (relative, original, _) in files {
        write_fixture(&source_dir.join(relative), original)?;
      }
      let prepared = fixture.child("selected");
      materialize_fixtures(&source_dir, &prepared, "nightly")?;
      files
        .into_iter()
        .flat_map(|(relative, original, selected)| [(source_dir.join(relative), original), (prepared.join(relative), selected)])
        .map(|(path, expected)| read_fixture(&path).map(|contents| (path, contents, expected.to_vec())))
        .collect()
    })();
    ensure_that(
      (fixture, observations),
      "revision materialization preserves the original tree, auxiliary Rust files, and binary assets",
      |subject| {
        subject
          .1
          .as_ref()
          .is_ok_and(|copies: &FixtureCopies| copies.len() == 6 && copies.iter().all(|copy| copy.1 == copy.2))
      },
    )
    .map(drop)
    .map_err(|failure| FixtureTestFailure::Observation(Box::new(failure)))
  }

  #[test]
  fn fixture_materialization_retains_invalid_source_bytes() -> Result<(), FixtureTestFailure> {
    let fixture = TempDir::new("derive-invalid-fixture")?;
    let result = (|| {
      let source = fixture.child("original");
      fs::create_dir_all(&source).map_err(|cause| HarnessFailure::Write {
        path:   source.clone(),
        source: cause,
      })?;
      write_fixture(&source.join("invalid.rs"), &[255, 0, 17])?;
      materialize_fixtures(&source, &fixture.child("selected"), "nightly")
    })();
    ensure_that(
      (fixture, result),
      "invalid Rust source retains its original bytes and path within the owned fixture",
      |subject| {
        matches!(subject.1, Err(HarnessFailure::SourceUtf8 { ref path, ref source })
        if *path == subject.0.child("original").join("invalid.rs") && source.as_bytes() == [255, 0, 17])
      },
    )
    .map(drop)
    .map_err(|failure| FixtureTestFailure::InvalidSource(Box::new(failure)))
  }

  #[test]
  fn fingerprint_parsing_uses_typed_fields() -> Result<(), PredicateFailure<Result<CargoFingerprint, FingerprintFailure>>> {
    ensure_that(
      CargoFingerprint::parse(r#"{"rustc":17,"config":23,"features":"[\"default\", \"std\"]","deps":[[41,"proptest",false,47]]}"#),
      "numeric hashes and exact feature tokens survive parsing",
      |result| {
        result.as_ref().is_ok_and(|fingerprint| {
          fingerprint
            == &CargoFingerprint {
              rustc:        17,
              config:       23,
              features:     [String::from("default"), String::from("std")].into_iter().collect(),
              dependencies: vec![(41, String::from("proptest"), false, 47)],
            }
            && fingerprint.dependency_hash("proptest") == Some(47)
            && fingerprint.dependency_hash("proptest_derive").is_none()
        })
      },
    )
    .map(drop)
  }

  /// Both native parses remain available if feature matching or decoding fails.
  type FingerprintPair = (
    Result<CargoFingerprint, FingerprintFailure>,
    Result<CargoFingerprint, FingerprintFailure>,
  );
  /// Both parsed fingerprints survive a failed feature-matching assertion.
  type FingerprintPairFailure = Box<PredicateFailure<FingerprintPair>>;

  #[test]
  fn fingerprint_feature_matching_uses_exact_tokens() -> Result<(), FingerprintPairFailure> {
    let current = CargoFingerprint::parse(r#"{"rustc":1,"config":2,"features":"[]","deps":[]}"#);
    let candidate = CargoFingerprint::parse(r#"{"rustc":1,"config":2,"features":"[\"default-code-coverage\", \"std\"]","deps":[]}"#);
    ensure_that(
      (current, candidate),
      "exact tokens match and prefixed feature names do not",
      |subject| match *subject {
        (Ok(ref active), Ok(ref inspected)) => {
          inspected.matches_current_build(active, &["std"]) && !inspected.matches_current_build(active, &["default"])
        }
        _ => false,
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// Preserve all three compared fingerprints without enlarging the return slot.
  type FingerprintMatchFailure = Box<PredicateFailure<[CargoFingerprint; 3]>>;

  #[test]
  fn fingerprint_matching_rejects_other_compilers_and_configurations() -> Result<(), FingerprintMatchFailure> {
    ensure_that(
      [
        CargoFingerprint {
          rustc:        17,
          config:       23,
          features:     BTreeSet::new(),
          dependencies: Vec::new(),
        },
        CargoFingerprint {
          rustc:        19,
          config:       23,
          features:     BTreeSet::new(),
          dependencies: Vec::new(),
        },
        CargoFingerprint {
          rustc:        17,
          config:       29,
          features:     BTreeSet::new(),
          dependencies: Vec::new(),
        },
      ],
      "matching features cannot admit artifacts built by another compiler or configuration",
      |fingerprints| {
        let [ref current, ref other_compiler, ref other_configuration] = *fingerprints;
        current.matches_current_build(current, &[])
          && !other_compiler.matches_current_build(current, &[])
          && !other_configuration.matches_current_build(current, &[])
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// Native layout discovery and its expected fingerprint location.
  type LayoutObservation = (Result<CargoLayout, PredicateFailure<PathBuf>>, PathBuf);
  /// Both layouts and expected fingerprint paths remain inspectable on failure.
  type LayoutMatchFailure = Box<PredicateFailure<[LayoutObservation; 2]>>;

  #[test]
  fn cargo_layouts_locate_library_fingerprints() -> Result<(), LayoutMatchFailure> {
    let executable = format!("compiletest-feed{}", env::consts::EXE_SUFFIX);
    let observations = [
      (
        PathBuf::from("target/custom-profile/deps").join(&executable),
        PathBuf::from("target/custom-profile/.fingerprint/proptest-abcd/lib-proptest.json"),
      ),
      (
        PathBuf::from("build-cache/custom-profile/build/proptest-derive/feed/out").join(executable),
        PathBuf::from("build-cache/custom-profile/build/proptest/abcd/fingerprint/lib-proptest.json"),
      ),
    ]
    .map(|(binary, expected)| (CargoLayout::from_executable(&binary), expected));
    ensure_that(
      observations,
      "shared and isolated layouts locate the requested package and hash under custom build roots",
      |layouts| {
        layouts.iter().all(|observation| {
          observation
            .0
            .as_ref()
            .is_ok_and(|layout| layout.fingerprint_path("proptest", "abcd", "lib-proptest") == observation.1)
        })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// Rejected executable paths remain intact in layout failures.
  type RejectedLayout = (PathBuf, Result<CargoLayout, PredicateFailure<PathBuf>>);

  #[test]
  fn cargo_layout_discovery_rejects_misplaced_binaries() -> Result<(), PredicateFailure<Vec<RejectedLayout>>> {
    let executable = format!("compiletest-feed{}", env::consts::EXE_SUFFIX);
    let observations = [
      "target/custom-profile",
      "target/custom-profile/build/another-package/feed/out",
      "target/custom-profile/build/proptest-derive/abcd/out",
      "target/custom-profile/another-root/proptest-derive/feed/out",
    ]
    .into_iter()
    .map(|directory| {
      let binary = Path::new(directory).join(&executable);
      let result = CargoLayout::from_executable(&binary);
      (binary, result)
    })
    .collect();
    ensure_that(
      observations,
      "misplaced binaries and mismatched build-unit identities retain their native paths",
      |layouts: &Vec<RejectedLayout>| {
        layouts
          .iter()
          .all(|observation| observation.1.as_ref().is_err_and(|failure| failure.subject == observation.0))
      },
    )
    .map(drop)
  }

  #[test]
  fn fingerprint_parsing_retains_invalid_input() -> Result<(), PredicateFailure<Result<CargoFingerprint, FingerprintFailure>>> {
    ensure_that(
      CargoFingerprint::parse("{"),
      "invalid outer JSON retains its input and parser diagnostic",
      |result| matches!(result, Err(FingerprintFailure::Json { input, source }) if input == "{" && source.is_eof()),
    )
    .map(drop)
  }

  #[test]
  fn fingerprint_parsing_rejects_missing_and_mistyped_fields() -> Result<(), PredicateFailure<FingerprintObservations>> {
    let observations = [
      serde_json::json!({"rustc": 17, "features": "[]", "deps": []}),
      serde_json::json!({"rustc": "17", "config": 23, "features": "[]", "deps": []}),
      serde_json::json!({"rustc": 17, "config": 23, "features": [], "deps": []}),
      serde_json::json!({"rustc": 17, "config": 23, "features": "[]"}),
    ]
    .into_iter()
    .map(|value| {
      let result = CargoFingerprint::parse(&value.to_string());
      (value, result)
    })
    .collect();
    ensure_that(
      observations,
      "invalid fingerprint fields retain the complete native JSON",
      |subjects: &FingerprintObservations| {
        subjects
          .iter()
          .all(|observed| matches!(observed.1, Err(FingerprintFailure::Fields { ref value }) if *value == observed.0))
      },
    )
    .map(drop)
  }

  #[test]
  fn fingerprint_parsing_retains_invalid_feature_lists() -> Result<(), PredicateFailure<FingerprintObservations>> {
    let observations = ["[", "[17]"]
      .into_iter()
      .map(|features| {
        let value = serde_json::json!({"rustc": 17, "config": 23, "features": features, "deps": []});
        let result = CargoFingerprint::parse(&value.to_string());
        (value, result)
      })
      .collect();
    ensure_that(
      observations,
      "malformed and mistyped feature arrays retain JSON and native diagnostics",
      |subjects: &FingerprintObservations| {
        matches!(subjects.as_slice(), [(first, Err(FingerprintFailure::Features { value: first_value, source: first_error })),
        (second, Err(FingerprintFailure::Features { value: second_value, source: second_error }))]
        if first == first_value && first_error.is_eof() && second == second_value && second_error.is_data())
      },
    )
    .map(drop)
  }

  #[test]
  fn fingerprint_parsing_retains_invalid_dependencies() -> Result<(), PredicateFailure<FingerprintObservations>> {
    let observations = [serde_json::json!(null), serde_json::json!([[41, "proptest", false, "47"]])]
      .into_iter()
      .map(|dependencies| {
        let value = serde_json::json!({"rustc": 17, "config": 23, "features": "[]", "deps": dependencies});
        let result = CargoFingerprint::parse(&value.to_string());
        (value, result)
      })
      .collect();
    ensure_that(
      observations,
      "invalid dependency identities retain the original JSON and native parser failures",
      |subjects: &FingerprintObservations| {
        subjects.iter().all(|observed| {
          matches!(observed.1, Err(FingerprintFailure::Dependencies { ref value, ref source })
            if *value == observed.0 && source.is_data())
        })
      },
    )
    .map(drop)
  }

  #[test]
  fn artifact_paths_require_unambiguous_flags() -> Result<(), PredicateFailure<[ArtifactPath; 2]>> {
    ensure_that(
      [
        path_to_string(PathBuf::from("deps/artifact.rlib")),
        path_to_string(PathBuf::from("deps/an artifact.rlib")),
      ],
      "ordinary paths survive conversion and whitespace paths remain native failures",
      |observations| {
        matches!(observations, [Ok(accepted), Err(rejected)]
        if accepted == "deps/artifact.rlib" && rejected.subject == Path::new("deps/an artifact.rlib"))
      },
    )
    .map(drop)
  }

  #[cfg(unix)]
  #[test]
  fn artifact_paths_retain_non_utf8_names() -> Result<(), PredicateFailure<ArtifactPath>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let path = PathBuf::from(OsString::from_vec(vec![0xff]));
    ensure_that(
      path_to_string(path.clone()),
      "non-UTF-8 paths survive failed flag conversion",
      |result| result.as_ref().is_err_and(|failure| failure.subject == path),
    )
    .map(drop)
  }
}
