# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/test-persistence-location/` — a standalone harness verifying *where* proptest writes its `proptest-regressions/` failure files: anchored at the crate under test and mirroring the failing test's source path. For shared conventions see the workspace-root `AGENTS.md`.

## Why it's separate

Two self-contained mini-projects, each its own Cargo workspace with a `path` dependency on the core crate, deliberately kept out of the main workspace so building/testing proptest never touches them — `cargo test` does **not** run them:

- `single-crate/` — a lone crate (`name = "single-crate"`); its `Cargo.toml` carries an empty `[workspace]` table so it roots its own workspace (cargo won't walk up to an ancestor), and depends on proptest via `path = "../.."`.
- `workspace/` — a multi-crate workspace (`members = ["member"]`) whose `member/` (`name = "member"`) depends on proptest via `path = "../../.."`. Its `[workspace] exclude = ["../..", "../../.."]` stops that path dependency from dragging the proptest crate and the repo root in as members of this throwaway workspace.

Both `../..` and `../../..` resolve to the core `proptest/` crate. Drive the harness with its script, not `cargo test`:

```sh
cd proptest/test-persistence-location && ./run-tests.sh   # run-tests.bat on Windows
```

## What it checks

Each project's `src/submodule/code.rs` defines a `proptest!` test that *always* `panic!()`s. The failure is the entire point: a failing case is what makes proptest persist a regression seed, so the harness deliberately provokes one and then asserts the seed landed at the expected path:

- `single-crate/` → `single-crate/proptest-regressions/submodule/code.txt`
- `workspace/` → `workspace/member/proptest-regressions/submodule/code.txt`

The path *is* the property under test. Two invariants must hold: the `submodule/code` nesting mirrors the failing test's source path (`src/submodule/code.rs` → `proptest-regressions/submodule/code.txt`), and the `proptest-regressions/` root sits at the **crate** directory — in the workspace case that is `member/`, not the workspace root. That crate-root-vs-workspace-root anchoring is precisely what regresses if path-resolution logic changes, so re-run this harness after touching `proptest/src/test_runner/failure_persistence/`.

## run-tests.sh vs run-tests.bat

Same two-scenario check, one script per platform; they differ only in mechanics:

- POSIX `run-tests.sh`: opens with a `find … -name '*.txt' -o -name proptest-regressions … -exec rm -rf` cleanup of the prior run's output, then per scenario runs `cargo test` (single-crate) / `cargo test --all` (workspace) capturing stdout+stderr to `cargo-out.txt`, `cargo clean`s, and `test -f`s the expected file. On a miss it dumps `find .` and the captured output to stderr and exits non-zero; success is silent (exit 0).
- Windows `run-tests.bat`: cleans per scenario with `rd /s /q proptest-regressions` (it does not sweep `*.txt`), captures stdout only to `cargo.txt`, and on a missing file does `goto fail`. It prints an explicit `PASS` on success and, under `:fail`, dumps `dir /s` plus the captured output and exits 1.

Note the captured-output filenames differ (`cargo-out.txt` vs `cargo.txt`). Both of those, along with the generated `proptest-regressions/` dirs, are git-ignored here (see `.gitignore`) because they are throwaway test artifacts — unlike the real `proptest-regressions/` directories elsewhere in the repo, which are checked into source control.
