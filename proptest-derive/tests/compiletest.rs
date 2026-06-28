// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate compiletest_rs as ct;

use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
    fn parse(json: &str) -> Self {
        let json = serde_json::from_str(json).expect("Cargo fingerprint JSON");
        Self {
            rustc: json_u64(&json, "rustc"),
            config: json_u64(&json, "config"),
            features: parse_feature_set(json_str(&json, "features")),
        }
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
    fn current() -> Self {
        let current_exe = env::current_exe().expect("current test binary path");
        let deps_dir = current_exe
            .parent()
            .expect("test binary deps dir")
            .to_owned();
        let fingerprint_dir = deps_dir
            .parent()
            .expect("target profile dir")
            .join(".fingerprint");

        let exe_hash = artifact_hash(
            &current_exe,
            "compiletest-",
            env::consts::EXE_EXTENSION,
        )
        .expect("compiletest artifact hash");
        let test_fingerprint = fingerprint_dir
            .join(format!("{}-{}", env!("CARGO_PKG_NAME"), exe_hash))
            .join("test-integration-test-compiletest.json");
        let test_fingerprint = fs::read_to_string(&test_fingerprint)
            .expect("compiletest fingerprint");

        Self {
            deps_dir,
            fingerprint_dir,
            current_fingerprint: CargoFingerprint::parse(&test_fingerprint),
        }
    }

    fn extern_arg(
        &self,
        package: &str,
        lib_name: &str,
        file_prefix: &str,
        extension: &str,
        required_features: &[&str],
    ) -> String {
        let path = self.resolve_artifact(
            package,
            lib_name,
            file_prefix,
            extension,
            required_features,
        );
        format!("--extern {}={}", lib_name, path_to_str(&path))
    }

    fn resolve_artifact(
        &self,
        package: &str,
        lib_name: &str,
        file_prefix: &str,
        extension: &str,
        required_features: &[&str],
    ) -> PathBuf {
        let entries = fs::read_dir(&self.deps_dir).expect("target deps dir");
        let mut candidates = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter_map(|artifact| {
                let hash = artifact_hash(&artifact, file_prefix, extension)?;
                let fingerprint = self
                    .fingerprint_dir
                    .join(format!("{}-{}", package, hash))
                    .join(format!("lib-{}.json", lib_name));
                let fingerprint = fs::read_to_string(fingerprint).ok()?;
                self.matches_current_build(&fingerprint, required_features)
                    .then_some(artifact)
            })
            .collect::<Vec<_>>();

        candidates.sort_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
        candidates
            .pop()
            .unwrap_or_else(|| panic!("no artifact found for {}", lib_name))
    }

    fn matches_current_build(
        &self,
        fingerprint: &str,
        required_features: &[&str],
    ) -> bool {
        CargoFingerprint::parse(fingerprint)
            .matches_current_build(&self.current_fingerprint, required_features)
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
    file_name.strip_suffix(&format!(".{}", extension))
}

fn json_u64(json: &Value, field: &str) -> u64 {
    json.get(field).and_then(Value::as_u64).unwrap_or_else(|| {
        panic!("missing numeric `{}` in Cargo fingerprint", field)
    })
}

fn json_str<'a>(json: &'a Value, field: &str) -> &'a str {
    json.get(field).and_then(Value::as_str).unwrap_or_else(|| {
        panic!("missing string `{}` in Cargo fingerprint", field)
    })
}

fn parse_feature_set(features: &str) -> BTreeSet<String> {
    serde_json::from_str::<Vec<String>>(features)
        .expect("Cargo fingerprint feature list")
        .into_iter()
        .collect()
}

fn path_to_str(path: &Path) -> &str {
    let path = path.to_str().expect("artifact path is valid UTF-8");
    assert!(
        !path.contains(char::is_whitespace),
        "compiletest cannot split paths with whitespace: {}",
        path
    );
    path
}

fn rustc_flags() -> String {
    let cargo = CargoArtifacts::current();
    let proptest = cargo.extern_arg(
        "proptest",
        "proptest",
        "libproptest-",
        "rlib",
        &["bit-set", "default", "fork", "std", "timeout"],
    );
    let proptest_derive = cargo.extern_arg(
        "proptest-derive",
        "proptest_derive",
        &format!("{}proptest_derive-", env::consts::DLL_PREFIX),
        env::consts::DLL_EXTENSION,
        &[],
    );

    format!(
        "-L {} {} {} --edition=2024",
        path_to_str(&cargo.deps_dir),
        proptest,
        proptest_derive,
    )
}

fn run_mode(src: &'static str, mode: &'static str) {
    let mut config = ct::Config {
        mode: mode.parse().expect("invalid mode"),
        target_rustcflags: Some(rustc_flags()),
        src_base: format!("tests/{}", src).into(),
        ..ct::Config::default()
    };
    if let Ok(name) = env::var("TESTNAME") {
        config.filters = vec![name];
    }

    ct::run_tests(&config);
}

#[test]
fn compile_test() {
    run_mode("compile-fail", "compile-fail");
}

#[test]
fn fingerprint_parsing_uses_typed_fields() {
    let fingerprint = CargoFingerprint::parse(
        r#"{"rustc":17,"config":23,"features":"[\"default\", \"std\"]"}"#,
    );

    assert_eq!(17, fingerprint.rustc);
    assert_eq!(23, fingerprint.config);
    assert!(fingerprint.features.contains("default"));
    assert!(fingerprint.features.contains("std"));
}

#[test]
fn fingerprint_feature_matching_uses_exact_tokens() {
    let current =
        CargoFingerprint::parse(r#"{"rustc":1,"config":2,"features":"[]"}"#);
    let candidate = CargoFingerprint::parse(
        r#"{"rustc":1,"config":2,"features":"[\"default-code-coverage\", \"std\"]"}"#,
    );

    assert!(candidate.matches_current_build(&current, &["std"]));
    assert!(!candidate.matches_current_build(&current, &["default"]));
}
