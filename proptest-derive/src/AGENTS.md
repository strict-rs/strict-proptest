# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-derive/src/` — the `#[derive(Arbitrary)]` implementation. For build/test/feature commands, crate layout, and the nightly requirement see the crate `AGENTS.md` one directory up; for workspace-wide conventions see the workspace-root `AGENTS.md`.

## The compile pipeline

`lib.rs` is the only public surface: `#[proc_macro_derive(Arbitrary, attributes(proptest))]` parses the input with `syn` and hands the `DeriveInput` to `derive::impl_proptest_arbitrary`. Everything else is internal. The stages, in order:

1. `derive.rs` — the orchestrator. `derive_proptest_arbitrary` rejects lifetimes (E0001) and unions (E0002), parses the type's own `#[proptest(..)]` attrs, builds a `UseTracker`, then dispatches to `derive_struct` / `derive_enum`, which fold the fields/variants into the `ast.rs` IR.
2. `attr.rs` — turns raw `syn` attributes into the logical model `ParsedAttributes` (`skip`, `weight`, `params: ParamsMode`, `strategy: StratMode`, `filter`, `no_bound`). See modifiers below.
3. `interp.rs` — a tiny CTFE interpreter, `eval_expr(&syn::Expr) -> Option<u128>`, over integer/byte literals and `+ - * / % ^ & | << >> !` (checked; `None` on overflow, division by zero, floats, negatives, or anything non-const). Used to evaluate `weight` expressions (attr.rs) and array lengths `N` in `[T; N]` (void.rs). Integer-literal parsing is adapted from `syn` to accept `u128`.
4. `use_tracking.rs` — `UseTracker` plus the `UseMarkable` trait decide which generic type parameters need an `Arbitrary` bound.
5. `ast.rs` — the high-level IR; `Impl::into_tokens` linearizes it to the final Rust `impl`.

`impl_proptest_arbitrary` runs the derive against an error `Context` and then `ctx.check()`: it returns generated tokens on success, `compile_error!(..)` if any errors were recorded, and panics ("internal error, this is a bug") only if a `Fatal` was returned without a recorded error — i.e. every abort must record a message first.

## `#[proptest(...)]` modifiers (attr.rs)

Dispatched by name in `dispatch_attribute`; each maps to a `ParsedAttributes` field:

- `skip` (bare) — enum variants only; omit this variant from generation.
- `weight = N` / `w = N` (also `weight(N)`, `weight = "expr"`) — relative weight of an enum variant; the value is run through `interp::eval_expr` and must fit `u32`.
- `params(Ty)` / `params = "Ty"` — set the associated `Parameters` type (the string form is needed for complex types).
- `no_params` (bare) — use `()` / default parameters.
- `strategy = "expr"` / `strategy("expr")` — use an explicit strategy (type-erased into `BoxedStrategy` via `.boxed()`).
- `value = "expr"` / `value(expr)` — a constant, non-shrinking strategy (`(|| expr) as fn() -> _`).
- `regex = "str"` / `regex(ident)` — generate via `StrategyFromRegex::from_regex`; the field type must implement that trait.
- `filter("expr")` / `filter = "expr"` — add a `prop_filter` predicate. The only repeatable modifier (it accumulates into a `Vec`); every other modifier errors if set twice (E0017).
- `no_bound` (bare) — on the type or a generic param, suppress the `Arbitrary` bound (see below).

Mutual exclusion is resolved late: at most one of `{strategy, value, regex}` (else E0025), at most one of `{params, no_params}` (else E0022). `normalize_meta` defines the three accepted shapes — a plain word, `= <lit>`, and `(<word>|<lit>)` — and a bare-`<ident>` argument to `strategy`/`value`/`regex` is interpreted as a nullary call (`ident()`). Misuse is its own error class: bare `#[proptest]` → E0014, `#[proptest = lit]` → E0015, a literal directly inside the list → E0016, inner `#![proptest(..)]` → E0013, unknown modifier → E0018 (with did-you-mean suggestions for common typos like `weights`, `strat`, `parameters`).

## Bound inference (use_tracking.rs)

`UseTracker::new` seeds a `used_map` (an insertion-ordered `Vec<(Ident, bool)>`, not a hash map, to keep generic order stable) from the type's generics. As `derive.rs` builds strategies, `StratMode::Arbitrary` fields call `ty.mark_uses(tracker)`; the `UseMarkable for syn::Type` impl walks the type with a `syn::visit` visitor that:

- marks each simple-path identifier matching a generic as used (`use_tyvar`),
- skips `PhantomData<T>` innards (via `util::is_phantom_data`) so phantom params don't get a spurious bound,
- skips macro bodies, and
- records associated-type projections of a generic (e.g. `T::Assoc`, `<T as Tr>::Assoc`) into a separate `where_types` set so they get a `where` bound instead of a bound on the param itself.

`add_bounds` (called from `ast.rs`) then pushes `Arbitrary` onto every *used* param and `Debug` onto every *unused* one — proptest's `Arbitrary` requires `Debug`, so even params that don't drive generation still need it. A param carrying `#[proptest(no_bound)]` gets `Debug` only; `no_bound` on the whole type calls `no_track()` so nothing is marked used. `no_bound` anywhere else → E0031, and `has_no_bound` also rejects any other attribute placed on a type parameter.

## The generated impl (ast.rs)

