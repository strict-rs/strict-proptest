## Unreleased

### Breaking Changes

- The minimum supported Rust version has been increased to 1.96.0.
- `#[property_test]` now preserves `Result<A, E>` bodies and aliases. The generated wrapper returns `PropertyResult<(ArgumentTypes, ...), A, E>` with a concrete counterexample tuple and argument labels, retaining native assertion evidence and failures. Unit-returning bodies produce a targeted compile error. Strict defaults select deterministic seeds and disable regression-file persistence.
- `config = <expr>` now routes through `proptest::strict::ensure_property_with_config` with `test_name` and `source_file` forced over the given expression; without `config`, the strict defaults apply.

### Bug Fixes

- An invalid `proptest_path = ...` value now surfaces as its intended compile error instead of a proc-macro panic: recoverable option errors are emitted as statement-form `compile_error!` tokens at item position, so the diagnostics also survive builds that cfg-strip the generated `#[test]` fn (rustdoc, trybuild). The diagnostic is attached to the full invalid value expression.
- An internal code-generation parse failure now falls back to a `compile_error!` diagnostic instead of panicking the proc macro.

### New Features

- Added `transport = CodecType => codec_expression` for explicit typed fork execution. The wrapper preserves the concrete codec error type, constructs the codec once, and resolves its runner and signature through the selected crate path.

- Added support for `proptest_path = ::path::to::proptest` on `#[property_test]`, allowing the macro to target a re-exported `proptest` crate; the strict module is resolved through that path (`<proptest_path>::strict::...`), never hard-coded.

### Other Notes

- Added crate README metadata for package metadata lint coverage.
- The crate's one elided lifetime is now written explicitly (`ParseStream<'_>`); no generated-code changes.

## 0.5.0

### Breaking Changes

- The minimum supported Rust version has been increased to 1.84.0. ([\#612](https://github.com/proptest-rs/proptest/pull/612))

### New Features

- Set `Config::test_name` to the actual function name in the `proptest!` macro. ([\#619](https://github.com/proptest-rs/proptest/pull/619))
- Support returning `TestCaseResult` from `#[property_test]` tests. ([\#622](https://github.com/proptest-rs/proptest/pull/622))

### Bug Fixes

- Fixes and improvements to the `proptest!` macro implementation and code generation. ([\#622](https://github.com/proptest-rs/proptest/pull/622))

## 0.4.0

### Breaking Changes

- The minimum supported Rust version has been increased to 1.82.0. ([\#605](https://github.com/proptest-rs/proptest/pull/605))

## 0.3.1

### Bug Fixes

- Fix attr macro incorrectly eating mutability modifiers. ([\#602](https://github.com/proptest-rs/proptest/pull/602))

## 0.3.0

### New Features

- Update attr macro to use argument names where trivial, preserving better debugging experience. ([\#594](https://github.com/proptest-rs/proptest/pull/594))

### Bug Fixes

- Fix shorthand struct initialization lint.

## 0.2.0

### Other Notes

- Updated `rand` dependency from 0.8 to 0.9.
- Bump all dependencies to latest compatible with MSRV 1.66.

## 0.1.0

Initial release, an MVP of a #[proptest] attribute macro
