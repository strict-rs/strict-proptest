# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/src/arbitrary/_alloc/` — `Arbitrary` implementations for `liballoc` types. This whole module is gated behind `#[cfg(any(feature = "std", feature = "alloc"))]` in `arbitrary.rs`. For the tier split, the `Arbitrary` trait, and the impl/test macros these files lean on, see `../AGENTS.md`; for workspace-wide conventions see the root `AGENTS.md`. The sibling `_core/` (always-on) and `_std/` (`std`-only) tiers hold the other impls.

## The idiom — every file is macro-driven

There is almost no hand-written `impl Arbitrary` here; the files call helper macros (defined in `../macros.rs`, with `lift1!` in `../functor.rs`) and let those expand to the trait impls. Reading a file means recognizing the macro forms:

- `arbitrary!(Type, Strategy, Params; args => expr)` — the `Arbitrary` impl itself, fixing `Parameters` + `Strategy` and a body returning the strategy. Shorter forms default `Params = ()` or wrap a constant in `Just<Self>`.
- `wrap_from!(W)` — for wrappers (`Box`, `Rc`, `Arc`): `W<A>` reuses `A`'s strategy via `MapInto<A::Strategy, Self>` (`prop_map_into`).
- `wrap_ctor!(W, ctor)` — like `wrap_from!` but routes `any_with::<A>` through a constructor closure (`SMapped`); used for the range wrappers in `ops.rs`.
- `lazy_just!(T, f; …)` — `Arbitrary` via `LazyJust::new(f)` for zero-data types.
- `lift1!(…)` — the companion `ArbitraryF1` higher-order impl (so `proptest-derive` can map over the container). Most files pair one `lift1!` per `arbitrary!`; two-type-parameter containers hand-write `functor::ArbitraryF2` instead, because `lift1!` only covers a single element type.
- each module ends with `#[cfg(test)] mod test { no_panic_test!(name => Type, …) }`, which only asserts that generating a value does not panic — shrinking quality is intentionally out of scope here.

