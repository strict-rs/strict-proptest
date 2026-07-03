# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

## What this is

`proptest` is a Hypothesis-style property-testing framework for Rust: you describe how to *generate* inputs with composable `Strategy` objects, and when a test fails proptest *shrinks* the input down to a minimal failing case and persists it for replay. Unlike QuickCheck, generation and shrinking are defined per-`Strategy` value rather than per-type, which is what makes strategies freely composable.

This is a Cargo workspace (`resolver = "3"`, edition 2024) of four published crates:

- **`proptest/`** — the core library. `#![no_std]` with optional `std`/`alloc`. Almost all of the logic lives here.
- **`proptest-derive/`** — proc-macro crate providing `#[derive(Arbitrary)]`. Testing it requires **nightly**.
- **`proptest-macro/`** — proc-macro crate providing the `#[property_test]` attribute macro (a terser alternative to writing a `proptest!` block).
- **`proptest-state-machine/`** — state-machine / model-based testing built on top of `proptest`.

## Build, test, lint

Toolchain: the workspace `Cargo.toml` pins `edition = "2024"` / `rust-version = "1.96"` (the MSRV), inherited by every member crate via `.workspace = true`; the `README.md` MSRV note and the `pinned` CI job (`.github/workflows/rust.yml`) are kept in sync with it. Formatting is enforced by `rustfmt` with `max_width = 80` (`rustfmt.toml`); edition-2024 formatting requires the nightly formatter, so run `cargo +nightly fmt --all`.

Core crate:

```sh
cargo build -p proptest
cargo test  -p proptest                 # whole core test suite (inline #[cfg(test)] modules)
cargo test  -p proptest simple_example  # run a single test / filter by name substring
```

`#[property_test]` integration test (lives in `proptest/tests/attr_macro.rs`, behind the `attr-macro` feature — this is the only integration-test target in the core crate):

```sh
cargo test -p proptest --test attr_macro --features attr-macro
```

proptest-macro (snapshot tests via `insta`; review changed snapshots with `cargo insta review`):

```sh
cargo test -p proptest-macro
```

proptest-derive — **requires nightly**, and is sensitive to stale build artifacts: if you get errors that make no sense, `cargo clean` and retry (it uses `compiletest_rs` with UI cases under `tests/compile-fail/` and `tests/*.rs`):

```sh
cargo +nightly test -p proptest-derive
cargo +nightly test -p proptest-derive --features boxed_union
```

proptest-state-machine:

```sh
cargo test -p proptest-state-machine
```

Feature-matrix / no_std builds (compile-only checks — the test suite is **not** no_std-clean, so only the build is verified). The default feature set is `["std", "fork", "timeout", "bit-set", "strict-test"]`:

```sh
cargo build -p proptest --no-default-features --features std
cargo build -p proptest --no-default-features --features fork
cargo +nightly build -p proptest --no-default-features --features "no_std alloc unstable hardware-rng"
./prerelease-checks.sh   # cross-compiles no_std to thumbv7em + wasm32 (needs those targets + nightly)
```

Failure-persistence tests are **not** part of `cargo test`; run them directly:

```sh
cd proptest/test-persistence-location && ./run-tests.sh
```

## Architecture (the big picture)

**Generation vs. shrinking — `Strategy` + `ValueTree`** (`proptest/src/strategy/traits.rs`). A `Strategy` is the generation layer: `new_tree()` produces a `ValueTree`, and combinators (`prop_map`, `prop_filter`, `prop_flat_map`, `prop_perturb`, unions via `prop_oneof!`, …) build larger strategies from smaller ones. A `ValueTree` is the shrinking layer: it holds the generated value plus the state to walk it — `current()` reads the value, `simplify()` steps toward a *simpler* value, `complicate()` backtracks when a simpler value stopped reproducing the failure. Keeping shrink state inside the `ValueTree` (instead of re-deriving it from the output, as QuickCheck does) is what enables integrated shrinking — and is why proptest carries more state and is slower than QuickCheck.

**The runner / shrink loop** (`proptest/src/test_runner/`). `TestRunner` (`runner.rs`) drives execution: per case it seeds a PRNG, calls `Strategy::new_tree()`, runs the test closure, and on failure enters the shrink loop (`simplify`/`complicate` on the `ValueTree`) to minimize the input. Configuration — case count, env vars like `PROPTEST_CASES`/`PROPTEST_TIMEOUT` — lives in `config.rs`; minimized failures are written by `failure_persistence/` (the `proptest-regressions/` files) and can be replayed (`replay.rs`).

**The strict runner surface** (`proptest/src/strict.rs`, behind the default-on `strict-test` feature; requires `std`). `proptest::strict` is a Result-returning property harness over the same `TestRunner`: `ensure_property(strategy, context, property)` / `ensure_property_with_config(...)` run the closure as `Result<(), TestFailure>` (`TestFailure` re-exported from `strict-test-support`, `TestResult` as the alias) and map outcomes onto `TestFailure::PropertyFalsified` / `TestFailure::PropertyAborted` instead of panicking; a closure `Err` converts through `TestCaseError::fail`, so shrinking still runs. `strict_default_config()` starts from `Config::default()` (ordinary `PROPTEST_*` env behavior preserved), disables failure persistence (a strict run never writes `proptest-regressions/` files), and seeds deterministically from `STRICT_TEST_SEED`: unset or unparseable → fixed `0x5EED`, `random` → OS entropy, `<integer>` → that fixed seed.

