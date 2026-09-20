# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-derive/tests/` — the leaf test suite for `#[derive(Arbitrary)]`: per-feature compile-and-run integration tests (the sibling `*.rs` files) plus a `compile-fail/` UI suite driven by a bespoke `compiletest_rs` harness. Stable tests use `core::convert::Infallible` for uninhabited-type coverage; exact literal-`!` fixtures live in nightly-only compiletest directories and use the current compiler's feature-free syntax. Run full literal-`!` coverage with `cargo +nightly test -p proptest-derive` and again with `--features boxed_union`. See `../AGENTS.md` for the crate and the workspace-root `AGENTS.md` for shared build/lint conventions.

## Two kinds of test here

There are two completely different mechanisms in this directory, and conflating them is the main source of confusion:

- **Top-level `tests/*.rs`** are ordinary Cargo integration-test crates. Cargo compiles and links each one against freshly-built `proptest` + `proptest_derive` the normal way, then runs it. `compiletest.rs` has nothing to do with these.
- **`compile-fail/*.rs`** are *not* compiled by Cargo. The `compiletest.rs` target shells out to raw `rustc` (via `compiletest_rs`) on each file and asserts it *fails* with specific diagnostics. Because Cargo isn't doing the build, `compiletest.rs` has to hand-assemble the `--extern`/`-L`/`--edition` flags itself — which is the entire reason the fingerprint machinery below exists.
- Neither category is a lint or source-hygiene escape hatch. Top-level integration fixtures must use ordinary idiomatic Rust. Compile-diagnostic fixtures are allowed only when the behavior under test depends on full `rustc` integration; if a case is really parser, field-normalization, attribute, or expansion behavior, put it in `proptest-derive-internal` as quoted/parser input or an expansion snapshot instead.

## The custom compiletest harness (`compiletest.rs`)

`compiletest_rs` needs `--extern proptest=<path>` and `--extern proptest_derive=<path>` pointing at the current artifacts (`libproptest-<hash>.rlib` and `<dllprefix>proptest_derive-<hash>.<dllext>`). Cargo can place these in a shared `<profile>/deps/` directory or in isolated `<profile>/build/<package>/<hash>/out/` directories. Multiple builds can coexist in either layout, so the harness selects libraries by matching Cargo fingerprints:

- `CargoArtifacts::current()` derives `CargoLayout` from its executable path and reads `test-integration-test-compiletest.json` to learn the current build's `rustc` and `config` hashes. Shared fingerprints live in `<profile>/.fingerprint/<package>-<hash>/`; isolated fingerprints live in `<profile>/build/<package>/<hash>/fingerprint/`. Discovery preserves custom profile and build-root locations.
- `resolve_artifact()` scans the layout's output directories and reads each candidate's `lib-<lib_name>.json` and adjacent fingerprint hash. A candidate must match the exact dependency fingerprint in the running test binary's `deps` list, the current `rustc` and `config` hashes, and every required feature as an exact token. Cargo renders its `u64` fingerprint as little-endian hexadecimal bytes. Modification time breaks ties only among matching candidates; unreadable or malformed candidates are skipped.
- Required features in `rustc_flags()` reflect the workspace dependency: `proptest` carries `std` and `strict-test`; default features are disabled in the dependency declaration. Exact dependency fingerprints select the active `proptest_derive` build, including its `boxed_union` choice, without relying on stale artifacts from broader builds.
- The resolved paths are emitted as `--extern …` alongside `--edition=2024` and `-L` for every dependency output directory. The selected core library supplies both its `.rlib` and sibling `.rmeta`, because Cargo can omit full metadata from `.rlib` files. Isolated layouts require transitive libraries' output directories too.

When discovery fails, inspect the native path and fingerprint failure before changing build state. Library selection must retain compiler, configuration, and feature matching; selecting an arbitrary recent artifact can produce unrelated compiler diagnostics.

Other harness details worth knowing:

- The fingerprint `features` field is itself a JSON-string-encoded array; `CargoFingerprint::parse` decodes it with `serde_json` and preserves invalid input and native parser failures.
- `path_to_string` rejects non-UTF-8 paths and whitespace because `compiletest` splits its rustc flag string on spaces. Failure retains the original `PathBuf`.
- `compile_test()` queries `${RUSTC:-rustc} --version` and uses that same compiler for the fixtures. It always runs `compile-fail` with `Mode::CompileFail`; on nightly it also runs `compile-fail-nightly` with `Mode::CompileFail` and `run-pass-nightly` with `Mode::RunPass`. Setup failures retain completed suite configurations, compiler-version process output, and the temporary fixture owner.
- The harness materializes fixture trees into an owned temporary directory, preserving auxiliary files and program bytes. A `// revisions: stable nightly` header selects the active compiler's named `compiletest` revision; line endings and diagnostic line positions remain intact. An undeclared compiler revision is a typed setup failure. Fixtures without revision headers are copied unchanged. The temporary directory also owns compiler outputs and is cleaned when the completed outcome is dropped.
- Set the `TESTNAME` env var to filter to a single compile-fail case (`config.filters`).
- Self-tests cover typed fingerprint parsing, malformed input, exact feature tokens, compiler/configuration mismatches, both Cargo layouts, misplaced executable paths, path-to-flag conversion, revision selection, unchanged source trees, auxiliary assets, and invalid source bytes. Running the harness on stable and nightly exercises raw compiler integration against each toolchain's actual artifact layout.

