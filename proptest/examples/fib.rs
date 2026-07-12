//-
// Copyright 2018 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Demonstrates the `fork` and `timeout` features on a deliberately
//! exponential `fib`.
//!
//! Backs the Proptest Book's `forking` chapter. The `test_fib` property runs
//! the `fib(n) >= n` expectation over arbitrary `u64`, where large `n` runs
//! far too long, overflows the stack, or overflows integer arithmetic; `fork`
//! isolates each case in a subprocess and `timeout` bounds it, so the run
//! survives the crashes and still shrinks. Fails by design.

#[cfg(all(feature = "timeout", feature = "strict-test"))]
use proptest::num::u64::ANY;
#[cfg(all(feature = "timeout", feature = "strict-test"))]
use proptest::strict::TestFailure;
#[cfg(all(feature = "timeout", feature = "strict-test"))]
use proptest::strict::ensure_property_with_config;
#[cfg(all(feature = "timeout", feature = "strict-test"))]
use proptest::test_runner::Config;
#[cfg(all(feature = "timeout", feature = "strict-test"))]
use strict_test_support::ensure;
#[cfg(all(feature = "timeout", feature = "strict-test"))]
use strict_test_support::ensure_some;

// This #[cfg] is only here so that CI can test building proptest with the
// timeout feature disabled. You do not need it in your code.
// The worst possible way to calculate Fibonacci numbers
/// Calculate `fib(n)` recursively with deliberately exponential work.
#[cfg(all(feature = "timeout", feature = "strict-test"))]
fn fib(n: u64) -> Option<u64> {
  if n <= 1 {
    return Some(n);
  }

  let left = fib(n.saturating_sub(1))?;
  let right = fib(n.saturating_sub(2))?;
  left.checked_add(right)
}

#[cfg(all(feature = "timeout", feature = "strict-test"))]
/// Run the tutorial Fibonacci property under fork and timeout.
#[allow(
  clippy::single_call_fn,
  reason = "name the tutorial Fibonacci property that the example main runs"
)]
fn test_fib() -> Result<(), TestFailure> {
  ensure_property_with_config(
    &ANY,
    "the tutorial Fibonacci property holds",
    Config {
      // Setting both fork and timeout is redundant since timeout implies
      // fork, but both are shown for clarity.
      fork: true,
      timeout: 1000,
      ..Config::default()
    },
    |n| {
      // For large n, this will variously run for an extremely long time,
      // overflow the stack, or exceed `u64`.
      let fib_n = ensure_some(fib(n), "fibonacci value fits in u64")?;
      ensure(fib_n >= n, "the tutorial property expects fib(n) >= n")
    },
  )
}

#[cfg(all(feature = "timeout", feature = "strict-test"))]
fn main() -> Result<(), TestFailure> {
  test_fib()
}

#[cfg(not(all(feature = "timeout", feature = "strict-test")))]
fn main() {}
