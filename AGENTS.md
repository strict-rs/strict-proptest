# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

## What this is

`proptest` is a Hypothesis-style property-testing framework for Rust: you describe how to *generate* inputs with composable `Strategy` objects, and when a test fails proptest *shrinks* the input down to a minimal failing case and persists it for replay. Unlike QuickCheck, generation and shrinking are defined per-`Strategy` value rather than per-type, which is what makes strategies freely composable.

This is a Cargo workspace (`resolver = "3"`, edition 2024) of five crates:

- **`proptest/`** — the core library. `#![no_std]` with optional `std`/`alloc`. Almost all of the logic lives here.
- **`proptest-derive/`** — proc-macro crate providing `#[derive(Arbitrary)]`. A thin shim: it converts tokens and delegates to `proptest-derive-internal`. Testing it requires **nightly**.
- **`proptest-derive-internal/`** — ordinary library crate holding the `#[derive(Arbitrary)]` pipeline (parsing, attribute interpretation, bound inference, codegen). An implementation detail of `proptest-derive` — published alongside it, never depended on directly.
- **`proptest-macro/`** — proc-macro crate providing the `#[property_test]` attribute macro (a terser alternative to writing a `proptest!` block).
- **`proptest-state-machine/`** — state-machine / model-based testing built on top of `proptest`.

## Build, test, lint

Toolchain: the workspace `Cargo.toml` pins `edition = "2024"` / `rust-version = "1.96"` (the MSRV), inherited by every member crate via `.workspace = true`; the `README.md` MSRV note and the `pinned` CI job (`.github/workflows/rust.yml`) are kept in sync with it. Formatting is enforced by `rustfmt` with `max_width = 80` (`rustfmt.toml`); edition-2024 formatting requires the nightly formatter, so run `cargo +nightly fmt --all`.

Lint policy: the workspace `Cargo.toml` carries a `[workspace.lints]` table (rust, rustdoc, and clippy levels) that every member crate adopts via `lints.workspace = true`; `clippy.toml` holds the thresholds and the disallowed macro/method/type lists with their reason strings (panicking assertions and the legacy property-macro front doors are banned in favor of the `strict_test_support` `ensure*` helpers and `proptest::strict::ensure_property`; preconditions belong in `Strategy::prop_filter`). Every deny-level entry holds across the full clippy matrix below; warn-level entries are the visible residual ledger. Fix code rather than weakening the table, the thresholds, or adding `#[allow]`/`#[expect]`.

Full verification matrix:

```sh
cargo +nightly fmt --all
cargo check --workspace --all-targets --all-features
cargo check --workspace --all-targets --no-default-features
cargo check -p proptest --no-default-features --features "alloc libm alt-stable"
cargo clippy --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --no-default-features
cargo clippy -p proptest --no-default-features --features "alloc libm alt-stable"
cargo test --workspace --all-targets --all-features
cargo test --workspace --all-targets --no-default-features
cargo test -p proptest --no-default-features --features "alloc libm alt-stable"
cargo +nightly check --workspace --all-targets --all-features
cargo +nightly check --workspace --all-targets --no-default-features
cargo +nightly check --workspace --all-targets --features proptest/unstable
cargo +nightly check -p proptest --no-default-features --features "alloc libm unstable"
cargo +nightly clippy --workspace --all-targets --all-features
cargo +nightly clippy --workspace --all-targets --no-default-features
cargo +nightly clippy --workspace --all-targets --features proptest/unstable
cargo +nightly clippy -p proptest --no-default-features --features "alloc libm unstable"
cargo +nightly test --workspace --all-targets --all-features
cargo +nightly test --workspace --all-targets --no-default-features
cargo +nightly test --workspace --all-targets --features proptest/unstable
cargo +nightly test -p proptest --no-default-features --features "alloc libm unstable"
```

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

proptest-derive full UI coverage — **requires nightly** for the literal-`!`
fixtures. The stable compiletest route uses `core::convert::Infallible` for
uninhabited-type coverage and skips the nightly-only fixture directories. If
you get errors that make no sense, `cargo clean` and retry. The expansion unit
tests live in `proptest-derive-internal`; run both crates, both feature
configs:

