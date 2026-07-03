# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-macro/` — the `proptest-macro` procedural-macro crate (`[lib] proc-macro = true`, v0.5.0) providing the `#[property_test]` attribute macro: a terser alternative to a hand-written `proptest!` block, with per-argument `#[strategy = <expr>]` overrides for the default `Arbitrary` strategy. For shared conventions see the workspace-root `AGENTS.md`.

## Where the code is

`src/lib.rs` is a thin `#[proc_macro_attribute] pub fn property_test(attr, item)` shim: it only `.into()`-converts the `proc_macro::TokenStream` arguments to `proc_macro2` (and the result back) before delegating to the `property_test` module. Its rustdoc — the `# Example`, the optional `config = …` / `proptest_path = ::path::to::proptest` attributes, and the `#[strategy = <expr>]` example — is the **user-facing semver contract**, so behavioral documentation belongs there. The per-test struct the macro synthesizes (its name, fields, even whether it exists) is explicitly an implementation detail that can change without a major bump, so never let docs or callers depend on it.

Everything else lives under `src/property_test/` (its own `AGENTS.md`): a parse / validate / options front end, then `codegen/` (also its own `AGENTS.md`) that rewrites the annotated fn into a params struct, an `Arbitrary` impl, and `TestRunner` glue.

## Deps & testing

- Built on `syn` (feature `full`), `quote`, `proc-macro2`, and `convert_case`.
- Tested with **`insta` snapshot tests** (dev-deps `insta` + `prettyplease`, the latter pretty-printing the generated code into readable Rust for the snapshot):

  ```sh
  cargo test -p proptest-macro
  cargo insta review     # review/accept changed snapshots
  ```

When you change generated code, expect snapshots to change — review them deliberately rather than blindly accepting. For the full feature/test matrix, defer to the workspace-root `AGENTS.md`.
