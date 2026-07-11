# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-derive/tests/` — the leaf test suite for `#[derive(Arbitrary)]`: per-feature compile-and-run integration tests (the sibling `*.rs` files) plus a `compile-fail/` UI suite driven by a bespoke `compiletest_rs` harness. The whole thing **requires nightly** (some cases use `#![feature(never_type)]`); run it both ways — `cargo +nightly test -p proptest-derive` and again with `--features boxed_union`. See `../AGENTS.md` for the crate and the workspace-root `AGENTS.md` for shared build/lint conventions.

## Two kinds of test here

There are two completely different mechanisms in this directory, and conflating them is the main source of confusion:

- **Top-level `tests/*.rs`** are ordinary Cargo integration-test crates. Cargo compiles and links each one against freshly-built `proptest` + `proptest_derive` the normal way, then runs it. `compiletest.rs` has nothing to do with these.
- **`compile-fail/*.rs`** are *not* compiled by Cargo. The `compiletest.rs` target shells out to raw `rustc` (via `compiletest_rs`) on each file and asserts it *fails* with specific diagnostics. Because Cargo isn't doing the build, `compiletest.rs` has to hand-assemble the `--extern`/`-L`/`--edition` flags itself — which is the entire reason the fingerprint machinery below exists.
- Neither category is a lint or source-hygiene escape hatch. Top-level integration fixtures must use ordinary idiomatic Rust. Compile-diagnostic fixtures are allowed only when the behavior under test depends on full `rustc` integration; if a case is really parser, field-normalization, attribute, or expansion behavior, put it in `proptest-derive-internal` as quoted/parser input or an expansion snapshot instead.

## The custom compiletest harness (`compiletest.rs`)

This is **not** the stock `compiletest_rs` setup. `compiletest_rs` needs `--extern proptest=<path>` and `--extern proptest_derive=<path>` pointing at the just-built artifacts, but those live in `target/<profile>/deps/` under hashed filenames (`libproptest-<hash>.rlib`, `<dllprefix>proptest_derive-<hash>.<dllext>` — note the core lib is an **rlib** and the derive macro is a **dylib**). Multiple stale copies with different hashes routinely coexist there, so the harness selects the right one by matching Cargo **fingerprints**:

- `CargoArtifacts::current()` finds its own test binary, walks up to `deps/` and the sibling `.fingerprint/` dir, and reads the compiletest target's own fingerprint (`test-integration-test-compiletest.json`) to learn the current build's `rustc` hash and `config` hash.
- `resolve_artifact()` scans `deps/`, and for each candidate reads `.fingerprint/<package>-<hash>/lib-<lib_name>.json`. A candidate matches only if its `rustc` and `config` hashes equal the current build's **and** its feature set is a superset of the required features. Ties are broken by most-recent mtime.
- Required features are hard-coded per crate in `rustc_flags()`: `proptest` must carry `["bit-set", "default", "fork", "std", "timeout"]` (the workspace default set); `proptest_derive` requires none (`&[]`), so a derive lib built with or without `boxed_union` matches either way.
- The resolved paths are emitted as `--extern …` alongside `-L deps/` and `--edition=2024`.

This fingerprint matching is exactly why stale artifacts produce nonsensical compile-fail results: a mismatched older rlib gets picked or none matches at all. When that happens, `cargo clean` and retry.

Other harness details worth knowing:

- The fingerprint `features` field is itself a JSON-string-encoded array (double-encoded); `parse_feature_set` decodes it with `serde_json`.
- `path_to_str` asserts artifact paths contain no whitespace — `compiletest` splits the rustc flag string on spaces and cannot represent a path with spaces.
- `run_mode("compile-fail", "compile-fail")` is the only mode used; it points `src_base` at `tests/compile-fail`.
- Set the `TESTNAME` env var to filter to a single compile-fail case (`config.filters`).
- Two `#[test]` self-tests guard the fingerprint logic: `fingerprint_parsing_uses_typed_fields` (typed extraction of `rustc`/`config`/`features`) and `fingerprint_feature_matching_uses_exact_tokens` (feature matching is by exact token — `default-code-coverage` does **not** satisfy a required `default`).

## `compile-fail/` — negative UI cases

`*.rs` files that must fail to compile, with expectations asserted by **inline `compiletest` annotations** — there are **no `.stderr` files** in this tree:

```rust
#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0001]
struct T0<'a>(&'a ());
```

Annotation forms used here: `//~ ERROR: <substr>` expects a diagnostic on that line; `//~| <substr>` adds another expected message to the same group; `//~^ <substr>` (and `//~^^`) bind the expectation to the line(s) above. Each `<substr>` is matched as a substring of the actual compiler output.

Before adding or preserving a `compile-fail/*.rs` fixture, prove the failure requires the full compiler boundary: span placement, downstream trait solving, proc-macro diagnostics as rendered by `rustc`, hygiene across crate boundaries, or another behavior unavailable to `syn`/IR/unit/expansion tests. Keep unrelated syntax idiomatic inside the file; the fixture should have one intentional reason to fail, not a pile of tolerated weirdness.

Two distinct flavors of expectation appear:

