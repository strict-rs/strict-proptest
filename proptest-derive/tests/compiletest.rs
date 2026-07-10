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
//! `proptest_derive` artifacts in `deps/` by matching their Cargo
//! fingerprints. Two self-tests guard that fingerprint parsing and
//! feature matching stay exact.

extern crate compiletest_rs as ct;

use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use strict_test_support::{TestFailure, ensure, ensure_ok, ensure_some};

struct CargoArtifacts {
    deps_dir: PathBuf,
    fingerprint_dir: PathBuf,
    current_fingerprint: CargoFingerprint,
}

#[derive(Debug, PartialEq, Eq)]
struct CargoFingerprint {
    rustc: u64,
    config: u64,
    features: BTreeSet<String>,
}

impl CargoFingerprint {
    fn parse(json: &str) -> Result<Self, TestFailure> {
        let json: Value = ensure_ok(
            serde_json::from_str(json),
            "the Cargo fingerprint is valid JSON",
        )?;
        Ok(Self {
            rustc: json_u64(
                &json,
                "rustc",
                "the fingerprint carries a numeric rustc hash",
            )?,
            config: json_u64(
                &json,
                "config",
                "the fingerprint carries a numeric config hash",
            )?,
            features: parse_feature_set(json_str(
                &json,
                "features",
                "the fingerprint carries a feature list string",
            )?)?,
        })
    }

    fn matches_current_build(
        &self,
        current: &Self,
        required_features: &[&str],
    ) -> bool {
        self.rustc == current.rustc
            && self.config == current.config
            && required_features
                .iter()
                .all(|feature| self.features.contains(*feature))
    }
}

impl CargoArtifacts {
    #[allow(
        clippy::single_call_fn,
        reason = "locates the compiletest binary's own Cargo fingerprint to match sibling artifacts"
    )]
    fn current() -> Result<Self, TestFailure> {
        let current_exe = ensure_ok(
            env::current_exe(),
            "the current test binary path resolves",
        )?;
        let deps_dir = ensure_some(
            current_exe.parent(),
            "the test binary sits in the target deps dir",
        )?
        .to_owned();
        let fingerprint_dir = ensure_some(
            deps_dir.parent(),
            "the deps dir sits in the target profile dir",
        )?
        .join(".fingerprint");

        let exe_hash = ensure_some(
            artifact_hash(
                &current_exe,
                "compiletest-",
                env::consts::EXE_EXTENSION,
            ),
            "the compiletest binary name carries a Cargo hash",
        )?;
        let test_fingerprint = fingerprint_dir
            .join(format!("{}-{}", env!("CARGO_PKG_NAME"), exe_hash))
            .join("test-integration-test-compiletest.json");
        let test_fingerprint = ensure_ok(
            fs::read_to_string(&test_fingerprint),
            "the compiletest fingerprint file is readable",
        )?;

        Ok(Self {
            deps_dir,
            fingerprint_dir,
            current_fingerprint: CargoFingerprint::parse(&test_fingerprint)?,
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
    ) -> Result<String, TestFailure> {
        let path = self.resolve_artifact(
            package,
            lib_name,
            file_prefix,
            extension,
            required_features,
            missing_context,
        )?;
        Ok(format!("--extern {}={}", lib_name, path_to_str(&path)?))
    }

    fn resolve_artifact(
        &self,
        package: &str,
        lib_name: &str,
        file_prefix: &str,
        extension: &str,
        required_features: &[&str],
        missing_context: &'static str,
    ) -> Result<PathBuf, TestFailure> {
        let entries = ensure_ok(
            fs::read_dir(&self.deps_dir),
            "the target deps dir is readable",
        )?;
        let mut candidates = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter_map(|artifact| {
                let hash = artifact_hash(&artifact, file_prefix, extension)?;
                let fingerprint = self
                    .fingerprint_dir
                    .join(format!("{package}-{hash}"))
                    .join(format!("lib-{lib_name}.json"));
                let fingerprint = fs::read_to_string(fingerprint).ok()?;
                // A stale or malformed candidate fingerprint means "not this
                // artifact", never an abort — skip it and keep scanning.
                let fingerprint = CargoFingerprint::parse(&fingerprint).ok()?;
                fingerprint
                    .matches_current_build(
                        &self.current_fingerprint,
                        required_features,
                    )
                    .then_some(artifact)
            })
            .collect::<Vec<_>>();

        candidates.sort_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
        ensure_some(candidates.pop(), missing_context)
    }
}

fn artifact_hash<'a>(
    artifact: &'a Path,
    prefix: &str,
    extension: &str,
) -> Option<&'a str> {
    let file_name = artifact.file_name()?.to_str()?;
    let file_name = file_name.strip_prefix(prefix)?;
    if extension.is_empty() {
        return Some(file_name);
    }
    file_name.strip_suffix(&format!(".{extension}"))
}

