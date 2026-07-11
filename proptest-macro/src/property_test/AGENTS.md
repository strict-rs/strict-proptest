# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-macro/src/property_test/` — the parse / validate / options front end of the `#[property_test]` attribute macro (code generation lives in `codegen/`). For build/test commands and crate dependencies see the parent `proptest-macro/AGENTS.md`; for workspace-wide conventions see the workspace-root `AGENTS.md`.

## Pipeline (`property_test.rs`)

`property_test(attr, item) -> TokenStream` is the module entry and a four-step pipeline:

1. `parse!(item)` into a `syn::ItemFn` — the annotated test fn, bound `mut` so `validate` can rewrite it.
2. `parse!(attr)` into `Options` — the attribute body, e.g. `config = ...` (see `options.rs`).
3. `validate(&mut item_fn)` — on `Err` the accumulated `compile_error!` tokens are returned verbatim.
4. `codegen::generate(item_fn, options)` — produces and returns the rewritten `#[test]` fn.

The local `parse!` macro is the diagnostic backbone: on a `parse2` failure it `return`s `e.into_compile_error()` (a `compile_error!` token stream) instead of panicking, so a malformed signature or attribute still surfaces as a normal, spanned compiler error at the use site rather than a macro panic. Each `parse!`'s target type is inferred from later use — `ItemFn` from `validate`, `Options` from `generate`.

`lib.rs` (one level up) holds the actual `#[proc_macro_attribute]` shim plus all user-facing rustdoc/examples; it only `.into()`-converts `proc_macro` <-> `proc_macro2` token streams and delegates here. Its doc comment is the semver contract — the synthesized struct's name, fields, and even existence are implementation details, which is why `codegen` is free to rename it.

## Validation (`validate.rs`)

`validate(f: &mut ItemFn) -> Result<(), TokenStream>` deliberately checks little: the guiding principle (stated in its doc comment) is to **defer to rustc** wherever rustc already emits a good error, passing the offending syntax straight through to the generated fn. Three checks run here:

