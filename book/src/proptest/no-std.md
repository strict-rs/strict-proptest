# `no_std` Support

Proptest has partial support for being used in `no_std` contexts.

In your `Cargo.toml`, adjust the Proptest dependency to look something like
this:

```toml
[dev-dependencies.proptest]
version = "proptestVersion"

# Opt out of the `std` feature
default-features = false

# libm: Enable allocation and `num-traits`' libm-backed float math without `std`.
features = ["libm"]
```

Some APIs are not available in the no-`std` build. This includes functionality
which necessarily needs `std` such as failure persistence and forking, as well
as features depending on other crates which do not support no-`std` usage, such
as regex support. Use `default-features = false` for a no-`std` build. Proptest requires an allocator: `alloc` provides the base configuration, and `libm` includes `alloc` together with `num-traits`' float math. The `atomic64bit`, `bit-set`, `hardware-rng`, and `f16` features also enable `alloc`, so each can be selected on its own.

Use `alt-stable` when you want stable substitutes for APIs that are still
nightly-only in `std`/`core`/`alloc`, such as `half::f16` instead of primitive
`f16` and `allocator_api2::alloc` types instead of the unstable allocator API:

```toml
features = ["alt-stable"]
```

`alt-stable` includes `libm` and its allocation prerequisite. Use `f16` or `unstable` on nightly for native language and standard-library APIs; `unstable` includes `f16` and therefore allocation. Enabling `alt-stable` alongside either feature selects the stable substitutes.

`attr-macro` enables `strict-test` and its `std` prerequisite because generated `#[property_test]` wrappers execute through the strict runner. It therefore selects a `std` build even with default features disabled.

The `no_std` build may not have access to an entropy source (one exception are
x86-64 machines that support rdrand, in this case the library can be compiled
with the `hardware-rng` feature to get random numbers). If no entropy source is
available, every `TestRunner` (i.e., every `#[test]` when using the `proptest!`
macro) uses a single hard-coded seed. For complex inputs, it may be a good idea
to increase the number of test cases to compensate. The hard-coded seed is not
contractually guaranteed and may change between Proptest releases without
notice.