Container `Parameters` follow a convention: `RangedParams1<A> = product_type![SizeRange, A]` and `RangedParams2<A, B> = product_type![SizeRange, A, B]` (a size bound plus the element types' own params), unpacked with `product_unpack!`; the `lift*` impls instead take a bare `SizeRange`.

Note that several targets wired up here are actually `libcore` types (the `ops` ranges, `Bound`, the `char` iterators, `Utf8Error`, `Ordering`, …). They live in the alloc tier rather than `_core/` because their strategies or `lift1!`/`ArbitraryF1` impls allocate — e.g. `ops.rs` shares an `Rc` across a range's endpoint pair and `collections.rs` shares an `Rc` across `Bound`'s arms.

## std_facade discipline (gotcha)

The crate is `#![no_std]`, so these files import every allocated type (`Box`, `Rc`, `Arc`, `Vec`, `VecDeque`, `BTreeMap`, `Cow`, …) from `crate::std_facade`, never from `alloc::`/`std::` directly — that re-export is what keeps the `alloc`-without-`std` build compiling. `multiplex_alloc!` (from `std_facade.rs`) is the related trick for types that live at different paths in `core`/`alloc`/`std` (the `DecodeUtf16` family in `char.rs`, the allocator module in `alloc.rs`).

## Modules

- `_alloc.rs` — declares the submodules; all are unconditional within the parent `std || alloc` gate, with each file carrying any narrower feature gates internally.
- `boxed.rs` / `rc.rs` — `Box<A>` / `Rc<A>` via `wrap_from!`. (`rc.rs` notes `Weak` is skipped: with no owned `Rc` alive, `upgrade()` would always be `None`.)
- `sync.rs` — `Arc<A>` via `wrap_from!`; a local `atomic!` macro maps `any::<base>()` through `Atomic*::new` for `AtomicBool`, `AtomicIsize`/`AtomicUsize`, and the 8/16/32-bit atomics (always), plus `AtomicI64`/`AtomicU64` under `atomic64bit`; `Ordering` is a `prop_oneof!` of its five variants. `AtomicPtr` is deliberately absent (no `Arbitrary for *mut T`).
- `collections.rs` — the bulk. Local macros `impl_1!` / `dst_wrapped!` / `into_iter_1!` generate, respectively: the owned containers `Vec`, `VecDeque`, `LinkedList`, `BTreeSet`, `BinaryHeap`, and (under `std`) `HashSet`; the DST-slice wrappers `Box<[A]>` / `Rc<[A]>` / `Arc<[A]>` from `Vec<A>`'s strategy (distinct from the sized `Box<A>`/`Rc<A>`/`Arc<A>` in `boxed.rs`/`rc.rs`/`sync.rs`); and each container's `IntoIter`. `HashMap`/`BTreeMap` (and their `IntoIter`) are written out long-hand with both `lift1!` and manual `ArbitraryF2` impls, `HashMap` again `std`-gated. `Bound<A>` is a weighted `prop_oneof!` (2:2:1) over `Included`/`Excluded` (sharing one `Rc`'d inner strategy) and `Unbounded`. `SizeRange`'s own `Arbitrary` lives here too. The actual generators and `*Strategy` types all come from `crate::collection`; this file only attaches the `Arbitrary` family on top.
- `borrow.rs` — `Cow<'static, B>` for `A: Arbitrary + Borrow<B>`, `B: ToOwned<Owned = A> + ?Sized`, mapping a generated `A` through `Cow::Owned`.
- `char.rs` — the iterator/error companions of `char`: `EscapeDebug`/`EscapeDefault`/`EscapeUnicode` and `ToLowercase`/`ToUppercase` via a local `impl_wrap_char!` over `any::<char>()`; `DecodeUtf16` (over a `Vec<u16>` iterator, length capped at `u16::MAX`); `ParseCharError`, `DecodeUtf16Error`, and `CharTryFromError` constructed by deliberately feeding bad input. Reuses `crate::collection::vec`.
- `str.rs` — `ParseBoolError` (the constant `"".parse::<bool>().unwrap_err()`) and `Utf8Error` (builds a `Vec<u8>` of `_` padding plus one of four bad UTF-8 tail sequences chosen by `prop_oneof!`, then takes the `from_utf8` error).
- `hash.rs` — `BuildHasherDefault<H>` for `H: Default + Hasher` (over-constrained on purpose), and, under `std`, `DefaultHasher`/`RandomState` via `lazy_just!`. The deprecated `SipHasher` is intentionally not implemented.
- `ops.rs` — range types: `RangeFull` (constant), `RangeFrom`/`RangeTo`/`RangeToInclusive` via `wrap_ctor!`, and `Range<A>`/`RangeInclusive<A>` which generate an unordered `(A, A)` pair and swap so `start <= end`. Exact `core::ops::CoroutineState<Y, R>` is compiled only with `unstable` without `alt-stable`; `alt-stable` uses the crate-local `alt_stable::CoroutineState<Y, R>` substitute with the same `Yielded`/`Complete` strategy shape. Both carry a manual `ArbitraryF2`.
- `alloc.rs` — allocator APIs. `alloc::Layout` is stable and always available in this tier; its generator picks a power-of-two alignment (a `0..32` shift) and a size clamped to avoid the round-up overflow that `Layout::from_size_align` rejects. Exact `alloc::Global` and `alloc::AllocError` compile only with `unstable` without `alt-stable`; `alt-stable` adds substitute impls for `allocator_api2::alloc::{Global, AllocError}`. `System`/`CollectionAllocErr` are left commented out.

## Feature gates at a glance

- `#[cfg(feature = "std")]` — `HashSet`/`HashMap` and their `IntoIter` (`collections.rs`), `DefaultHasher`/`RandomState` (`hash.rs`).
- `#[cfg(all(feature = "unstable", not(feature = "alt-stable")))]` — exact `alloc::Global`/`AllocError` (`alloc.rs`) and exact `core::ops::CoroutineState` (`ops.rs`).
- `#[cfg(feature = "alt-stable")]` — substitute `allocator_api2::alloc::{Global, AllocError}` (`alloc.rs`) and crate-local `alt_stable::CoroutineState` (`ops.rs`).
- `#[cfg(feature = "atomic64bit")]` — `AtomicI64`/`AtomicU64` (`sync.rs`).
