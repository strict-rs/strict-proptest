## Unreleased

### Breaking Changes

- The minimum supported Rust version has been increased to 1.96.0.
- `prop_state_machine!` now expands to ordinary `#[test]` functions that
  return `proptest::strict::TestResult` and run the generated transition
  sequence through `proptest::strict::ensure_property`, instead of expanding
  to a `proptest!` block that panics on failure.
- `StateMachineTest::apply` now returns
  `Result<Self::SystemUnderTest, TestFailure>`, and `check_invariants`,
  `teardown`, and `test_sequential` return `proptest::strict::TestResult`.
  Post-condition and invariant failures propagate
  `strict_test_support::TestFailure` values instead of panicking, so
  implementations report failures with the `ensure*` helpers and `?`.
- Because the generated tests run through the strict runner, they seed deterministically by default (`STRICT_TEST_SEED` selects the seed: unset or unparseable pins `0x5EED`, `random` opts into OS entropy, an integer pins that seed) and no longer write `proptest-regressions/` files; the shrunk minimal failing transition sequence is carried in the returned `TestFailure::PropertyFalsified` report instead.

## 0.8.0

- Added Send + Sync bounds to `strategy:Sequential` ([\#640](https://github.com/proptest-rs/proptest/pull/640))

## 0.7.0

### Breaking Changes

- The minimum supported Rust version has been increased to 1.84.0. ([\#612](https://github.com/proptest-rs/proptest/pull/612))

### New Features

- Extended `Sequential` test definition to accept closures in its function fields. ([\#609](https://github.com/proptest-rs/proptest/pull/609))

### Other Notes

- Added license files to the crate. ([\#618](https://github.com/proptest-rs/proptest/pull/618))

## 0.6.0

### Breaking Changes

- The minimum supported Rust version has been increased to 1.82.0. ([\#605](https://github.com/proptest-rs/proptest/pull/605))

## 0.5.0

### New Features

- Added reference state machine argument to the teardown function to allow comparison against the SUT.
  ([\#595](https://github.com/proptest-rs/proptest/pull/595))

## 0.4.0

### Other Notes

- Set MSRV to 1.82, which is what minimally compiles and completes testing.
- Updated `rand` dependency from 0.8 to 0.9.

## 0.3.1

- Fixed checking of pre-conditions with a shrinked or complicated initial state.
  ([\#482](https://github.com/proptest-rs/proptest/pull/482))

## 0.3.0

### New Features

- Remove unseen transitions on a first step of shrinking.
  ([\#388](https://github.com/proptest-rs/proptest/pull/388))

## 0.2.0

### Other Notes

- `message-io` updated from 0.17 to 0.18

### Bug Fixes

- Removed the limit of number of transitions that can be deleted in shrinking that depended on the number the of transitions given to `prop_state_machine!` or `ReferenceStateMachine::sequential_strategy`.
- Fixed state-machine macro's inability to handle missing config
- Fixed logging of state machine transitions to be enabled when verbose config is >= 1. The "std" feature is added to proptest-state-machine as a default feature that allows to switch the logging off in non-std env.
- Fixed an issue where after simplification of the initial state causes the test to succeed, the initial state would not be re-complicated - causing the test to report a succeeding input as the simplest failing input.
