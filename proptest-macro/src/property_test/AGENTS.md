# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-macro/src/property_test/` — the parse / validate / options front end of the `#[property_test]` attribute macro (code generation lives in `codegen/`). For build/test commands and crate dependencies see the parent `proptest-macro/AGENTS.md`; for workspace-wide conventions see the workspace-root `AGENTS.md`.

## Pipeline (`property_test.rs`)

`property_test(attr, item) -> TokenStream` is the module entry and a four-step pipeline:

1. `parse!(item)` into a `syn::ItemFn` — the annotated test fn, bound `mut` so `validate` can rewrite it.
2. `parse!(attr)` into `Options` — the attribute body, e.g. `config = ...` (see `options.rs`).
3. `validate(&mut item_fn)` — on `Err` the accumulated `compile_error!` tokens are returned verbatim.
4. `codegen::generate(item_fn, &options)` — produces and returns the rewritten `#[test]` fn.

The local `parse!` macro is the diagnostic backbone: on a `parse2` failure it `return`s `e.into_compile_error()` (a `compile_error!` token stream) instead of panicking, so a malformed signature or attribute still surfaces as a normal, spanned compiler error at the use site rather than a macro panic. Each `parse!`'s target type is inferred from later use — `ItemFn` from `validate`, `Options` from `generate`.

`lib.rs` (one level up) holds the actual `#[proc_macro_attribute]` shim plus all user-facing rustdoc/examples; it only `.into()`-converts `proc_macro` <-> `proc_macro2` token streams and delegates here. Its doc comment is the semver contract, including the concrete argument tuple, native callback outcomes, and explicit transport option.

## Validation (`validate.rs`)

`validate(f: &mut ItemFn) -> Result<(), TokenStream>` deliberately checks little: the guiding principle (stated in its doc comment) is to **defer to rustc** wherever rustc already emits a good error, passing the offending syntax straight through to the generated fn. Three checks run here:

- `all_args_non_self` — rejects any `FnArg::Receiver` with "`self` parameters are forbidden" (covers `self`, `&self`, `&mut self`, and `self: T` forms; see the `validate_fails_with_self_arg` test).
- `validate_parameter_attrs` — permits only outer `#[strategy = <expr>]` attributes on parameters. Any other attribute, an inner `#![...]`, or a malformed `#[strategy(...)]` / bare `#[strategy]` (all of which `is_strategy` rejects) gets a "only `#[strategy = <expr>]` attributes are allowed here" error; a second `#[strategy = ...]` on one param gets a duplicate error. As a side effect it rewrites each param's `attrs` down to at most the one valid strategy attr, so `strip_args` later sees a clean list.
- `returns_strict_result` — rejects a missing return type (`ReturnType::Default`) and a literal `-> ()` with the stable `UNIT_RETURN_ERROR` message pointing the author at `Result<A, E>` or a result alias. The check is **syntactic** (a `type Foo = ()` alias is not caught — that case falls through to rustc's type error at the generated `ensure_property` call); it deviates from defer-to-rustc deliberately, because the post-expansion inference error would not name the actual contract. Runs after the other checks so their diagnostics keep priority.

Everything else about the signature — generics, where-clauses, `async` / `const`, `unsafe`, argument patterns/types — is intentionally **not** inspected. `unsafe` is the canonical example: it is passed through verbatim so rustc emits its own "test fn cannot be unsafe" error rather than this macro re-implementing the diagnostic.

Errors are **accumulated, not bailed-on**: `validate_parameter_attrs` builds a single `TokenStream` of every `compile_error!` across all params and returns it only at the end, so one invocation can report multiple problems at once. (`all_args_non_self` does still short-circuit on the first receiver.) The shared `err()` helper emits statement-form `compile_error!("...");` tokens — the trailing semicolon matters, because the tokens are spliced at statement/item position.

## Options (`options.rs`)

`Options` (derives `Default`) contains four fields:

- `config: Option<Expr>` — an explicit runner configuration. Codegen forces `test_name` and `source_file` and preserves other supplied fields. Without this option it starts from strict defaults.
- `proptest_path: Option<Path>` — the renamed or re-exported crate path, validated as a qself-free `Expr::Path`.
- `transport: Option<Transport>` — `transport = CodecType => codec_expression`, retaining the concrete type for the wrapper signature and the constructor evaluated once per wrapper call.
- `errors: Vec<TokenStream>` — recoverable option diagnostics.

`true_proptest_path()` resolves every emitted proptest path through the override or `::proptest`. The `Parse` implementation reads a comma-separated option list; `transport` parses a type, `=>`, and an expression, while other recognized keys parse expressions. Unknown keys and invalid crate paths accumulate diagnostics while parsing continues. Invalid syntax returns `syn::Error`. Codegen emits accumulated diagnostics beside the generated item, so `#[test]` stripping cannot hide them in non-test builds.

## Helpers (`utils.rs`)

The bridge from a validated fn to codegen:

- `Argument { pat_ty: PatType, strategy: Option<Expr> }` — one parameter plus its optional `#[strategy = expr]` override.
- `strip_args(f: ItemFn) -> Result<(ItemFn, Vec<Argument>), TokenStream>` — takes the inputs and preserves the function and extracted argument syntax. A receiver returns its spanned compiler diagnostic even when this stage is called directly.
- `strip_strategy` — pulls the single strategy attr off a `PatType`, leaving the param's remaining attributes attached.
- `is_strategy(attr) -> bool` — the shared predicate used by both `validate` and `strip_strategy`: true only for an *outer* attribute whose path is exactly `strategy` and whose meta is `NameValue`. It is what makes `#![strategy = ...]`, `#[strategy(...)]`, and bare `#[strategy]` count as "not a strategy" (and thus get rejected by validate).

The per-argument override threads across three files: `validate` permits/strips it, `utils` extracts it into `Argument.strategy`, and `codegen` consumes it as that argument's custom strategy. Codegen uses the original argument types and patterns directly in a tuple callback.

## Code generation (`codegen/`)

`codegen::generate(item_fn, &options)` projects the declared result through `PropertyReturn`, builds a typed `PropertyResult` signature over the argument tuple, and invokes the strict runner with a typed callback. See `codegen/AGENTS.md` for the emitted-code shape.

## Tests (`tests/`)

`tests/snapshot_tests.rs` expands parsed token fixtures with default options, formats them with `prettyplease`, and compares them through `strict_test_support::ensure_snapshot` against `proptest-macro/tests/snapshots/*.snap`. The cases cover default strategies, all-custom strategies, and mixed strategies. Separate codegen fixtures cover argument patterns, result aliases, renamed crate paths, and transport. Parser and extraction tests inspect native AST results and diagnostics.

Run `cargo test -p proptest-macro`. Refresh snapshots with `SNAPSHOTS=overwrite cargo test -p proptest-macro`, then review the generated artifact diff. Compiler diagnostics are a separate consumer-side trybuild workflow.
