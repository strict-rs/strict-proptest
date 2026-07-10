# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-derive/src/` — the proc-macro shim. The directory holds a single file, `lib.rs`, and no implementation logic: the `#[derive(Arbitrary)]` pipeline lives in the sibling `proptest-derive-internal` crate (see its `AGENTS.md` and `src/AGENTS.md`). For this crate's layout, features, and test commands see the crate `AGENTS.md` one directory up; for workspace-wide conventions see the workspace-root `AGENTS.md`.

## The shim (`lib.rs`)

`#[proc_macro_derive(Arbitrary, attributes(proptest))] pub fn derive_proptest_arbitrary(input: TokenStream) -> TokenStream` is the crate's entire surface. Its body is one delegation — `proptest_derive_internal::derive_arbitrary(input.into()).into()` — converting `proc_macro::TokenStream` to `proc_macro2::TokenStream` at the boundary and back. There is no panic path here: parsing (and parse-failure handling via `compile_error!`) happens inside the internal crate's `derive_arbitrary`.

A proc-macro crate can export nothing but its macro entry points (`pub mod` is a hard compile error in this crate type), which is why the pipeline lives in the internal library crate where the workspace visibility lints can be satisfied with real module boundaries. Keep this file to the attribute, the crate docs, and the delegation; new derive behavior always goes to `proptest-derive-internal`.
