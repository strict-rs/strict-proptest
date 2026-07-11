# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/tests/` — the core crate's integration tests. (The bulk of the test suite is inline `#[cfg(test)]` modules inside `src/`; only the items below live out here.) For shared conventions see the workspace-root `AGENTS.md`.

This directory holds consumer-side integration and compile fixtures. The `attr_macro.rs`, `pass/`, and `fail/` paths test the `#[property_test]` attribute macro, which is re-exported as `proptest::property_test` only under the `attr-macro` feature. The `sugar/` paths test exported declarative macros that are available from the core crate without `attr-macro`. The attribute macro's own token expansion is snapshot-tested separately over in `proptest-macro/`; here we check that real consumer code compiles and behaves.

## Targets

### `attr_macro.rs` — the only true integration-test target

Runtime behavior checks, gated by a file-level `#![cfg(feature = "attr-macro")]`. Every `#[property_test]` body here returns `proptest::strict::TestResult` and uses the `strict_test_support` `ensure*` helpers (a `[dev-dependencies]` entry) — a `()` body no longer compiles. Unlike the `pass/` fixtures, these actually *run* the generated strict runner:

- `attr_macro_does_not_clobber_mutability` — the regression for issue #601: applies `#[proptest::property_test]` to `fn …(mut x: i32, (mut y, _z): (i32, i32))` and reassigns `x = 0; y = 0;` in the body, proving the macro preserves `mut` both on a plain ident argument and on an ident nested inside a tuple-destructuring pattern argument.
- `falsifying_wrapper_surfaces_test_failure` — an `#[ignore]`d, deliberately falsifying `#[property_test]` fixture (the macro preserves pre-existing attributes, so the harness never runs it as a failing test).
- `generated_wrapper_returns_test_failure_instead_of_panicking` — the negative-polarity proof: calls the ignored fixture's generated wrapper *directly* (it is a plain `fn() -> TestResult`), and checks the returned `Err` renders "property falsified" with the shrunk minimal counterexample (`x: 1` under the deterministic default seed). The call returning at all proves the wrapper propagates `TestFailure` rather than panicking.

```sh
cargo test -p proptest --test attr_macro --features attr-macro
```

Gotcha: without `--features attr-macro` the tests are `#[cfg]`'d out and the target builds to an empty, trivially-passing binary — so running this target without the feature is a false green.

### `pass/` and `fail/` — trybuild fixtures

The fixtures are *not* their own test target. They are compiled by `trybuild` (a `[dev-dependencies]` entry) from the `compile_tests()` unit test in `src/lib.rs`, which is `#[cfg(feature = "attr-macro")]` and runs `t.pass("tests/pass/*.rs")` + `t.compile_fail("tests/fail/*.rs")` through an explicit `t.run()` (the trybuild fork has no drop-driven execution — without the explicit call nothing runs). Because that test lives in the lib (not here), `--test attr_macro` does *not* run it; run it through the lib unit tests:

```sh
cargo test -p proptest --features attr-macro --lib compile_tests
# or just: cargo test -p proptest --features attr-macro   (runs this and attr_macro.rs)
```

trybuild builds each fixture as a standalone binary, so every one carries its own `fn main() {}` and fully-qualifies the macro as `proptest::property_test`. The globs auto-discover fixtures, so a new one needs no registration — just add the file (a `fail/` fixture also needs its `.stderr` expectation; regenerate with `TRYBUILD=overwrite`, then review the diff). Fixture bodies can use `strict_test_support` directly — trybuild propagates the host crate's dev-dependencies into fixture builds.

Compile-pass fixtures (`pass/`), all with result-returning bodies:

