# Strict property tests

The `proptest::strict` module is a panic-free way to run property tests: instead of wrapping your property in the `proptest!` macro and letting a failed assertion panic, you call an ordinary function that returns the verdict as a value. It lives behind the `strict-test` feature, which is enabled by default and requires `std`.

A strict property test is a plain `#[test]` function returning `proptest::strict::TestResult`, which is an alias for `Result<(), TestFailure>` (`TestFailure` is re-exported from the `strict-test-support` crate). The property closure returns the same type, so the whole test composes with `?` and never panics:

```rust,ignore
use proptest::strict::{ensure_property, TestResult};
use strict_test_support::ensure;

#[test]
fn addition_commutes() -> TestResult {
    ensure_property(&(0u32..1000, 0u32..1000), "addition commutes", |(a, b)| {
        ensure(a + b == b + a, "a + b equals b + a")
    })
}
```

`ensure_property(&strategy, context, property)` generates inputs from any `Strategy`, runs the closure on each, and — when the closure returns `Err` — shrinks the input to a minimal counterexample exactly like the classic runner. The failure comes back as a value:

- `TestFailure::PropertyFalsified { context, report }` — an input falsified the property; `report` carries the engine's rendering of the failure, including the shrunk minimal failing input.
- `TestFailure::PropertyAborted { context, reason }` — the run could not complete, for example because a strategy filter rejected too many inputs.

## Writing property bodies with `ensure*`

Inside the closure you use the `strict-test-support` vocabulary instead of panicking assertion macros: `ensure(condition, context)`, `ensure_eq(&left, &right, context)`, `ensure_ne`, `ensure_some(option, context)`, `ensure_ok(result, context)`, `ensure_contains`, and `ensure_all`. Each returns `Result<(), TestFailure>` with a message naming what was expected, so a failing case reads like a sentence rather than a backtrace.

## Preconditions belong in the strategy

Where a classic proptest test would reject unwanted inputs with `prop_assume!`, a strict property expresses the precondition in the strategy itself with [`Strategy::prop_filter`](https://docs.rs/proptest/latest/proptest/strategy/trait.Strategy.html#method.prop_filter):

```rust,ignore
use proptest::strategy::Strategy;

let non_zero = (0i32..100).prop_filter("divisor must be non-zero", |d| *d != 0);
```

Unwanted inputs are then skipped at generation time, and an over-strict filter surfaces as `TestFailure::PropertyAborted` instead of silently discarding cases.

## Determinism, seeds, and persistence

`ensure_property` builds its configuration with `strict_default_config()`, which starts from `Config::default()` — so the ordinary `PROPTEST_*` environment variables (case count, shrink budgets, and so on) still apply — and then makes two deliberate changes:

- **Failure persistence is disabled.** A strict run never writes a `proptest-regressions/` file; the shrunk minimal counterexample is carried in the returned `TestFailure` instead, and the intended workflow is to pin it as a named unit test.
- **The RNG is seeded deterministically.** Every run draws the same inputs by default, so coverage does not drift run-to-run. The `STRICT_TEST_SEED` environment variable selects the seed: unset (or an unrecognized value) pins the fixed seed `0x5EED`, `STRICT_TEST_SEED=random` opts into OS entropy for an exploratory run, and `STRICT_TEST_SEED=<integer>` pins that exact seed to replay a discovered case.

When you need explicit control, `ensure_property_with_config(&strategy, context, config, property)` uses the `Config` you pass verbatim — it applies none of the strict defaults, so persistence and seeding choices are yours.

## The `#[property_test]` attribute

With the `attr-macro` feature, the `#[property_test]` attribute macro generates exactly this strict shape: the annotated function must return `proptest::strict::TestResult` (a `()` body is a compile error), and the generated wrapper runs the body through `ensure_property` with the strict defaults — or through `ensure_property_with_config` when you pass `config = ...`:

```rust,ignore
use proptest::property_test;
use strict_test_support::ensure_eq;

#[property_test]
fn reversing_twice_is_identity(v: Vec<u8>) -> proptest::strict::TestResult {
    let mut w = v.clone();
    w.reverse();
    w.reverse();
    ensure_eq(&w, &v, "double reversal restores the original")
}
```

## The legacy macro surface

The `proptest!`, `prop_assert*`, and `prop_assume!` macros remain available and documented in the [tutorial](tutorial/macro-proptest.md); they panic on failure, which is what the strict surface exists to avoid. New code targeting the strict policy should prefer `proptest::strict::ensure_property` with `ensure*` bodies and `prop_filter` preconditions.