```sh
cargo +nightly test -p proptest-derive
cargo +nightly test -p proptest-derive --features boxed_union
cargo +nightly test -p proptest-derive-internal
cargo +nightly test -p proptest-derive-internal --features boxed_union
```

proptest-state-machine:

```sh
cargo test -p proptest-state-machine
```

Cross-target no_std release checks are separate from the local verification
matrix above:

```sh
./prerelease-checks.sh   # cross-compiles no_std to thumbv7em + wasm32 (needs those targets + nightly)
```

Failure-persistence tests are **not** part of `cargo test`; run them directly:

```sh
cd proptest/test-persistence-location && ./run-tests.sh
```

## Strict ecosystem refactor reasoning

- Treat diagnostics as symptoms, not architecture. Before fixing a compile warning, Clippy warning, generated-code lint, mutation survivor, coverage gap, or test-fixture lint locally, identify the upstream owner of the shape: generator, macro expansion, parser model, API boundary, feature split, build workflow, or test harness.
- Do not rationalize new `#[allow]` / `#[expect]`, lint-policy changes, static exclusions, generated-output edits, compatibility shims, or non-idiomatic test fixtures because the current command is narrower, the allowance was pre-existing, or the problematic code is generated. New code written in this repo should already align with the stricter end-state.
- Do not use “input data” or “compile-fail fixture” as an escape hatch. Negative compiler behavior is a valid behavior to test, but checked-in compile-failing Rust source is suspected architectural debt until proven otherwise; prefer parser/token/IR tests, quoted macro input, expansion snapshots, typed diagnostics over generated temporary crates, or another narrower harness boundary. Keep compile-diagnostic `.rs` fixtures only when full `rustc` integration is the behavior under test, and normalize or delete incidental non-idiomatic syntax inside those fixtures.
- Preferred loop: identify the upstream owner, refactor there, add positive and negative behavior tests or expansion/workflow snapshots that pin the real contract, then remove the local lint debt.

## Architecture (the big picture)

**Generation vs. shrinking — `Strategy` + `ValueTree`** (`proptest/src/strategy/traits.rs`). A `Strategy` is the generation layer: `new_tree()` produces a `ValueTree`, and combinators (`prop_map`, `prop_filter`, `prop_flat_map`, `prop_perturb`, unions via `prop_oneof!`, …) build larger strategies from smaller ones. A `ValueTree` is the shrinking layer: it holds the generated value plus the state to walk it — `current()` reads the value, `simplify()` steps toward a *simpler* value, `complicate()` backtracks when a simpler value stopped reproducing the failure. Keeping shrink state inside the `ValueTree` (instead of re-deriving it from the output, as QuickCheck does) is what enables integrated shrinking — and is why proptest carries more state and is slower than QuickCheck.

**The runner / shrink loop** (`proptest/src/test_runner/`). `TestRunner` (`runner.rs`) drives execution: per case it seeds a PRNG, calls `Strategy::new_tree()`, runs the test closure, and on failure enters the shrink loop (`simplify`/`complicate` on the `ValueTree`) to minimize the input. Configuration — case count, env vars like `PROPTEST_CASES`/`PROPTEST_TIMEOUT` — lives in `config.rs`; minimized failures are written by `failure_persistence/` (the `proptest-regressions/` files) and can be replayed (`replay.rs`).

**The strict runner surface** (`proptest/src/strict.rs`, behind the default-on `strict-test` feature; requires `std`). `proptest::strict` is a Result-returning property harness over the same `TestRunner`: `ensure_property(strategy, context, property)` / `ensure_property_with_config(...)` run the closure as `Result<(), TestFailure>` (`TestFailure` re-exported from `strict-test-support`, `TestResult` as the alias) and map outcomes onto `TestFailure::PropertyFalsified` / `TestFailure::PropertyAborted` instead of panicking; a closure `Err` converts through `TestCaseError::fail`, so shrinking still runs. `strict_default_config()` starts from `Config::default()` (ordinary `PROPTEST_*` env behavior preserved), disables failure persistence (a strict run never writes `proptest-regressions/` files), and seeds deterministically from `STRICT_TEST_SEED`: unset or unparseable → fixed `0x5EED`, `random` → OS entropy, `<integer>` → that fixed seed.

