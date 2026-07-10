# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-derive-internal/` — the ordinary library crate (`[lib]`, not a proc-macro, v0.8.0) that holds the `#[derive(Arbitrary)]` implementation. The published `proptest-derive` crate is a thin `#[proc_macro_derive]` shim that converts `proc_macro`↔`proc_macro2` tokens and delegates to this crate's `derive_arbitrary` entry point. This split exists because a proc-macro crate cannot expose `pub` modules, which the workspace visibility lints (`unreachable_pub`, `clippy::redundant_pub_crate`) require for crate-internal cross-module sharing; a normal library crate can. For shared conventions see the workspace-root `AGENTS.md`.

## Layout

- `src/` — the derive pipeline (tokens → IR → attribute parsing → bound inference → code generation) plus the inline `#[cfg(test)]` expansion tests (`tests.rs`). See `src/AGENTS.md`.
- The single stable entry point is `derive_arbitrary(proc_macro2::TokenStream) -> proc_macro2::TokenStream` in `lib.rs`: it parses via `syn::parse2` and returns either the generated `impl` or, for a malformed token stream (unreachable from rustc-driven derives), the parse error as a `compile_error!` — there is no panic path. The pipeline modules are `pub mod` boundaries whose contents are `pub(crate)`/private, so nothing else is part of a stable API; the crate is an implementation detail and must not be depended on directly.

## Features & deps

- Feature `boxed_union` (off by default) — emit heap-allocated `BoxedStrategy` unions instead of nested `TupleUnion` structs in derived enum code. The `proptest-derive` shim forwards it via `boxed_union = ["proptest-derive-internal/boxed_union"]`; this crate defines the `cfg`.
- Compile-time deps: `proc-macro2`, `quote`, and `syn` with features `visit`, `extra-traits`, and `full`.
- Dev-deps: `strict-test-support` only. The emitted code names `proptest` in the *downstream* crate, and compile-behavior coverage lives in the sibling `proptest-derive` integration tests, so this implementation crate does not depend on `proptest`.

## Module visibility shape

`lib.rs` declares every pipeline module as a plain, documented `pub mod`. A `pub(crate)` item inside a genuinely `pub` module fires neither `unreachable_pub` (not `pub`) nor `redundant_pub_crate` (not inside a *private* module). Every module satisfies `missing_docs` with real documentation (module-level `//!` docs, or a `///` on the `pub mod` declaration for `interp`) — never with `#[doc(hidden)]`; the crate's rustdoc deliberately documents its own internals. Keep contents `pub(crate)` for cross-module items and plain private `fn` for module-local helpers. A genuinely-`pub` item enters the public API and must carry docs and `#[derive(Debug)]` and avoid `len`/`must_use`-candidate shapes; the `Params` IR type is the only such item besides `derive_arbitrary` (its `empty`/`len` methods are kept `pub(crate)` for that reason). In `error.rs` the `error!` macro's `local`-prefixed arms emit private `fn` for the constructors only the in-file checkers call; its unmarked arms and every `fatal!` arm emit `pub(crate) fn`.

## Testing — requires nightly

Some cases use nightly-only features, and `boxed_union` changes the generated code, so run both ways:

```sh
cargo +nightly test -p proptest-derive-internal
cargo +nightly test -p proptest-derive-internal --features boxed_union
```

The compile-fail UI suite, the per-feature integration tests, and the `large_enum` benchmark exercise the *real* `#[derive(Arbitrary)]` macro and therefore live in `proptest-derive`, not here.

## Changelog & conventions

This crate is an implementation detail documented through `proptest-derive`'s `CHANGELOG.md`; it keeps no changelog of its own. For the commit-message format and copyright-header rules see the workspace-root `AGENTS.md`.
