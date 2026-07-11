# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/src/arbitrary/` — the `Arbitrary` trait, its free-function entry points and type aliases, the macros that write `Arbitrary` impls, and the per-target impls it dispatches to (primitives/arrays/tuples/sample live here; everything else lives in the `_core`/`_alloc`/`_std` tiers). For shared conventions see the workspace-root `AGENTS.md`.

## The trait and entry points (`traits.rs`)

`Arbitrary: Sized + fmt::Debug` gives a type its *canonical* `Strategy` — the strategy that `any::<T>()` returns:

- associated `type Parameters: Default` — the config `arbitrary_with` accepts; `type Strategy: Strategy<Value = Self>` — the concrete strategy type produced.
- `arbitrary_with(args: Self::Parameters) -> Self::Strategy` is the one required method; `arbitrary()` is a provided default equal to `arbitrary_with(Default::default())` (overriding it is a logic error unless it preserves that meaning).

Four free functions wrap those methods, all `#[must_use = "strategies do nothing unless used"]`:

- `any::<A>() -> StrategyFor<A>` and `any_with::<A>(args) -> StrategyFor<A>` — turbofish-friendly, name `A` explicitly; these are the common entry points (re-exported in the prelude).
- `arbitrary::<A, S>()` / `arbitrary_with::<A, S, P>(args)` — same result reached through the `proptest::arbitrary` path, but with extra `where` back-links (`S: Strategy<Value = A>, A: Arbitrary<Strategy = S, …>`) that drive type inference so you rarely name the type parameters.

`StrategyFor<A> = <A as Arbitrary>::Strategy` and `ParamsFor<A> = <A as Arbitrary>::Parameters` (also defined here, re-exported with `pub use self::traits::*`) let callers name those associated types without spelling out the often-huge generic concrete type, so tests don't break when an impl's `Strategy` changes.

## Doc-only `Strategy` aliases (`arbitrary.rs`)

Three aliases name the long mapped-strategy types the impl macros generate, purely to keep rustdoc readable — both public ones carry a `# Stability` note: *do not rely on them in your own code*.

- `SMapped<I, O> = statics::Map<StrategyFor<I>, fn(I) -> O>` — the output of `statics::static_map` over `any::<I>()`; this is the `Strategy` type `wrap_ctor!` produces.
- `Mapped<I, O> = Map<StrategyFor<I>, fn(I) -> O>` — same shape but over the ordinary `strategy::Map` (`prop_map`) combinator instead of the `statics` one.
- `SFnPtrMap<S, O> = statics::Map<S, fn(<S as Strategy>::Value) -> O>` — the `pub(crate)` variant for an arbitrary source strategy `S` (not necessarily `StrategyFor<I>`).

## Module wiring & tier dispatch (`arbitrary.rs`)

Declaration order is load-bearing: `functor` (`#[macro_use] pub mod`, exports `lift1!`) and `macros` (`#[macro_use] mod`, exports `arbitrary!`/`wrap_ctor!`/…) are declared *before* every module that uses those macros — `arrays`, `primitives`, `sample`, `tuples`, then the three tiers. Textual macro scope means moving either declaration down breaks the build.

Impls are partitioned by what they require and `#[cfg]`-gated, one tier per dependency level — put a new impl in the tier matching its lightest requirement, or you'll break the `no_std`/`alloc` builds:

- `_core/` — always compiled, no gate (libcore types). See `_core/AGENTS.md`.
- `_alloc/` — `#[cfg(any(feature = "std", feature = "alloc"))]` (liballoc types). See `_alloc/AGENTS.md`.
- `_std/` — `#[cfg(feature = "std")]` (libstd types). See `_std/AGENTS.md`.

## Impl-writing macros (`macros.rs`)

These exist so a typical impl is one line. They encode the subsystem's contract, spelled out on `no_panic_test!`'s doc: an `Arbitrary` impl only has to *generate* a value without panicking — shrinking quality is secondary and tested separately.

- `arbitrary!` — writes the `impl Arbitrary`. Full form `arbitrary!([bounds] T, Strat, Params; args => expr)`; shorter shared forms default `Params = ()` or wrap a constant as `Just<Self>`; the list form `arbitrary!(A, B, …)` expands each to `arbitrary!(T, T::Any; T::ANY)` (used for the bool/integer primitives). The `std`-only full-params shorthand lives in `_std.rs` as `std_arbitrary_with_params!`, so no-`std` builds do not compile arms they cannot use.
- `wrap_ctor!(W, ctor)` / `wrap_ctor!([bounds] W, ctor)` — newtype `W<A>` mapped from `any::<A>()` through the explicit constructor as `SMapped<A, Self>` via `static_map`; also emits the matching `lift1!`. The `std`-only default-constructor shorthand lives in `_std.rs` as `std_wrap_ctor_default!`.
- `wrap_from!([bound] W)` — same idea via `From`/`Into` (`MapInto<A::Strategy, Self>` + `prop_map_into`); also emits `lift1!`.
- `lazy_just!(T, f; …)` — `Strategy = LazyJust<Self, fn() -> Self>`, deferring construction to generation time.
- `no_panic_test!(name => Type, …)` — `#[cfg(test)]` only; emits one test function per `name` that draws `any::<Type>()` through the strict runner and asserts no panic. Every impl module ends with one.

## Higher-order traits (`functor.rs`)

`ArbitraryF1<A>` / `ArbitraryF2<A, B>` lift base strategies to unary (`Box`, `Vec`, `Option`) / binary (`Result`, `HashMap`) type constructors — each with `type Parameters: Default`, a required `lift1_with`/`lift2_with`, and a provided `lift1`/`lift2` that defaults the params. They return `BoxedStrategy<Self>` (a deliberate boxing cost, since they predate stable `-> impl Trait`; impls just end in `.boxed()`). They exist *mainly for `proptest-derive`* to map over container types when deriving recursive types, and are intentionally not conveniently exported (stability note: prefer e.g. `proptest::collection::vec`).

The `lift1!` macro (defined here) generates `ArbitraryF1` impls in four forms: a full hand-written body, a params-defaulted body, a `prop_map`-via-mapper body, and a `prop_map_into` default. There is **no** `lift2!`; every `ArbitraryF2` impl is hand-written.

## Direct impls (primitives, arrays, tuples, sample)

- `primitives.rs` — `bool` and the integers via the `arbitrary!(…)` list form (`<T>::Any`/`<T>::ANY` from `crate::num`); `f32`/`f64` as `<f>::Any` over a `POSITIVE | NEGATIVE | ZERO | SUBNORMAL | NORMAL` union that **deliberately excludes NaN and infinity**; `char` via `char::CharStrategy<'static>` (`char::any()`).
- `arrays.rs` — a hand-written `impl<A: Arbitrary, const N: usize> Arbitrary for [A; N]` (any length) with `Strategy = UniformArrayStrategy<A::Strategy, [A; N]>` (from `crate::array`) and `Parameters = A::Parameters`.
- `tuples.rs` — `()` as a `Just`, plus the local `impl_tuple!` for arities 1..=10: `Parameters = product_type![Ti::Parameters…]`, `Strategy = (Ti::Strategy…)` (a tuple of strategies is itself a strategy), delegating each element to `any_with`.
- `sample.rs` — hand-written `Arbitrary` (both `Parameters = ()`) for `crate::sample`'s `Index` (`IndexStrategy`) and `Selector` (`SelectorStrategy`).
