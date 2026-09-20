# Forking and Timeouts

By default, proptest tests are run in-process and are allowed to run for
however long it takes them. This is resource-efficient and produces the nicest
test output, and for many use cases is sufficient. However, problems like
overflowing the stack, aborting the process, or getting stuck in an infinite
loop will simply break the entire test process and prevent proptest from
determining a minimal reproducible case.

As of version 0.7.1, proptest has optional "fork" and "timeout" features
(both enabled by default), which make it possible to run your test cases in
a subprocess and limit how long they may run. This is generally slower,
may make using a debugger more difficult, and makes test output harder to
interpret, but allows proptest to find and minimise test cases for these
situations as well.

Set the `fork` and/or `timeout` fields on `Config`; a nonzero timeout implies forking. Typed properties additionally require an explicit transport.

## Typed properties

Call `proptest::strict::ensure_property_with_transport` with a `PropertyTransport<V, A, E>`, where `V` is the strategy's native case and the callback returns `Result<A, E>`. The codec defines representations for cases, successful evidence, concrete assertion failures, and its own typed errors. It also restores strategy state when a child replays a completed or interrupted case. Ordinary in-process properties need no codec or serialization bounds.

The parent supervises children and decodes their versioned, framed records without invoking the property callback. Successful values and failures both reach the returned report. Partial payloads never become completed evaluations; malformed framing, codec errors, native I/O errors, child termination, and timeout remain distinguishable, preserving all already-decoded evidence. A child that cannot return does not produce an invented assertion failure.

Set `Config::test_name` to the test's full harness name so child re-execution selects the same property. `#[property_test]` supplies that name automatically and accepts `transport = CodecType => codec_expression` together with its `config` option. Typed entry points without a codec reject fork or timeout settings before the first callback.

The complete `proptest/examples/fib.rs` example declares a fixed-width codec for `(input, Option<fib_value>)` and its native predicate failure. It deliberately uses exponential recursion to demonstrate child interruption and shrinking. Run it with `cargo run -p proptest --example fib`; failure is expected. A codec's faithful domain must include every value and failure the property promises to return. Process-local resource owners require a deliberate transferable representation.

## Legacy runner

The legacy `TestRunner::run` and `proptest!` contract uses its existing marker replay protocol. This example uses both settings:

```rust,should_panic
# extern crate proptest;
use proptest::prelude::*;

// The worst possible way to calculate Fibonacci numbers
fn fib(n: u64) -> u64 {
    if n <= 1 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        // Setting both fork and timeout is redundant since timeout implies
        // fork, but both are shown for clarity.
        fork: true,
        timeout: 100,
        # cases: 1, // Need to set this to 1 to avoid doctest running forever
        .. ProptestConfig::default()
    })]
    #[test]
    # fn dummy(0..1) {} // Doctests don't build `#[test]` functions, so we need this
    fn test_fib(n: u64) {
        // For large n, this will variously run for an extremely long time,
        // overflow the stack, or panic due to integer overflow.
        assert!(fib(n) >= n);
    }
}
# fn main() { test_fib(); }
```

The exact value of the test failure depends heavily on the performance of
the host system, the rust version, and compiler flags, but on the system
where it was originally tested, it found that the maximum value that
`fib()` could handle was 39, despite having dozens of processes dump core
due to stack overflow or time out along the way.

If you just want to run tests in subprocesses or with a timeout every now
and then, you can do that by setting the `PROPTEST_FORK` or
`PROPTEST_TIMEOUT` environment variables to alter the default
configuration. Typed properties still need their explicit codec when these environment variables enable isolation. For example, on Unix,

```sh
# Run all the proptest tests in subprocesses with no timeout.
# Individual tests can still opt out by setting `fork: false` in their
# own configuration.
PROPTEST_FORK=true cargo test
# Run all the proptest tests in subprocesses with a 1 second timeout.
# Tests can still opt out or use a different timeout by setting `timeout: 0`
# or another timeout in their own configuration.
PROPTEST_TIMEOUT=1000 cargo test
```