- **proptest_derive diagnostics** — `[proptest_derive, E####]`. The `E####` code is the literal prefix `mk_err_msg!` stamps onto every derive error, so the codes map one-to-one to the `error!`/`fatal!` definitions in `src/error.rs` (e.g. `E0001` = generic lifetimes, `E0007` = strategy on the wrong shape, `E0034` = malformed regex). The macro batches non-fatal errors, so a single derive can emit `//~ ERROR: 2 errors:` followed by several `//~| [proptest_derive, E####]` lines (see `E0001-lifetime.rs`, `E0007-illegal-strategy.rs`). Filenames are by convention `E####-<slug>.rs`.
- **downstream rustc errors** — some cases assert the *compiler's* own errors rather than a derive code: `must-be-debug.rs` and `no-arbitrary.rs` expect `[E0277]` trait-bound failures (`Debug` / `Arbitrary` not satisfied), `regex_wrong_type.rs` expects `StrategyFromRegex … is not satisfied [E0277]`, and `E0658-no-bare-modifiers.rs` expects `cannot find attribute …` (bare `#[no_params]` outside `#[proptest(...)]` is rejected by rustc, not the derive).

## Conventions in the top-level integration tests

Every `tests/*.rs` follows the same two-part shape, and new cases should match it:

- a `#[test] fn asserting_arbitrary()` containing a local `fn assert_arbitrary<T: Arbitrary>() {}` called once per derived type — a pure compile-time check that the impl and its bounds resolve;
- one or more `proptest! { … }` blocks that actually generate values and `prop_assert!` the attribute semantics (e.g. that a `value`/`strategy`/`filter`/`regex`/`weight` produced what it should), usually via `any_with::<T>(params)` when params are involved.

The derive is pulled in as `use proptest_derive::Arbitrary;` in every top-level integration test (the raw-rustc `compile-fail/` fixtures still use the `#[macro_use] extern crate proptest_derive;` form). `skip.rs` and `uninhabited-pass.rs` carry `#![feature(never_type)]` — the concrete reason the suite needs nightly.

Each file targets one attribute / feature area:

- **struct.rs** — baseline named-field structs of varying arity, no `#[proptest(...)]` attributes.
- **lint_clean.rs** — derives compiled under `deny(warnings)` and `deny(unsafe_code)`, pinning that generated impls are item-scope and allowance-free for the rustc lint surfaces that used to require generated allowances.
- **enum.rs** — enums from 1 to 25 idiomatic unit variants, plus payload-carrying enums; checks variant-count scaling and that every payload is generated. Coverage for non-idiomatic zero-payload variant syntax belongs in `proptest-derive-internal` parser/expansion tests, not in this broad runtime fixture.
- **units.rs** — degenerate empty shapes: unit struct `T0;`, empty `T1 {}` / `T2()`, and idiomatic unit enum variants. Coverage for empty tuple/struct enum variant syntax belongs in `proptest-derive-internal` parser/expansion tests.
- **value.rs** — field `#[proptest(value = …)]` / `value(…)`: literals, expressions, and `fn`-path calls yielding a constant field.
- **value_param.rs** — `value` combined with `params`, where the value expression reads `params`; driven by `any_with`.
- **strategy.rs** — field `#[proptest(strategy = …)]` / `strategy(…)` / `strategy(fn)` on structs and enum variants.
- **params.rs** — container/field `#[proptest(params(T))]` (and `params = "T"`), `no_params`, strategies referencing `params`, and per-field "parallel" params; driven by `any_with`.
- **filter.rs** — `#[proptest(filter(…))]` at container, variant, and field level in every spelling (closure string, `fn` path, `= "…"`), multiple filters, and combinations with strategy/value/params.
- **weight.rs** — enum-variant `#[proptest(weight = N)]` / `weight(N)` in both string- and integer-literal forms.
- **skip.rs** — `#[proptest(skip)]` on enum variants and uninhabited `!` variants (needs `never_type`).
- **regex.rs** — field `#[proptest(regex = …)]` / `regex(…)` / `regex(fn)` for `String`, `Vec<u8>`, and custom `StrategyFromRegex` impls; raw-string forms; combined with `filter`.
- **phantom.rs** — `PhantomData<T>` field detection across import spellings, so the phantom type need not be `Arbitrary`.
- **no_bound.rs** — container `#[proptest(no_bound)]` dropping the generated `Arbitrary` bounds on all type params (per-tyvar `no_bound` is still TODO and commented out).
- **use_tracker.rs** — type-parameter usage tracking: only params used in real (non-`PhantomData`) fields get the `Arbitrary` bound (exercises `src/use_tracking.rs`).
- **assoc.rs** — fields whose types are associated-type projections (`<T as Trait>::Out`, `Tyvar::OutB`, nested projections) still infer correct bounds.
- **uninhabited-pass.rs** — the passing side of uninhabited detection: `!`, `[!; N]` with const-expr lengths, and macro-/projection-hidden fields the derive cannot inspect (needs `never_type`).
- **misc.rs** — grab-bag of container `params` plus variant-level `value`/`strategy`/`no_params`/`params` combinations not covered elsewhere.

For the derive internals these tests exercise (the `error.rs` codes, `use_tracking.rs`, uninhabited detection in `void.rs`), see `../src/AGENTS.md`.
