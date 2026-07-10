## Unreleased

### Breaking Changes

- The minimum supported Rust version has been increased to 1.96.0.

### Bug Fixes

- The derived code no longer stamps the field's span onto its qualified `_proptest::arbitrary::...` helper paths, so consuming crates that enable the `unused_qualifications` lint no longer get false "unnecessary qualification" warnings attributed to their own fields. A missing `Arbitrary` impl still reports an error pointing at the offending field (the interpolated field type keeps its span); the impl-level duplicate of that error now attributes to the derive attribute instead of the field.

### Other Notes

- Split the derive implementation into an internal `proptest-derive-internal` library crate; `proptest-derive` is now a thin proc-macro shim that converts tokens and delegates. No change to the `#[derive(Arbitrary)]` API, the generated code, or the diagnostics — the split lets the pipeline be exercised (and documented) as ordinary library code, and the parse step is now panic-free (a malformed token stream surfaces as a `compile_error!` instead of unwinding).
- The `value` codegen now produces its `fn() -> T` strategy through a typed `let` coercion (`{ let value_fn: fn() -> _ = || <expr>; value_fn }`) instead of an `as fn() -> _` cast; behavior is identical, and a user item named `value_fn` referenced from the pinned expression still resolves to the user's item (pinned by a regression test).
- Replaced the crate's `#[macro_use] extern crate syn;` / `#[macro_use] extern crate quote;` globs with per-module `use` imports of `quote!`, `quote_spanned!`, `parse_quote!`, and `Token!`, and spelled out the anonymous lifetime (`Ctx<'_>`) across the internal derive pipeline. No generated-code or diagnostic changes.

## 0.8.0

### Breaking Changes

- The minimum supported Rust version has been increased to 1.84.0. ([\#612](https://github.com/proptest-rs/proptest/pull/612))

## 0.7.0

### Breaking Changes

- The minimum supported Rust version has been increased to 1.82.0. ([\#605](https://github.com/proptest-rs/proptest/pull/605))

## 0.6.0

### Other Notes

- Fixed URLs in proptest-derive error messages ([\#574](https://github.com/proptest-rs/proptest/pull/574))
- Updated `rand` dependency from 0.8 to 0.9.
- Bump all dependencies to latest compatible with MSRV 1.66.

## 0.5.1

- Fix non-local impl nightly warning with allow(non_local_definitions)
  ([\#531](https://github.com/proptest-rs/proptest/pull/531))
- Adds support for re-exporting crate. `proptest-derive` now works correctly
  when `proptest` is re-exported from another crate. This removes the
  requirement for `proptest` to be a direct dependency.
  ([\#530](https://github.com/proptest-rs/proptest/pull/530))
- Fix bounds generation for generics in derive(Arbitrary). The implementation
  of UseTracker expects that iteration over items of used_map gives items in
  insertion order. However, the order of BTreeSet is based on Ord, not
  insertion. ([\#511](https://github.com/proptest-rs/proptest/pull/511))

## 0.5

### Features

- Add `boxed_union` feature which when turned on uses heap allocation for
  `#[derive(Arbitrary)]` strategy synthesis preventing stack overflow for
  exceptionally large structures.

### Dependencies

- Upgraded `syn` to 2.x
- Upgraded `compiletest_rs` 0.10 to 0.11

### Other Notes

- Fixed various clippies and diagnostic issues

### 0.4.0

### Other Notes

- Upgraded `compiletest_rs` from 0.9 to 0.10
- Upgraded `syn`, `quote`, and `proc-macro2` to 1.0

## 0.3.0

### Breaking changes

- The minimum supported Rust version has been increased to 1.50.0.

### Bug Fixes

- Certain `enum`s could not be derived before, and now can be.

- Structs with more than 10 fields can now be derived.

## 0.2.0

### Breaking changes

- Generated code now requires `proptest` 0.10.0.

## 0.1.2

### Other Notes

- Derived enums now use `LazyTupleUnion` instead of `TupleUnion` for better
  efficiency.

## 0.1.1

This is a minor release to correct a packaging error. The license files are now
included in the files published to crates.io.
