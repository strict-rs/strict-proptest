# Strict property tests

The `proptest::strict` module is a panic-free way to run property tests: instead of wrapping your property in the `proptest!` macro and letting a failed assertion panic, you call an ordinary function that returns the verdict as a value. It lives behind the `strict-test` feature, which is enabled by default and requires `std`.

A property closure returns `Result<A, E>`, where `A` is its successful evidence and `E` is its concrete assertion failure. The runner returns `PropertyResult<S::Value, A, E>`: either a `PropertyRun<A, E>` or a boxed `PropertyFailure` containing the native counterexample, stopping cause, and all reached evidence. A terminal test can return this result directly or call `.map(drop)` to consume successful evidence:

```rust
use std::process::Termination;
use proptest::collection::vec;
use proptest::prelude::any;
use proptest::strict::ensure_property;
use strict_test_support::ensure_eq;

fn double_reversal() -> impl Termination {
    ensure_property(&vec(any::<u8>(), 0..100), "double reversal", |original| {
        let mut reversed = original.clone();
        reversed.reverse();
        reversed.reverse();
        ensure_eq(reversed, original, "double reversal restores the original")
    }).map(drop)
}
# fn main() -> impl Termination { double_reversal() }
```

`ensure_property(&strategy, context, property)` uses the native generation and shrinking algorithm. The `cause` of a failed report distinguishes:

- `PropertyCause::Falsified { counterexample, failure, evaluation }`: the minimized native input and its corresponding original `E`.
- `PropertyCause::Aborted(reason)`: generation or rejection limits prevented completion.
- `PropertyCause::Interrupted { counterexample, interruption, evaluation }`: a panic, child termination, or timeout interrupted the callback without an assertion failure.
- `PropertyCause::Engine(error)`: configuration, transport, protocol, or infrastructure failed. Codec errors retain their concrete type; I/O failures retain their native error.

The report retains ordered evaluations, successful subjects, intermediate failures, execution events, and case statistics. Cache hits reference an `EvaluationId`; they neither clone payloads nor invoke the callback again. The final failure appears once in `cause`, with an `EvaluationOutcome::RetainedFailure` marker at its original evaluation position. If an engine error interrupts shrinking, `established_failure` identifies the last established counterexample and its still-retained evaluation. Finalization errors remain separately available in `finalization`.

Passing or rejected intermediate candidates do not replace the established failing pair. Exhausting an iteration or time budget returns that pair without reevaluating the property. In-process subjects may borrow caller-owned data and need not implement `Clone`, `Send`, `Sync`, or serialization traits. Keeping the report alive also keeps its subjects alive; consume or drop it at the intended boundary.

## Writing property bodies with `ensure*`

Use the `strict-test-support` assertion vocabulary inside the property closure. `ensure_eq` and `ensure_ne` retain both native compared values, while `ensure_that` checks a complete owned subject by reference and returns that subject or `PredicateFailure<S>`. `ensure_some` and `ensure_ok` preserve native absence and errors through `OptionFailure<T>` and `ResultFailure<E>`. Reserve `ensure` for intrinsically boolean inputs, and retain the accepted checks and unvisited remainder when using `ensure_all`.

Helpers return their complete successful evidence. Compose heterogeneous failures in a property-owned `thiserror` enum, retaining prior observations when a later check fails. Adapt success to `()` only at a terminal test or executable boundary. Import fixture-only `TestFailure` from `strict_test_support` when a fixture operation needs it; it is not a conversion target for generic assertion failures.

## Preconditions belong in the strategy

Where a classic proptest test would reject unwanted inputs with `prop_assume!`, a strict property expresses the precondition in the strategy itself with [`Strategy::prop_filter`](https://docs.rs/proptest/latest/proptest/strategy/trait.Strategy.html#method.prop_filter):

```rust
use proptest::strategy::Strategy;

let non_zero = (0i32..100).prop_filter("divisor must be non-zero", |d| *d != 0);
```

Unwanted inputs are then skipped at generation time, and an over-strict filter surfaces as `PropertyCause::Aborted` instead of silently discarding cases.

## Determinism, seeds, and persistence

`ensure_property` builds its configuration with `strict_default_config()`, which starts from `Config::default()` — so the ordinary `PROPTEST_*` environment variables (case count, shrink budgets, and so on) still apply — and then makes two deliberate changes:

- **Failure persistence is disabled.** A default strict run writes no `proptest-regressions/` file; the minimized native counterexample is carried in the returned `PropertyFailure`, and the intended workflow is to pin it as a named unit test.
- **The RNG is seeded deterministically.** Every run draws the same inputs by default, so coverage does not drift run-to-run. The `STRICT_TEST_SEED` environment variable selects the seed: unset (or an unrecognized value) pins the fixed seed `0x5EED`, `STRICT_TEST_SEED=random` opts into OS entropy for an exploratory run, and `STRICT_TEST_SEED=<integer>` pins that exact seed to replay a discovered case.

When you need explicit control, `ensure_property_with_config(&strategy, context, config, property)` uses the `Config` you pass verbatim — it applies none of the strict defaults, so persistence and seeding choices are yours.

## The `#[property_test]` attribute

With the `attr-macro` feature, `#[property_test]` accepts `Result<A, E>` or a result alias. Unit-returning bodies are rejected. The generated wrapper returns `PropertyResult<(ArgumentTypes, ...), A, E>` with an inspectable argument tuple and diagnostic argument labels. Custom strategies, patterns, mutability, and `proptest_path` overrides are preserved:

```rust,test_harness
use proptest::property_test;
use strict_test_support::{ComparisonFailure, ensure_eq};

type ReversalCheck = Result<(Vec<u8>, Vec<u8>), ComparisonFailure<Vec<u8>, Vec<u8>>>;

#[property_test]
fn reversing_twice_is_identity(v: Vec<u8>) -> ReversalCheck {
    let mut w = v.clone();
    w.reverse();
    w.reverse();
    ensure_eq(w, v, "double reversal restores the original")
}
```

The wrapper uses strict defaults unless `config = ...` supplies an explicit configuration. It sets `test_name` and `source_file` to the annotated test while preserving the remaining configured fields. `transport = CodecType => codec_expression` supplies a concrete `PropertyTransport` for the argument tuple, successful evidence, and assertion failure.

## Explicit process transport

`ensure_property_with_transport(&strategy, context, config, codec, property)` runs configured forked or timeout-limited properties with a declared codec. The parent reconstructs typed outcomes from versioned, framed records and never executes the property callback. Truncation, codec errors, process termination, and timeout have distinct causes; a failed decode preserves the already-decoded prefix.

The codec must preserve the case, both returned outcomes, its own failures, and any replay state needed by the strategy. State-machine cases require restoration of the shared seen-transition counter before shrinking resumes. Process-local resources require an explicit faithful representation; ordinary in-process properties impose no transport bounds. Fork or timeout settings passed to a typed entry point without a codec produce `ExecutionError::TransportRequired` before the first callback.

Transport and regression persistence are independent. An explicitly configured persistence backend remains effective for both typed entry points; default strict persistence remains disabled. See [Forking and Timeouts](forking.md) and the complete codec in `proptest/examples/fib.rs`.

## The legacy macro surface

The `proptest!`, `prop_assert*`, and `prop_assume!` macros remain available and documented in the [tutorial](tutorial/macro-proptest.md); they panic on failure, which is what the strict surface exists to avoid. New code targeting the strict policy should prefer `proptest::strict::ensure_property` with `ensure*` bodies and `prop_filter` preconditions.