`Impl` = (type ident, `UseTracker`, `ImplParts = (Params, Strategy, Ctor)`). `into_tokens` emits the impl inside `#[allow(..)] const _: () = { use proptest as _proptest; impl .. Arbitrary for .. { type Parameters; type Strategy; fn arbitrary_with(..) } };` — the anonymous const plus the `_proptest` alias keep the expansion hygienic.

`Strategy` and `Ctor` are dual IRs (the associated `Strategy` *type* versus the constructor *expression*); they are always built together by the `pair_*` smart constructors and tokenized via `ToTokens`:

- `Arbitrary` → `any::<T>()` / `any_with::<T>(params)`; `Value` → `fn() -> T`; `Existential` → `BoxedStrategy<T>`; `Regex` → `StrategyFromRegex`; `Map` → `prop_map` of a tuple of field strategies into `Self`/the variant (the closure is built by `MapClosure`); `Filter` → `prop_filter`.
- `NestedTuple` nests tuples in chunks of `NESTED_TUPLE_CHUNK_SIZE` (9) so generated tuples stay within proptest's tuple `Arbitrary` impls.
- Unions: by default a nested `TupleUnion` with `Arc`-wrapped summands, kept linear for the first `UNION_CHUNK_SIZE` (9) and then nesting; keep both constants in sync with proptest.

`boxed_union` feature: swaps the `TupleUnion` codegen for `Union::new_weighted(vec![(w, strat.boxed()), ..])` over `BoxedStrategy<Self>` — simpler output that erases and allocates instead of building the static tuple type. `UNION_CHUNK_SIZE` is compiled out under this feature.

Constants: `TOP_PARAM_NAME = "_top"` (the `arbitrary_with` argument) is internal; `API_PARAM_NAME = "params"` is user-facing — an explicit `strategy`/`value` expression may name `params` — so renaming it is a breaking change. Parameter threading from the "no explicit params" derivation uses the `FromReg`/`ToReg` register let-bindings emitted by `Ctor::Extract`.

## derive.rs specifics

Structs: reject enum-only attrs and a `strategy` on the struct itself; a unit struct becomes a constant `Self {}` strategy; otherwise reject uninhabited (E0003) and build a `prop_map`. Enums: reject `skip`/`strategy`/`weight` on the enum; bail on zero variants (E0004) or all-uninhabited variants (E0005); per variant, `keep_inhabited_variant` drops `skip`ped variants (validating that *only* `skip` is set) and uninhabited variants and defaults weight to 1; if every inhabited variant was skipped → E0006; a weight-sum overflow → E0033. The "params set on the type" branches are simple; the "not set" branches thread each field/variant's own `Parameters` through the `PartsAcc` / `ParamAcc` / `StratAcc` accumulators.

## Uninhabitedness (void.rs)

`IsUninhabited` is a best-effort, sound-but-incomplete check: a `false` means "can't prove it uninhabited", not "inhabited". Full inhabitation is undecidable and the macro can't see type definitions, type macros, or projections, so the analysis is intentionally conservative. The `syn::Type` visitor flags the never type `!`, a hardcoded known list (`std::string::ParseError`), and `[T; N]` with `N > 0` and uninhabited `T` (with `N` evaluated by `interp::eval_expr`); it deliberately stops descent at bare fns, macros, `impl Trait`, trait objects, and zero-length arrays. Combinators: an enum is uninhabited iff *all* variants are; a struct/variant iff *any* field is. It is purposely stricter than Rust ("can we generate a value", not type-theoretic inhabitation), and `derive.rs` uses it to drop uninhabited variants and to reject uninhabited structs/enums.

## Diagnostics (error.rs)

`Context` accumulates messages so multiple problems surface at once. The two generator macros draw the line: `error!` records a non-fatal message and lets analysis continue (the generated fn returns `()`); `fatal!` records and returns `Err(Fatal)` to abort immediately. `check()` collapses the collected messages into a single `compile_error!`. `mk_err_msg!` stamps each message with its code and a docs URL (`errors.html#e0001`); the inline `test_mk_err_msg_format` test pins that exact format. Codes run E0001–E0035 but are *not* one-per-message — several related diagnostics share a code (E0007, E0018, E0028, E0029, E0030) and E0024 is unused. The `tests/compile-fail` UI tests assert against these codes, so changing a code, message text, or the `mk_err_msg!` format will break them. Note `unkown_modifier` is a deliberately-kept misspelling (carries a TODO).

## util.rs, tests.rs

`util.rs` holds the `syn` helpers: `fields_to_vec` normalizes named/unnamed/unit fields into a `Vec<Field>` (the derive always emits braced `Path { .. }` literals regardless of the original struct style), plus `self_ty`, `is_unit_type`, `match_singleton`, and the path predicates (`eq_simple_path`, `match_pathsegs`, `extract_simple_path`, `path_is_global`, `is_phantom_data`).

`tests.rs` is an inline `#[cfg(test)]` module of expansion tests using `test!` / `test_derive!` (adapted from `synstructure`): they run `impl_proptest_arbitrary` and compare its stringified tokens against an expected expansion; the plain form additionally emits an `ensure_compiles` fn, while the `no_build` form only diffs tokens. Coverage here is just unit structs — behavioral and error-path coverage lives in the crate's `tests/` directory (run on nightly; see its `AGENTS.md`).

Also note the standing limitations called out at the top of `lib.rs`: arrays `[T; N]` with `N > 32` and self-/mutually-recursive types are unsupported.