fn json_u64(
    json: &Value,
    field: &str,
    context: &'static str,
) -> Result<u64, TestFailure> {
    ensure_some(json.get(field).and_then(Value::as_u64), context)
}

#[allow(
    clippy::single_call_fn,
    reason = "extracts a required string field from a parsed Cargo fingerprint JSON value"
)]
fn json_str<'a>(
    json: &'a Value,
    field: &str,
    context: &'static str,
) -> Result<&'a str, TestFailure> {
    ensure_some(json.get(field).and_then(Value::as_str), context)
}

#[allow(
    clippy::single_call_fn,
    reason = "decodes the fingerprint's double-encoded JSON feature list into a set"
)]
fn parse_feature_set(features: &str) -> Result<BTreeSet<String>, TestFailure> {
    Ok(ensure_ok(
        serde_json::from_str::<Vec<String>>(features),
        "the fingerprint feature list is a JSON string array",
    )?
    .into_iter()
    .collect())
}

fn path_to_str(path: &Path) -> Result<&str, TestFailure> {
    let path = ensure_some(path.to_str(), "the artifact path is valid UTF-8")?;
    ensure(
        !path.contains(char::is_whitespace),
        "compiletest cannot split paths with whitespace",
    )?;
    Ok(path)
}

#[allow(
    clippy::single_call_fn,
    reason = "assembles the extern, L, and edition rustc flags for the compile-fail harness"
)]
fn rustc_flags() -> Result<String, TestFailure> {
    let cargo = CargoArtifacts::current()?;
    let proptest = cargo.extern_arg(
        "proptest",
        "proptest",
        "libproptest-",
        "rlib",
        &["bit-set", "default", "fork", "std", "timeout"],
        "a proptest rlib matching the current build exists in deps",
    )?;
    let proptest_derive = cargo.extern_arg(
        "proptest-derive",
        "proptest_derive",
        &format!("{}proptest_derive-", env::consts::DLL_PREFIX),
        env::consts::DLL_EXTENSION,
        &[],
        "a proptest_derive dylib matching the current build exists in deps",
    )?;

    Ok(format!(
        "-L {} {} {} --edition=2024",
        path_to_str(&cargo.deps_dir)?,
        proptest,
        proptest_derive,
    ))
}

#[allow(
    clippy::single_call_fn,
    reason = "configures and runs the compiletest_rs suite against the compile-fail fixtures"
)]
fn run_mode(src: &'static str, mode: &'static str) -> Result<(), TestFailure> {
    let mut config = ct::Config {
        mode: ensure_some(
            mode.parse().ok(),
            "the compiletest mode string parses",
        )?,
        target_rustcflags: Some(rustc_flags()?),
        src_base: format!("tests/{src}").into(),
        ..ct::Config::default()
    };
    if let Ok(name) = env::var("TESTNAME") {
        config.filters = vec![name];
    }

    // `compiletest_rs` owns the pass/fail verdict of the UI cases; its
    // runner has no Result-returning entry point.
    ct::run_tests(&config);
    Ok(())
}

#[test]
fn compile_test() -> Result<(), TestFailure> {
    run_mode("compile-fail", "compile-fail")
}

#[test]
fn fingerprint_parsing_uses_typed_fields() -> Result<(), TestFailure> {
    let fingerprint = CargoFingerprint::parse(
        r#"{"rustc":17,"config":23,"features":"[\"default\", \"std\"]"}"#,
    )?;

    ensure(
        fingerprint.rustc == 17,
        "the rustc hash is extracted as a number",
    )?;
    ensure(
        fingerprint.config == 23,
        "the config hash is extracted as a number",
    )?;
    ensure(
        fingerprint.features.contains("default"),
        "the decoded feature set carries default",
    )?;
    ensure(
        fingerprint.features.contains("std"),
        "the decoded feature set carries std",
    )
}

#[test]
fn fingerprint_feature_matching_uses_exact_tokens() -> Result<(), TestFailure> {
    let current =
        CargoFingerprint::parse(r#"{"rustc":1,"config":2,"features":"[]"}"#)?;
    let candidate = CargoFingerprint::parse(
        r#"{"rustc":1,"config":2,"features":"[\"default-code-coverage\", \"std\"]"}"#,
    )?;

    ensure(
        candidate.matches_current_build(&current, &["std"]),
        "an exact feature token satisfies the requirement",
    )?;
    ensure(
        !candidate.matches_current_build(&current, &["default"]),
        "a prefixed feature token does not satisfy the requirement",
    )
}
