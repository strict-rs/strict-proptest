# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-macro/` — the `proptest-macro` procedural-macro crate (`[lib] proc-macro = true`) providing the `#[property_test]` attribute macro. The annotated body returns `Result<A, E>` or an alias; the generated wrapper returns `proptest::test_runner::PropertyResult<(ArgumentTypes, ...), A, E, TransportError>`. Per-argument `#[strategy = <expr>]` overrides replace the default `Arbitrary` strategies. A missing return type or literal `-> ()` is a compile error. For shared conventions see the workspace-root `AGENTS.md`.

## Where the code is

`src/lib.rs` is a thin `#[proc_macro_attribute] pub fn property_test(attr, item)` shim: it converts `proc_macro::TokenStream` to and from `proc_macro2` and delegates to the `property_test` module. Its rustdoc is the user-facing semver contract, including `config = …`, `proptest_path = ::path::to::proptest`, `transport = CodecType => codec_expression`, and `#[strategy = <expr>]`. Counterexamples are concrete argument tuples in declaration order, with diagnostic labels retained separately in the run.

Everything else lives under `src/property_test/` (its own `AGENTS.md`): a parse / validate / options front end, then `codegen/` (also its own `AGENTS.md`) that builds a tuple strategy, a typed callback preserving the original patterns and return type, and a strict runner invocation. Explicit transport selects `ensure_property_with_transport`; other wrappers use `ensure_property_with_config` with caller configuration or strict defaults.

## Deps & testing

- Built on `syn` (feature `full`), `quote`, and `proc-macro2`.
- Expansion tests use `prettyplease` and `strict_test_support::ensure_snapshot`, with committed artifacts under `tests/snapshots/`. The `proptest` dev-dependency executes the public macro doctests:

  ```sh
  cargo test -p proptest-macro
  SNAPSHOTS=overwrite cargo test -p proptest-macro
  git diff -- proptest-macro/tests/snapshots
  ```

When you change generated code, expect snapshots to change — review them deliberately rather than blindly accepting. For the full feature/test matrix, defer to the workspace-root `AGENTS.md`.
