# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/tests/` — the core crate's integration tests. (The bulk of the test suite is inline `#[cfg(test)]` modules inside `src/`; only the items below live out here.) For shared conventions see the workspace-root `AGENTS.md`.

Everything in this directory tests the `#[property_test]` attribute macro from the *consumer* side — the macro is re-exported as `proptest::property_test` only under the `attr-macro` feature, so all of it is feature-gated. The macro's own token expansion is snapshot-tested separately over in `proptest-macro/`; here we check that real annotated functions compile and behave.

## Targets

### `attr_macro.rs` — the only true integration-test target

A single runtime property test, `attr_macro_does_not_clobber_mutability`, gated `#[cfg(feature = "attr-macro")]`. It is the regression for issue #601: it applies `#[proptest::property_test]` to `fn …(mut x: i32, (mut y, _z): (i32, i32))` and then reassigns `x = 0; y = 0;` in the body, proving the macro preserves `mut` both on a plain ident argument and on an ident nested inside a tuple-destructuring pattern argument when it rewrites the signature into a generated params struct. Unlike the `pass/` fixtures, this one actually *runs* the generated runner — it is a behavior check, not a compile-only check.

```sh
cargo test -p proptest --test attr_macro --features attr-macro
```

Gotcha: without `--features attr-macro` the test is `#[cfg]`'d out and the target builds to an empty, trivially-passing binary — so running this target without the feature is a false green.

### `pass/` — trybuild compile-pass fixtures

`hygiene.rs`, `simple_example.rs`, and `with_params.rs` are *not* their own test target. They are compiled by `trybuild` (a `[dev-dependencies]` entry) from the `compile_tests()` unit test in `src/lib.rs`, which is itself `#[cfg(feature = "attr-macro")]` and does only `trybuild::TestCases::new().pass("tests/pass/*.rs")` — pass-only, with no `compile_fail`/`.stderr` expectation files in this tree. Because that test lives in the lib (not here), `--test attr_macro` does *not* run it; run it through the lib unit tests:

```sh
cargo test -p proptest --features attr-macro --lib compile_tests
# or just: cargo test -p proptest --features attr-macro   (runs this and attr_macro.rs)
```

trybuild builds each fixture as a standalone binary, so every one carries its own `fn main() {}` and fully-qualifies the macro as `proptest::property_test`. The `tests/pass/*.rs` glob auto-discovers them, so a new fixture needs no registration — just add the file. What each validates:

- `simple_example.rs` — the minimal form: `#[property_test] fn my_test(x: i32)` with a default `Arbitrary`-derived strategy and a trivial body. The baseline "happy path still compiles" check.
- `hygiene.rs` — defines a module-level `struct MyTestArgs` that collides *by name* with the macro's generated params struct (the macro derives the struct name from the fn name: `my_test` → `MyTestArgs`, i.e. PascalCase + `Args`). It compiles because the macro emits its struct and `Arbitrary` impl *inside* the rewritten function body, not at module scope — this fixture guards that scoping so the generated name never clashes with a user item.
- `with_params.rs` — the `config = proptest::test_runner::Config { cases: 10, .. }` attribute option, exercised against both a non-trailing-comma argument list (`no_trailing_comma(x: i32)`) and a trailing-comma one (`trailing_comma(x: i32,)`).

## Note

Failure-persistence behavior (where regression files get written) is tested separately by `proptest/test-persistence-location/run-tests.sh`, which is **not** part of `cargo test` — see the root `AGENTS.md`.