**`Arbitrary` and the no_std split** (`proptest/src/arbitrary/`). `Arbitrary` (`arbitrary/traits.rs`) gives a type its canonical strategy via `any::<T>()` / `arbitrary_with(params)`. Impls are partitioned by what they require, wired up by `#[cfg]` in `arbitrary/mod.rs`: `_core/` (always available — primitives, `Option`, `Result`), `_alloc/` (needs `alloc` — `Vec`, `String`, maps), `_std/` (needs `std` — `Path`, IO, sync types).

**`std_facade` — the no_std bridge** (`proptest/src/std_facade.rs`). The crate is `#![no_std]`, so it must **never** name `std::` directly outside test code or `std`-gated code. Import allocated/std types from `crate::std_facade` instead (e.g. `Arc`, `Vec`, `Box`); it re-exports from `std`, `alloc`, or `core` depending on enabled features. New code that reaches for an allocating type must pull it from `std_facade` or it will break the `no_std`/`alloc` builds.

**Sugar macros** (`proptest/src/sugar.rs`, with internal helpers in `proptest/src/macros.rs`). The user-facing surface: `proptest! { #[test] fn … (x in strat) { … } }`, `prop_assert!`/`prop_assert_eq!`, `prop_assume!` (reject the current case), `prop_compose!` (define a strategy), `prop_oneof!` (weighted union).

**Optional features** plug into the runner and are gated in `proptest/Cargo.toml`: `fork` (process-isolate each case via `rusty-fork`), `timeout` (per-case time limits; requires `fork`), `bit-set` (bitset strategies), `unstable` (nightly-only language features), `handle-panics` (suppress intermediate panic spew during shrinking), `strict-test` (default-on; gates the `proptest::strict` Result-returning property harness and its `strict-test-support` dependency; requires `std`).

### Supporting crates

- **proptest-derive** (`src/lib.rs` → `derive.rs`): tokens → AST (`ast.rs`) → parse `#[proptest(...)]` attributes (`attr.rs`, evaluated in `interp.rs`) → track type-parameter usage to emit correct `Arbitrary` bounds (`use_tracking.rs`) → generate the `Arbitrary` impl. The `boxed_union` feature swaps generated `TupleUnion` structs for boxed strategies.
- **proptest-macro** (`src/property_test/`): `#[property_test]` validates the fn signature (`validate.rs`), reads options like `config = …` / `proptest_path = …` (`options.rs`), then rewrites the fn — synthesizing a params struct with an `Arbitrary` impl and wrapping the body in a runner (`codegen/`). A per-argument `#[strategy = <expr>]` overrides the `Arbitrary` default.
- **proptest-state-machine** (`src/strategy.rs`, `src/test_runner.rs`): you implement `ReferenceStateMachine` (an abstract model — `State`, `Transition`, `transitions()`, `apply()`, preconditions) and `StateMachineTest` (the real system under test — `apply()`, `check_invariants()`). The `prop_state_machine!` macro expands to a `proptest!` that generates and shrinks transition *sequences*. See `examples/state_machine_heap.rs` and `examples/state_machine_echo_server.rs`.

## Conventions & gotchas

- **Don't hand-edit `proptest/README.md`** — it's generated by `proptest/gen-readme.sh` from `book/src/*.md` plus `readme-prologue.md`/`readme-antelogue.md`. Edit those sources. The user guide is the mdBook under `book/`.
- **Copyright headers:** every source file starts with the template in `proptest/src/file-preamble`; new files get it filled in with the current year and "The proptest developers" (see `CONTRIBUTING.md`).
- **Changelog:** any user-observable change gets a bullet under an `## Unreleased` section in the relevant crate's `CHANGELOG.md`. Subsections, in order: Breaking Changes, Deprecations, Bug Fixes, New Additions, Nightly-only Breakage, Other Notes.
- **`proptest-regressions/`** directories hold persisted failing seeds and are checked into source control — don't delete them.
- Non-code prose in this repo is historically hard-wrapped to 80 columns; match the surrounding file when editing existing prose, but don't reflow untouched lines.

## Commit messages

Use Conventional-Commit style: `type(scope): imperative description` naming the *structural* change. **Scope is required.** **Never use `chore`** — choose a descriptive type (`feat`, `fix`, `refactor`, `perf`, `build`, `ci`, `docs`, `test`, `style`, `revert`, …); append `!` for breaking changes (e.g. `feat(runner)!:`).

Body: 1–5 sections sized to the change. Each section opens with a plain-text header line (no markdown `#`, no bold), followed by 3–5 imperative bullets describing structural changes; separate sections with one blank line. Pass the message via a HEREDOC to `git commit -m` so blank lines and bullet spacing survive shell quoting.
