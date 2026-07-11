# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/src/arbitrary/_core/` — the `Arbitrary` impls for `libcore` types. This is the base tier: `mod _core;` in `arbitrary.rs` has no `#[cfg]` gate (the sibling `_alloc`/`_std` tiers do), so it underpins every build and must stay no-`std`-clean — never name `std::` outside `#[cfg(test)]` code, and pull shared types from `crate::std_facade`. For the `Arbitrary` trait, the `SMapped`/`Mapped`/`SFnPtrMap` aliases, and the impl-writing macros, see the parent `arbitrary/AGENTS.md`; for workspace-wide conventions, the root `AGENTS.md`.

## How the impls are written

Almost nothing here is a hand-rolled `impl Arbitrary` — `non_zero.rs` is the only exception. The impls are spun up by the helper macros in the sibling `arbitrary/macros.rs`, plus `lift1!` from `arbitrary/functor.rs`:

- `arbitrary!(T; expr)` — canonical `Strategy` is `Just<T>` wrapping `expr` (one fixed/derived value). The richer forms `arbitrary!(T, Strat; expr)` and `arbitrary!([bounds] T, Strat, Params; args => expr)` give an explicit `Strategy`/`Parameters` and access to the generated params.
- `wrap_ctor!(W[, ctor])` — newtype `W<A>` mapped from `any::<A>()` through a constructor (default `W::new`) as `SMapped<A, Self>` + `static_map`; it also emits the matching `lift1!`.
- `wrap_from!([bound] W)` — same idea but maps via `From`/`Into` (`MapInto<A::Strategy, Self>` + `prop_map_into`); the optional `[bound]` is an extra trait bound on the inner `A`.
- `lazy_just!(T, || …)` — `LazyJust<Self, fn() -> Self>`, deferring construction until generation time (for values that can't be produced in a const initializer).
- `lift1!` (functor.rs) generates the higher-order `ArbitraryF1` impl so `proptest-derive` can lift a base strategy over a single-type-param container. There is **no** `lift2!`, so every `ArbitraryF2` impl here (iter `Zip`/`Chain`, result `Result`) is written out by hand.

Every module ends with `#[cfg(test)] mod test` calling `no_panic_test!(name => Type, …)` (also in `arbitrary/macros.rs`): it just generates one value per type and asserts no panic — shrinking is not exercised.

## Per-module map

- `_core.rs` — declares the twelve submodules below; no logic.
- `ascii.rs` — `EscapeDefault`, via `static_map(any::<u8>(), escape_default)` (`SMapped<u8, Self>`).
- `cell.rs` — `Cell` (inner `A: Copy`), `RefCell`, `UnsafeCell` via `wrap_from!`; plus the opaque `BorrowError`/`BorrowMutError`, each built with `lazy_just!` by deliberately provoking a real double-borrow on a throwaway `RefCell` and capturing the `Err`.
- `cmp.rs` — `Reverse` via `wrap_ctor!(Reverse, Reverse)` (the tuple-struct ctor, since there is no `Reverse::new`); `Ordering` as a `prop_oneof!` of the three variants, each a `Just` (`Strategy = TupleUnion<(WeightedStrategy<Just<Ordering>>, …)>`).
- `convert.rs` — intentionally empty: `Infallible` is uninhabited, so no `Arbitrary` exists; the doc comment notes derive must simply exclude such void-like types.
- `fmt.rs` — `core::fmt::Error` via `arbitrary!(Error; Error)` (a `Just`).
- `iter.rs` — the bulk of the directory: `Arbitrary` for iterator adapters. `wrap_ctor!` covers `Once`, `Repeat`, `Cycle`, `Enumerate`, `Fuse`, `Peekable`, `Rev`; `Empty`, `Cloned`, `Zip`, `Chain`, and (via the local `usize_mod!` macro) `Skip`/`Take` get explicit `arbitrary!` + `lift1!`. `Cloned` additionally hand-rolls `ArbitraryF1` (its `Iterator<Item = &'a T>` lifetime can't go through `lift1!`); `Zip` and `Chain` hand-roll `ArbitraryF2`. `StepBy` is `#[cfg(feature = "unstable")]`. A `TODO` notes closure-carrying adapters (`Map`, `Filter`, `FlatMap`, `Scan`, …) are unsupported pending `CoArbitrary`.
- `marker.rs` — `PhantomData<T>` (`T: ?Sized`) via `arbitrary!([T: ?Sized] PhantomData<T>; PhantomData)`.
- `mem.rs` — `Discriminant<A>` via `static_map(any_with::<A>(…), |x| discriminant(&x))` + `lift1!`. `ManuallyDrop` is intentionally left out (commented, because callers can't invoke its `drop`).
- `non_zero.rs` — every `NonZero{U,I}{8,16,32,64,size}` via the local `non_zero_impl!` macro, the directory's only explicit `impl Arbitrary`: it generates the underlying primitive and `prop_filter_map`s through `TryFrom` (`"must be non zero"`), so `0` is rejected and retried (`Strategy = FilterMap<StrategyFor<prim>, fn(prim) -> Option<Self>>`). The 128-bit `NonZeroU128`/`NonZeroI128` are `#[cfg(not(target_arch = "wasm32"))]`.
- `num.rs` — `Wrapping`/`Saturating` via `wrap_ctor!`; the opaque `ParseFloatError`/`ParseIntError` (and unstable `TryFromIntError`) built by triggering the real error (e.g. `"".parse::<f32>().unwrap_err()`); `FpCategory` as a five-way `prop_oneof!` of `Just`s over its variants.
- `option.rs` — `Option<A>` via `OptionStrategy<A::Strategy>`, parameterised by a `Probability` (chance of `Some`) packed with `A::Parameters`, built with `weighted(…)` from `crate::option`; `Probability` itself maps `(0.0..=1.0)` into the newtype (`MapInto<RangeInclusive<f64>, Self>`); `opt::IntoIter<A>` via `Option::into_iter`. `Option<string::ParseError>` (and unstable `Option<!>`) is hard-coded to `None` because the inner type is uninhabited.
- `result.rs` — `Result<A, B>` via `MaybeOk<A::Strategy, B::Strategy>` (deemed the canonical choice), parameterised by `Probability` + both inner params via `maybe_ok_weighted`, with hand-rolled `ArbitraryF1`/`ArbitraryF2`; `IntoIter<A>` via `Result::into_iter`. `Result<_, string::ParseError>` / `Result<string::ParseError, _>` (and unstable `!` variants) map into the single inhabited variant since the other is uninhabited.

## Recurring patterns & gotchas

- **Uninhabited variants.** `Option`/`Result` over `string::ParseError` or (under `unstable`) `!` are special-cased: the empty side can't be generated, so the impl is either `None` or a map into the only inhabited variant. New "Option/Result of an empty type" cases belong here and should follow the same shape.
- **Opaque error types** (`cell::BorrowError`, `num::ParseIntError`, …) are constructed by *causing* the real error at generation time, since their fields are private and there is no public constructor.
- **`unstable`-gated entries:** `iter::StepBy`, `num::TryFromIntError`, and all the `!`-based `Option`/`Result` impls — keep them behind `#[cfg(feature = "unstable")]`.
- **wasm32 carve-out:** the 128-bit `NonZero` types are excluded on `wasm32`.
- **no_std discipline.** `option.rs`/`result.rs` import `string` from `crate::std_facade` (which resolves to `alloc::string` or `std::string`), never from `alloc`/`std` directly — the canonical example of the rule in this tier. Anything that genuinely needs an allocator (`Vec`, `String`, `Box`, …) does not belong in `_core`; it goes in the sibling `_alloc` tier, and `std`-only types in `_std`.
