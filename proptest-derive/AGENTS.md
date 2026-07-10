# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-derive/` — the procedural-macro crate (`[lib] proc-macro = true`, v0.8.0) providing `#[derive(Arbitrary)]`. It is a thin shim: the derive pipeline itself lives in the sibling `proptest-derive-internal` library crate (a proc-macro crate cannot expose `pub` modules, so the implementation is split out where real module boundaries are possible), and this crate converts `proc_macro`↔`proc_macro2` tokens and delegates. For shared conventions see the workspace-root `AGENTS.md`.

## Layout

- `src/` — `lib.rs` only: the `#[proc_macro_derive]` entry point delegating to `proptest_derive_internal::derive_arbitrary`. See `src/AGENTS.md`. The pipeline (tokens → IR → attribute parsing → bound inference → code generation) is documented in `../proptest-derive-internal/src/AGENTS.md`.
- `tests/` — `compiletest_rs` UI cases (`compile-fail/`) plus per-feature compile-and-run integration tests. See `tests/AGENTS.md`.
- `benches/large_enum.rs` — the lone `criterion` benchmark (declared `[[bench]] name = "large_enum"`, `harness = false`). It times building and sampling a derived strategy (`any::<T>()` → `new_tree` → read the value) for `LargeEnum1` (16 `String` variants) and `LargeEnum2` (16 variants, each wrapping a `LargeEnum1`) — i.e. the runtime cost of the enum/union codegen that `boxed_union` toggles. Run it with `cargo bench -p proptest-derive`.
- `README.md` — intentionally minimal (it only notes the crate is "currently experimental"); the real docs are the Proptest Book.

## Features & deps

- Feature `boxed_union` (off by default, pulls in no extra deps) — emit heap-allocated, type-erased `BoxedStrategy` unions instead of the static nested `TupleUnion` structs in derived enum code, trading an allocation for not building deep nested tuple types (which can stack-overflow on exceptionally large structures). This crate only forwards the flag (`boxed_union = ["proptest-derive-internal/boxed_union"]`); the codegen `cfg` and its specifics live in `proptest-derive-internal`.
- Compile-time deps: `proptest-derive-internal` (the pipeline) and `proc-macro2` (the token-conversion boundary). `syn`/`quote` are dependencies of the internal crate, not of this one.
- Dev-deps: `proptest`, `compiletest_rs`, `criterion`, `serde_json`, and `strict-test-support`. `proptest` is dev-only — as a proc-macro crate this emits code that names `proptest` in the *downstream* crate rather than linking it itself, so it isn't a normal dependency, but the integration tests and bench need it. `criterion` is the bench harness; `serde_json` is used by the compile-fail harness to read Cargo fingerprint JSON; `strict-test-support` provides the `ensure*` vocabulary the tests return through.
- `compiletest_rs` is pulled with `features = ["tmp", "stable"]` rather than its defaults: the suite is never actually run on stable (some cases use nightly features), but compiletest-rs's *default* features fail to compile (upstream laumann/compiletest-rs#166) while its `stable` fallback compiles fine. See the comment in `Cargo.toml`.

## Testing — requires nightly

Some cases use nightly-only features (e.g. `#![feature(never_type)]`), so the suite runs on nightly; run it both ways, since `boxed_union` changes the generated code:

```sh
cargo +nightly test -p proptest-derive
cargo +nightly test -p proptest-derive --features boxed_union
```

The expansion unit tests moved with the pipeline into `proptest-derive-internal` — run that crate's suite (both feature configs) alongside this one; this crate keeps everything that needs the *real* macro: the compile-fail UI suite, the integration tests, and the bench.

The full feature matrix, formatting, and other workspace-wide commands live in the workspace-root `AGENTS.md`.

## Gotcha

This crate is **sensitive to stale build artifacts** — the `compile-fail/` harness picks freshly-built `proptest`/`proptest_derive` artifacts by Cargo fingerprint, and stale copies can be matched by mistake (mechanics in `tests/AGENTS.md`). If the suite fails in ways that make no sense, `cargo clean` and retry.

## Changelog & conventions

User-observable changes get a `CHANGELOG.md` bullet under `## Unreleased`; for the subsection ordering, commit-message format, and copyright-header rules see the workspace-root `AGENTS.md`.