- `simple_example.rs` — the minimal form: default `Arbitrary`-derived strategy, body ending in an `ensure_eq`. The baseline "happy path still compiles" check.
- `hygiene.rs` — defines a module-level `struct MyTestArgs` that collides *by name* with the macro's generated params struct (fn `my_test` → `MyTestArgs`, PascalCase + `Args`). It compiles because the macro emits its struct and `Arbitrary` impl *inside* the rewritten function body, not at module scope.
- `with_params.rs` — the `config = proptest::test_runner::Config { cases: 10, .. }` attribute option (routed through `ensure_property_with_config`), against both non-trailing-comma and trailing-comma argument lists.
- `custom_strategy.rs` — a `#[strategy = "[0-9]{1,8}"]` regex override on an argument.
- `custom_proptest_path.rs` — `extern crate proptest as aliased_proptest;` + `proptest_path = ::aliased_proptest`, with the return type spelled through the alias — proves the generated code reaches the strict module through the configured path, never a hard-coded `::proptest`.
- `prop_oneof_general_arm.rs` — an eleven-alternative `prop_oneof!` (the vec-backed general arm, whose expansion names `$crate::std_facade::vec!`) built and driven from `main()`: it samples the union deterministically and checks every alternative is generated, then runs the strategy through `ensure_property` — pinning that the facade macro path resolves from an external crate.

Compile-fail fixtures (`fail/`), each pinning a diagnostic in its `.stderr`:

- `unit_body.rs` — a `()` property body → the strict return-type rejection ("strict property tests must return `Result<(), TestFailure>` …").
- `explicit_unit_return.rs` — a literal `-> ()` → the same rejection, spanned on the return type.
- `invalid_proptest_path.rs` — `proptest_path = actually::a::function()` → the options diagnostic ("argument to `proptest_path` must be a path to the proptest crate, …").

### `sugar/` — always-on trybuild fixtures

The `sugar/pass/` and `sugar/fail/` fixtures are compiled by the always-on `sugar_macro_compile_tests()` unit test in `src/lib.rs`. Run them through the ordinary core lib tests:

```sh
cargo test -p proptest sugar_macro_compile_tests
```

The pass fixtures build and execute strategies from an external-crate perspective:

- `prop_compose_ffi_one_layer.rs` — a one-layer `prop_compose_ffi!` builder with a scalar C-ABI mapper; `main()` draws a deterministic sample and checks the mapper received both the generated value and the builder argument.
- `prop_compose_ffi_two_layer.rs` — a two-layer `prop_compose_ffi!` builder where the second layer depends on the first; `main()` draws a deterministic sample and checks the mapper received the dependent scalar values.
- `prop_compose_ffi_typed_arguments.rs` — a two-layer `prop_compose_ffi!` builder using typed strategy arguments; `main()` draws a deterministic sample and checks the mapper received generated values plus the builder argument.

The fail fixtures pin the negative macro contract:

- `prop_compose_rejects_bracketed_modifier.rs` — `prop_compose! { [extern "C"] fn ... }` fails with the targeted diagnostic directing users to `prop_compose_ffi!`.
- `prop_compose_ffi_rejects_rust_abi.rs` — a `prop_compose_ffi!` mapper with a Rust-only `Vec<i32>` parameter under `#![deny(improper_ctypes_definitions)]` fails because the mapper is a real `extern "C"` item.

### `prelude/` — always-on trybuild fixtures

The `prelude/pass/` and `prelude/fail/` fixtures are compiled by the always-on `prelude_compile_tests()` unit test in `src/lib.rs`. Run them through the ordinary core lib tests:

```sh
cargo test -p proptest prelude_compile_tests
```

The pass fixture pins the current rand prelude surface:

- `rng_traits.rs` — `prelude::*` brings both `Rng` and `RngExt` into scope, so a consumer can name the `Rng` trait and call `RngExt` methods on the runner RNG.

The fail fixture pins the removed compatibility surface:

- `rng_core_removed.rs` — `prelude::*` no longer provides the deprecated `RngCore` re-export.

## Note

Failure-persistence behavior (where regression files get written) is tested separately by `proptest/test-persistence-location/run-tests.sh`, which is **not** part of `cargo test` — see the root `AGENTS.md`.
