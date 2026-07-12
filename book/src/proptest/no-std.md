# `no_std` Support

Proptest has partial support for being used in `no_std` contexts.

In your `Cargo.toml`, adjust the Proptest dependency to look something like
this:

```toml
[dev-dependencies.proptest]
version = "proptestVersion"

# Opt out of the `std` feature
default-features = false

# alloc: Use the `alloc` crate directly. Proptest has a hard requirement on
# memory allocation, so either this or `std` is needed.
# libm: Use `num-traits`' libm-backed float math without enabling `std`.
features = ["libm", "alloc"]
```

Some APIs are not available in the no-`std` build. This includes functionality
which necessarily needs `std` such as failure persistence and forking, as well
as features depending on other crates which do not support no-`std` usage, such
as regex support. Use `default-features = false` for a no-`std` build, add
`alloc` when allocation-backed APIs are needed, and add `libm` when float math
from `num-traits` is needed without `std`.

Use `alt-stable` when you want stable substitutes for APIs that are still
nightly-only in `std`/`core`/`alloc`, such as `half::f16` instead of primitive
`f16` and `allocator_api2::alloc` types instead of the unstable allocator API:

```toml
features = ["libm", "alloc", "alt-stable"]
```

Use `unstable` only on nightly when you want the exact nightly standard-library
or language API implementations rather than stable substitutes.

The `no_std` build may not have access to an entropy source (one exception are
x86-64 machines that support rdrand, in this case the library can be compiled
with the `hardware-rng` feature to get random numbers). If no entropy source is
available, every `TestRunner` (i.e., every `#[test]` when using the `proptest!`
macro) uses a single hard-coded seed. For complex inputs, it may be a good idea
to increase the number of test cases to compensate. The hard-coded seed is not
contractually guaranteed and may change between Proptest releases without
notice.
