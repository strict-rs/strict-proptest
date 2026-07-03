# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-macro/src/property_test/codegen/` — the code-generation stage of `#[property_test]`, invoked by the parent `property_test` module (after `validate`) to rewrite a validated test fn into a plain `#[test]` that runs a proptest. It emits what a hand-written `proptest! { #[test] fn ...(x in any::<T>()) { ... } }` block would. `<proptest>` below stands for the configured crate path (see "Proptest path indirection"). For shared conventions and the crate-wide snapshot workflow see the workspace-root and `proptest-macro/` `AGENTS.md`s.

## Entry point: `generate()` (`mod.rs`)

`generate(item_fn: ItemFn, options: Options) -> TokenStream` is the only `pub(super)` item and drives the whole pipeline:

- `strip_args(item_fn)` (parent `utils.rs`) splits the fn into an argument-less `ItemFn` plus a `Vec<Argument>`; each `Argument` holds a `PatType` and an optional `#[strategy = expr]` override (`Option<Expr>`).
- `generate_struct(ident, args)` synthesizes the params struct; `arbitrary::gen_arbitrary_impl(...)` builds its `Arbitrary` impl; the two are concatenated into `struct_and_arb`.
- `test_body::body(...)` wraps the original block in the runner glue and replaces `argless_fn.block`.
- `test_attr()` pushes `#[test]`, then `argless_fn.sig.output` is reset to `ReturnType::Default` — the generated wrapper always returns `()`, even when the source fn was `-> TestCaseResult` (the runner consumes the result internally).

`strip_args` panics on receivers / malformed fns; those cases are meant to be rejected earlier by the parent `validate.rs`, so they should never reach codegen.

## The synthesized struct (`generate_struct`, `struct_name`, `nth_field_name`)

Emits `#[derive(Debug)] struct <Name>Args { <field>: <ty>, ... }` — one field per argument, in order. `Debug` is required because the runner prints failing inputs.

- `struct_name` PascalCases the fn name (via `convert_case`) and appends `Args`: `some_function` -> `SomeFunctionArgs`.
- `nth_field_name` names each field: if the arg's pattern is a plain `Pat::Ident`, that ident is reused verbatim; otherwise the field is `arg<n>` where `<n>` is the *positional* index counting all args (so in `fn foo(a, (b,c), d, ...)` the tuple field is `arg1`, not `arg0` — see the `arg_ident_and_pattern` fixture/snapshot).

## The `Arbitrary` impl — unboxed vs boxed (`arbitrary.rs`)

`gen_arbitrary_impl` picks a path by inspecting `args`:

- If every arg has `strategy.is_none()` -> `no_custom_strategies` (unboxed). Because the concrete types are written in the signature, it can name the strategy type: `type Strategy = <proptest>::strategy::Map<<proptest>::arbitrary::StrategyFor<(T0, T1, ...)>, fn((T0, ...)) -> Self>`, and the value is `any::<(T0, ...)>().prop_map(|(n0, ...)| Self { n0, ... })`.
- If any arg carries `#[strategy = expr]` -> `custom_strategies` (boxed). The override expr supplies a strategy but not its return type, so the impl falls back to `type Strategy = <proptest>::strategy::BoxedStrategy<Self>`. It builds a tuple mixing each arg's override `expr` with `any::<Ty>()` for the rest (the fallback is emitted via `quote_spanned!` at the arg type's span for better error locations), then `.prop_map(...).boxed()`.

`arbitrary_shared` emits the common `impl Arbitrary` shell shared by both paths; `type Parameters = ()` and `arbitrary_with((): Self::Parameters)` are fixed.

NOTE: the in-directory `test_data` fixtures only exercise the unboxed path — none here use `#[strategy = ...]`, so the boxed branch is not snapshot-guarded from this module.

## The runner body (`test_body.rs`)

`body(...)` assembles the new block in this order:

1. `#(#errors)*` — the deferred `compile_error!` tokens collected in `Options::errors` (recoverable option-parse errors, emitted at the use site rather than aborting earlier).
2. The struct + `Arbitrary` impl.
3. `let config = <proptest>::test_runner::Config { test_name: Some(concat!(module_path!(), "::", stringify!(fn))), source_file: Some(file!()), ..<trailing> };` where `make_config` sets `<trailing>` to `Config::default()` or the user's `config = <expr>`. `test_name`/`source_file` are always forced; everything else splats from `<trailing>`.
4. `let mut runner = <proptest>::test_runner::TestRunner::new(config);`
5. `runner.run(&any::<Args>().prop_map(|values| NamedArguments(stringify!(Args), values)), |NamedArguments(_, <struct_pattern>)| { let result = #block; #handle_result })` — `NamedArguments` (`<proptest>::sugar`) labels the input for failure messages, as `proptest!` does.
6. `match result { Ok(()) => {} Err(e) => panic!("{}", e) }`.

`<struct_pattern>` destructures `<Name>Args { ... }` back to the user's original bindings so the body sees its parameter names again. Ident fields use shorthand (`x,` / `mut x,`, preserving mutability) to avoid the redundant `x: x` lint — see issue #601 referenced in the source; non-ident fields re-attach the original pattern (`arg1: (b, c),`).

`handle_result(ret_ty)` is the unit-vs-result heuristic: a missing return type or a literal `-> ()` yields `Ok(result)` (the body returns `()`, so wrap it into an `Ok`); any other return type yields bare `result` (the body already evaluates to a `TestCaseResult`, e.g. from `prop_assert!`). The check is purely *syntactic* — a `type Foo = ()` alias is not recognized, only literal `()` / no return (see the `return_value` fixture for the result-style expansion).

## Proptest path indirection

Every emitted path is prefixed with `options.true_proptest_path()` (parent `options.rs`): `::proptest` by default, or the user's `proptest_path = <path>`. This is why the snapshots render fully-qualified paths, and why the `with_options::simple` snapshot renders `::hello::world::...`.

## Snapshot tests & fixtures

Two `#[cfg(test)]` modules live at the bottom of `mod.rs`:

- `tests` — plain unit tests for `generate_struct` (`generates_correct_struct`, `derives_debug`) plus `generates_arbitrary_impl`, which snapshots `gen_arbitrary_impl(...).to_string()` as *raw* (unformatted) tokens.
- `snapshot_tests` — the `snapshot_test!` macro `include_str!`s `test_data/<name>.rs`, runs `generate()`, formats with `prettyplease::unparse`, and `insta::assert_snapshot!`s the result. Cases: `simple`, `many_params`, `arg_pattern`, `arg_ident_and_pattern`, `return_value`, and `with_options::simple` (which passes a custom `Options` carrying `proptest_path`).

Each `test_data/<name>.rs` input pairs with a `snapshots/..._<name>.snap` expansion. To change codegen: edit the input or the generators, run `cargo test -p proptest-macro`, then `cargo insta review` to inspect/accept the diff (review deliberately — don't blind-accept). See `proptest-macro/AGENTS.md` for the crate-wide command set.
