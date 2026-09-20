# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-macro/src/property_test/codegen/` — the code-generation stage of `#[property_test]`. It produces a test wrapper returning `PropertyResult` with a concrete argument tuple, native success and failure types, and the selected transport error. `<proptest>` below is the path resolved by `options.true_proptest_path()`. Shared conventions and snapshot commands live in the parent guidance.

## Entry point: `generate()` (`codegen.rs`)

`generate(item_fn: ItemFn, options: &Options) -> TokenStream` drives the pipeline:

- `strip_args(item_fn)` yields the argument-less function and ordered `Vec<Argument>`, or compiler diagnostic tokens.
- The original declared return type is projected through `<proptest>::test_runner::PropertyReturn::{Success, Failure}`. This preserves result aliases without interpreting their spelling.
- The counterexample type is `(T0, T1, ...)`, including a trailing comma for one argument and `()` for zero arguments.
- The transport error is `Infallible` unless `transport = CodecType => expression` selects `<CodecType as PropertyTransport<Input, Success, Failure>>::Error`.
- `test_body::body` constructs the callback and runner invocation. The wrapper retains the original visibility, signature properties, and attributes, and adds `#[test]`.
- Recoverable option diagnostics are emitted beside the function. They must stay outside the body so `#[test]` stripping cannot swallow them in non-test builds.

No function-local parameter struct or generated `Arbitrary` implementation is needed: callers can name and inspect the returned argument tuple directly.

## The strict-runner body (`test_body.rs`)

`body(...)` assembles the block in this order:

1. Build a tuple of the supplied strategy expressions and `any::<ArgumentType>()` defaults. Zero arguments use `any::<()>()`.
2. Construct `Config` from the explicit expression or `strict_default_config()`, forcing `test_name` and `source_file` while preserving other fields.
3. Build a callback with an explicitly typed tuple pattern and the original return type. It retains original patterns and mutability and calls `PropertyReturn::into_result` without erasing success or failure payloads.
4. Invoke `ensure_property_with_config`, or construct the declared codec once and invoke `ensure_property_with_transport`.
5. Attach argument labels to the successful or failed run, then return the complete outcome.

The context is `concat!(module_path!(), "::", stringify!(function_name))`. Labels use the identifier for plain bindings or the full pattern for destructuring. Labels are diagnostic metadata; they do not replace the native counterexample tuple.

## Proptest path indirection

Every emitted proptest path goes through `options.true_proptest_path()`: `::proptest` by default, or the explicit renamed/re-exported crate path. The override covers the signature, strategy, configuration, callback result projection, runner, and transport trait.

## Snapshot tests & fixtures

`codegen.rs`'s `snapshot_tests` module parses `test_data/*.rs`, calls `generate`, formats through `prettyplease`, and uses `strict_test_support::ensure_snapshot`. The five file fixtures cover simple and multiple arguments, patterns, mixed identifier/pattern arguments, and return types. `renamed_crate_and_transport` additionally checks the crate override, explicit configuration, and codec option together.

The parent `tests/snapshot_tests.rs` covers default, custom, and mixed argument strategies. All expansion artifacts live in `proptest-macro/tests/snapshots/*.snap`. Refresh through `SNAPSHOTS=overwrite cargo test -p proptest-macro`, then review the diff. Runtime integration tests and compiler-diagnostic fixtures in `proptest/tests/` exercise the generated wrappers as real consumers.