- `all_args_non_self` — rejects any `FnArg::Receiver` with "`self` parameters are forbidden" (covers `self`, `&self`, `&mut self`, and `self: T` forms; see the `validate_fails_with_self_arg` test).
- `validate_parameter_attrs` — permits only outer `#[strategy = <expr>]` attributes on parameters. Any other attribute, an inner `#![...]`, or a malformed `#[strategy(...)]` / bare `#[strategy]` (all of which `is_strategy` rejects) gets a "only `#[strategy = <expr>]` attributes are allowed here" error; a second `#[strategy = ...]` on one param gets a duplicate error. As a side effect it rewrites each param's `attrs` down to at most the one valid strategy attr, so `strip_args` later sees a clean list.
- `returns_strict_result` — rejects a missing return type (`ReturnType::Default`) and a literal `-> ()` with the stable `UNIT_RETURN_ERROR` message pointing the author at `Result<(), TestFailure>` (`proptest::strict::TestResult`) and an `Ok(())` body ending. The check is **syntactic** (a `type Foo = ()` alias is not caught — that case falls through to rustc's type error at the generated `ensure_property` call); it deviates from defer-to-rustc deliberately, because the post-expansion inference error would not name the actual contract. Runs after the other checks so their diagnostics keep priority.

Everything else about the signature — generics, where-clauses, `async` / `const`, `unsafe`, argument patterns/types — is intentionally **not** inspected. `unsafe` is the canonical example: it is passed through verbatim so rustc emits its own "test fn cannot be unsafe" error rather than this macro re-implementing the diagnostic.

Errors are **accumulated, not bailed-on**: `validate_parameter_attrs` builds a single `TokenStream` of every `compile_error!` across all params and returns it only at the end, so one invocation can report multiple problems at once. (`all_args_non_self` does still short-circuit on the first receiver.) The shared `err()` helper emits statement-form `compile_error!("...");` tokens — the trailing semicolon matters, because the tokens are spliced at statement/item position.

## Options (`options.rs`)

`Options` (derives `Default`) is the parsed attribute body, with three fields:

- `config: Option<Expr>` — from `config = <expr>`; when present, codegen routes through `<proptest>::strict::ensure_property_with_config` with `test_name`/`source_file` forced over the user's expression. When absent, codegen calls plain `ensure_property`, which applies the strict defaults (deterministic `STRICT_TEST_SEED` seeding, persistence off, `PROPTEST_*` env passthrough).
- `proptest_path: Option<Path>` — from `proptest_path = <path>`, the path to the proptest crate (for a re-exported or renamed proptest). The value must be a plain `Expr::Path` with no qself; otherwise a `compile_error!` describing the expected form is recorded.
- `errors: Vec<TokenStream>` — accumulated, *recoverable* parse errors.

`true_proptest_path() -> TokenStream` resolves the path codegen prefixes onto every emitted item: `::proptest` when `proptest_path` is unset, else the user's path.

The `Parse` impl reads the attribute *contents* (`foo = bar, baz = qux`, not the wrapping `#[...]`) as `Punctuated<MetaNameValue, ,>`. Crucially it almost never returns `syn::Err`: an unknown key (`random = 123`) or a bad `proptest_path` value is pushed onto `errors` and parsing continues, returning `Ok` with whatever was understood (see the `simple_parse_example` and `invalid_proptest_path` tests). Every pushed token stream is a full `compile_error!("...");` **statement** (trailing semicolon), and codegen's `generate()` splats `#(#errors)*` at **item position** beside the generated fn — not inside its body, where the `#[test]` cfg-strip would silently swallow the diagnostic in non-test builds (trybuild, rustdoc). Only a syntactic failure of the `MetaNameValue` list itself yields `Err`.

## Helpers (`utils.rs`)

The bridge from a validated fn to codegen:

- `Argument { pat_ty: PatType, strategy: Option<Expr> }` — one parameter plus its optional `#[strategy = expr]` override.
- `strip_args(f: ItemFn) -> (ItemFn, Vec<Argument>)` — `mem::take`s the inputs to yield an argument-less `ItemFn` (codegen reuses its name, return type, and body) alongside the per-arg `Vec<Argument>`. It **panics** on receivers or malformed strategy attrs — rejecting those is `validate`'s job, so reaching them is a bug, not user error.
- `strip_strategy` — pulls the single strategy attr off a `PatType`, leaving the param's remaining attributes attached.
- `is_strategy(attr) -> bool` — the shared predicate used by both `validate` and `strip_strategy`: true only for an *outer* attribute whose path is exactly `strategy` and whose meta is `NameValue`. It is what makes `#![strategy = ...]`, `#[strategy(...)]`, and bare `#[strategy]` count as "not a strategy" (and thus get rejected by validate).

The per-argument override threads across three files: `validate` permits/strips it, `utils` extracts it into `Argument.strategy`, and `codegen` consumes it as that argument's custom strategy. The struct- and field-*naming* helpers live in `codegen/`, not here.

## Code generation (`codegen/`)

`codegen::generate(item_fn, options)` turns the validated, argument-less fn plus its `Vec<Argument>` into a `#[test]` fn returning `<proptest>::strict::TestResult` — a params struct, its `Arbitrary` impl, and a tail call into `<proptest>::strict::ensure_property` (or `ensure_property_with_config` under `config = …`). See `codegen/AGENTS.md` for the emitted-code shape; don't duplicate that detail here.

## Tests (`tests/`)

`tests/snapshot_tests.rs` (declared by `tests.rs`) is an `insta` snapshot suite. Its `snapshot_test!` macro `parse_quote!`s an inline fn, runs it through `codegen::generate(input, Options::default())`, formats the output with `prettyplease::unparse`, and `insta::assert_snapshot!`s it; expansions land in `tests/snapshots/*.snap`. Note it drives the **codegen stage directly with default options** — it bypasses `property_test.rs`'s `validate` and option parsing. Three cases: `basic_derive_example` (no `#[strategy]` overrides), plus `custom_strategy` and `mix_custom_and_default_strategies` (both use `#[strategy = ...]`); the latter two are the snapshot coverage of the custom-strategy expansion that `codegen/`'s own `test_data` fixtures don't exercise.

Run `cargo test -p proptest-macro`, then `cargo insta review` to inspect/accept changed snapshots (review deliberately, don't blind-accept). This suite is distinct from the `snapshot_tests` module *inside* `codegen/` (driven by its own `test_data/*.rs` fixtures) — see `codegen/AGENTS.md`.