**`Arbitrary` and the no_std split** (`proptest/src/arbitrary/`). `Arbitrary` (`arbitrary/traits.rs`) gives a type its canonical strategy via `any::<T>()` / `arbitrary_with(params)`. Impls are partitioned by what they require, wired up by `#[cfg]` in `arbitrary.rs`: `_core/` (always available — primitives, `Option`, `Result`), `_alloc/` (needs `alloc` — `Vec`, `String`, maps), `_std` (needs `std` — `Path`, IO, channels; the poisoning-prone `std::sync` locks — `Mutex`, `RwLock`, `Condvar`, and lock-dependent `WaitTimeoutResult` — deliberately have no `Arbitrary` impls, pinned by compile-fail fixtures).

**`std_facade` — the no_std bridge** (`proptest/src/std_facade.rs`). The crate is `#![no_std]`, so it must **never** name `std::` directly outside test code or `std`-gated code. Import allocated/std types from `crate::std_facade` instead (e.g. `Arc`, `Vec`, `Box`); it re-exports from `std`, `alloc`, or `core` depending on enabled features. New code that reaches for an allocating type must pull it from `std_facade` or it will break the `no_std`/`alloc` builds.

**Stable substitutes for nightly APIs** (`proptest/src/alt_stable.rs`,
behind the `alt-stable` feature). Exact impls for still-nightly standard
types (`alloc::Global` / `AllocError`, primitive `f16`,
`core::ops::CoroutineState`, and `std::net::Ipv6MulticastScope`) stay behind
`unstable` when `alt-stable` is **not** enabled. The `alt-stable` feature adds
stable substitutes instead: `allocator_api2::alloc::{Global, AllocError}`,
`half::f16`, and `proptest::alt_stable::{CoroutineState,
Ipv6MulticastScope}`. `alt-stable` wins when both features are enabled, so
stable `--all-features` builds select the substitute surface rather than
requesting `#![feature(...)]`.

**Sugar macros** (`proptest/src/sugar.rs`, with internal helpers in `proptest/src/macros.rs`). The user-facing surface: `proptest! { #[test] fn … (x in strat) { … } }`, `prop_assert!`/`prop_assert_eq!`, `prop_assume!` (reject the current case), `prop_compose!` (define a strategy), `prop_oneof!` (weighted union).

**Optional features** plug into the runner and are gated in `proptest/Cargo.toml`: `fork` (process-isolate each case via `rusty-fork`), `timeout` (per-case time limits; requires `fork`), `bit-set` (bitset strategies), `unstable` (exact nightly-only standard-library and language APIs), `f16` (primitive nightly `f16`, unless `alt-stable` selects `half::f16`), `alt-stable` (stable substitute APIs for the still-nightly surfaces), `handle-panics` (suppress intermediate panic spew during shrinking), `strict-test` (default-on; gates the `proptest::strict` Result-returning property harness and its `strict-test-support` dependency; requires `std`).

### Supporting crates

- **proptest-derive** (shim `src/lib.rs`) delegating to **proptest-derive-internal** (`lib.rs` → `derive.rs`): tokens → AST (`ast.rs`) → parse `#[proptest(...)]` attributes (`attr.rs`, evaluated in `interp.rs`) → track type-parameter usage to emit correct `Arbitrary` bounds (`use_tracking.rs`) → generate the `Arbitrary` impl. The `boxed_union` feature (forwarded by the shim) swaps generated `TupleUnion` structs for boxed strategies.
- **proptest-macro** (`src/property_test/`): `#[property_test]` validates the fn signature (`validate.rs` — including that the body returns `Result<(), TestFailure>` / `proptest::strict::TestResult`; `()` bodies are compile errors), reads options like `config = …` / `proptest_path = …` (`options.rs`), then rewrites the fn — synthesizing a params struct with an `Arbitrary` impl and running the body through `proptest::strict::ensure_property`, so the generated `#[test]` wrapper returns `proptest::strict::TestResult` instead of panicking on failure (`codegen/`). A per-argument `#[strategy = <expr>]` overrides the `Arbitrary` default.
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