## `compile-fail/` — negative UI cases

`*.rs` files that must fail to compile, with expectations asserted by **inline `compiletest` annotations** — there are **no `.stderr` files** in this tree:

```rust
#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0001]
struct T0<'a>(&'a ());
```

Annotation forms used here: `//~ ERROR: <substr>` expects a diagnostic on that line; `//~| <substr>` adds another expected message to the same group; `//~^ <substr>` (and `//~^^`) bind the expectation to the line(s) above. Each `<substr>` is matched as a substring of the actual compiler output.

Revisioned fixtures use `//[stable]~` and `//[nightly]~` with the same line modifiers. Each revision declares all required diagnostics, including shared errors; unexpected diagnostics remain failures. This keeps downstream trait-solver differences explicit without relaxing span checks or changing compiler behavior.

Before adding or preserving a `compile-fail/*.rs` fixture, prove the failure requires the full compiler boundary: span placement, downstream trait solving, proc-macro diagnostics as rendered by `rustc`, hygiene across crate boundaries, or another behavior unavailable to `syn`/IR/unit/expansion tests. Keep unrelated syntax idiomatic inside the file; the fixture should have one intentional reason to fail, not a pile of tolerated weirdness.

Two distinct flavors of expectation appear:

- **proptest_derive diagnostics** — `[proptest_derive, E####]`. The `E####` code is the literal prefix `mk_err_msg!` stamps onto every derive error, so the codes map one-to-one to the `error!`/`fatal!` definitions in `src/error.rs` (e.g. `E0001` = generic lifetimes, `E0007` = strategy on the wrong shape, `E0034` = malformed regex). The macro batches non-fatal errors, so a single derive can emit `//~ ERROR: 2 errors:` followed by several `//~| [proptest_derive, E####]` lines (see `E0001-lifetime.rs`, `E0007-illegal-strategy.rs`). Filenames are by convention `E####-<slug>.rs`.
- **downstream rustc errors** — some cases assert the *compiler's* own errors rather than a derive code: `must-be-debug.rs` and `no-arbitrary.rs` expect `[E0277]` trait-bound failures (`Debug` / `Arbitrary` not satisfied), `regex_wrong_type.rs` expects `StrategyFromRegex … is not satisfied [E0277]`, and `E0658-no-bare-modifiers.rs` expects `cannot find attribute …` (bare `#[no_params]` outside `#[proptest(...)]` is rejected by rustc, not the derive).

## Conventions in the top-level integration tests

Every `tests/*.rs` follows the same two-part shape, and new cases should match it:

- a `#[test] fn asserting_arbitrary()` containing a local `fn assert_arbitrary<T: Arbitrary>() {}` called once per derived type — a pure compile-time check that the impl and its bounds resolve;
- property tests that execute through `proptest::strict::ensure_property` and preserve native assertion outcomes for the attribute semantics (for example, `value`, `strategy`, `filter`, `regex`, or `weight`), usually via `any_with::<T>(params)` when params are involved. Only terminal tests adapt successful evidence to `()`.

The derive is pulled in as `use proptest_derive::Arbitrary;` in every top-level integration test and revised raw-rustc fixtures. `skip.rs` and `uninhabited-pass.rs` use `core::convert::Infallible` for stable uninhabited coverage; the exact literal-`!` versions live in `run-pass-nightly/`.

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
- **skip.rs** — `#[proptest(skip)]` on enum variants and uninhabited `core::convert::Infallible` variants; exact literal-`!` coverage lives in `run-pass-nightly/skip-never.rs`.
- **regex.rs** — field `#[proptest(regex = …)]` / `regex(…)` / `regex(fn)` for `String`, `Vec<u8>`, and custom `StrategyFromRegex` impls; raw-string forms; combined with `filter`.
- **phantom.rs** — `PhantomData<T>` field detection across import spellings, so the phantom type need not be `Arbitrary`.
- **no_bound.rs** — container `#[proptest(no_bound)]` dropping the generated `Arbitrary` bounds on all type params (per-tyvar `no_bound` is still TODO and commented out).
- **use_tracker.rs** — type-parameter usage tracking: only params used in real (non-`PhantomData`) fields get the `Arbitrary` bound (exercises `src/use_tracking.rs`).
- **assoc.rs** — fields whose types are associated-type projections (`<T as Trait>::Out`, `Tyvar::OutB`, nested projections) still infer correct bounds.
- **uninhabited-pass.rs** — the passing side of uninhabited detection: `core::convert::Infallible`, `[Infallible; N]` with const-expr lengths, and macro-/projection-hidden fields the derive cannot inspect; exact literal-`!` coverage lives in `run-pass-nightly/uninhabited-never.rs`.
- **misc.rs** — grab-bag of container `params` plus variant-level `value`/`strategy`/`no_params`/`params` combinations not covered elsewhere.

For the derive internals these tests exercise (the `error.rs` codes, `use_tracking.rs`, uninhabited detection in `void.rs`), see `../src/AGENTS.md`.
