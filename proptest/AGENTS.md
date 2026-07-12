# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/` — the core property-testing library crate (`proptest` v1.11.0), `#![no_std]` with optional `std`/`alloc`. Almost all of the framework's logic lives here; the sibling crates (`proptest-derive`, `proptest-macro`, `proptest-state-machine`) build on it. For the workspace-wide build/test/lint matrix, the full no_std/feature build commands, the `#![no_std]` rules, copyright headers, changelog and commit conventions, see the workspace-root `AGENTS.md`.

## Layout

- `src/` — the library source: the per-type strategy modules plus the `strategy`/`test_runner`/`arbitrary` subsystems. See `src/AGENTS.md`.
- `examples/` — runnable examples that back the mdBook tutorial (`cargo run -p proptest --example <name>`); a few fail by design. See `examples/AGENTS.md`.
- `tests/` — the crate's only integration targets: `attr_macro.rs` (behind the `attr-macro` feature) plus the trybuild compile-pass fixtures under `tests/pass/` and compile-fail fixtures under `tests/fail/`. See `tests/AGENTS.md`.
- `test-persistence-location/` — a standalone harness verifying *where* regression files get written; `exclude`d from the workspace and driven by its own `./run-tests.sh`, not by `cargo test`. See `test-persistence-location/AGENTS.md`.
- `proptest-regressions/` — persisted minimized failing seeds, checked into source control. Don't delete them.
- `CHANGELOG.md` — record any user-observable change under `## Unreleased` (the current dev line targets MSRV 1.96 and the rand 0.10 family).

## Features (`Cargo.toml`)

Default set: `["std", "fork", "timeout", "bit-set", "strict-test"]`. The complete set, with what each pulls in and how they chain:

- `std` — standard-library support; pulls `rand/std`, `rand/sys_rng`, the `regex-syntax` dep, and `num-traits/std`. Gates the `path`/`string`/`range_subset` modules and the `_std` arbitrary tier.
- `libm` — enables `num-traits/libm` so float math such as `mul_add` works without `std`; use `--no-default-features` for the actual no-`std` build mode.
- `alloc` — empty toggle that turns on allocator-backed APIs in a `no_std` build (`Vec`, `String`, maps — resolved through `std_facade`).
- `fork` — process-isolate each test case via `rusty-fork`; pulls `rusty-fork` + `tempfile` and **requires `std`**.
- `timeout` — per-case time limits; pulls `rusty-fork/timeout` and **requires `fork`**.
- `bit-set` — bitset strategies; pulls the `bit-set` + `bit-vec` deps (via `dep:` syntax).
- `attr-macro` — pulls the optional `proptest-macro` dep and re-exports `#[property_test]`.
- `unstable` — exact nightly-only standard-library and language API support; enables `f16`, but the `cfg_attr` gates in `lib.rs` request `allocator_api`, `coroutine_trait`, `never_type`, and `ip` only when `alt-stable` is not enabled.
- `f16` — primitive `f16` float strategy support. It requires nightly unless `alt-stable` is also enabled, in which case the stable `half::f16` substitute is selected instead. Enabled implicitly by `unstable`.
- `alt-stable` — stable substitutes for APIs that are still nightly in `std`/`core`/`alloc`; pulls `allocator-api2` and `half`, and wins over `unstable` in combined feature sets so stable `--all-features` builds do not request nightly crate attributes.
- `hardware-rng` — use hardware/OS entropy instead of a static seed on supported no-`std` targets; pulls `getrandom`, with OS-less RDRAND consumers selecting getrandom's `rdrand` backend by cfg.
- `atomic64bit` — gates `Arbitrary` for the 64-bit atomics (`AtomicI64`/`AtomicU64`); per its comment, excludable on no_std targets that lack 64-bit atomics.
- `handle-panics` — hide intermediate panic spew flowing to stderr during the shrink phase; **requires `std`**.
- `strict-test` — gates the `strict` module (`proptest::strict`, the Result-returning property harness); pulls the optional `strict-test-support` dep (`TestFailure` and the `ensure*` helpers) and **requires `std`** (enables it explicitly). On by default.
- `default-code-coverage` — a coverage-friendly mirror of `default` (`std`, `fork`, `timeout`, `bit-set` — without `strict-test`).

## Dependencies (`Cargo.toml`)

Always on: `bitflags`, `unarray`, `num-traits`, `rand` (with its `alloc` feature), `rand_chacha`, `rand_xorshift`. Optional / feature-gated: `regex-syntax` (`std`), `bit-set` + `bit-vec` (`bit-set`), `allocator-api2` + `half` (`alt-stable`), `getrandom` (`hardware-rng`), `rusty-fork` + `tempfile` (`fork`), `proptest-macro` (`attr-macro`), `strict-test-support` (`strict-test`). Dev-only: `regex`, `trybuild`, `strict-test-support` (the `ensure*` vocabulary for test targets and trybuild fixtures). Versions are pinned centrally in the workspace `[workspace.dependencies]`.

## Generated docs

- `README.md` is **generated — don't hand-edit it.** `gen-readme.sh` concatenates `readme-prologue.md`, the awk-transformed `../book/src/{intro,getting-started,vs-quickcheck,limitations}.md`, and `readme-antelogue.md`. Edit those sources, then regenerate. The repo-root `README.md` is a symlink to this crate's `README.md`.
- `gen-docs.sh` is a maintainer-only rustdoc publisher (absolute paths into a local GH-Pages checkout); its `nostd` mode builds `--no-default-features --features=libm,alloc,unstable` on nightly. Normal stable no-`std` checks use `--features=libm,alloc` (or add `alt-stable` for substitute APIs). Not part of normal dev.
- `[package.metadata.docs.rs]` sets `all-features = true` and `rustdoc-args = ["--cfg", "docsrs"]`, which lights up the `#[doc(cfg(...))]` feature badges. `Cargo.toml` also `exclude`s `/gen-*.sh` and `/readme-*.md` from the published crate.

## Most-used commands

Full matrix is in the root `AGENTS.md`; the ones you'll reach for most here:

```sh
cargo test  -p proptest                 # whole core suite (inline #[cfg(test)] modules)
cargo test  -p proptest simple_example  # filter by name substring
cargo test  -p proptest --test attr_macro --features attr-macro  # the integration target
cargo build -p proptest --no-default-features --features std     # a no-`std`-leaning build check
cargo check -p proptest --no-default-features --features "alloc libm alt-stable"
```

## Gotcha

This crate is `#![no_std]`: never name `std::` outside test code or `std`-gated code — import allocated/std types from `crate::std_facade` instead, or you'll break the `no_std`/`alloc` builds.
