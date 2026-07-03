# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-state-machine/` (v0.8.0) — the crate root for state-machine / model-based testing built on top of `proptest`. For shared workspace conventions (toolchain, formatting, the no_std feature matrix, MSRV, commit style) see the workspace-root `AGENTS.md`.

## What this is

A `Strategy` plus a convenience runner macro for *sequential* state-machine testing: you describe an abstract model and a real system, and the crate generates random transition sequences, drives both, and **shrinks** a failing sequence down to a minimal reproduction. The mental model is two traits:

- `ReferenceStateMachine` — the abstract model: what *should* happen (and which transitions are valid from a given state).
- `StateMachineTest` — the real system under test: what *does* happen, checked against the model after each transition.

The `prop_state_machine!` macro expands to ordinary `#[test]` functions returning `proptest::strict::TestResult` that run the model's sequence strategy through `proptest::strict::ensure_property`; a falsified run comes back as `TestFailure::PropertyFalsified` carrying the shrunk minimal transition sequence, and the `StateMachineTest` methods (`apply`, `check_invariants`, `teardown`, `test_sequential`) return/propagate `TestFailure` instead of panicking. The user guide is the Proptest Book's "State Machine testing" chapter (<https://proptest-rs.github.io/proptest/proptest/state-machine.html>).

## Deeper guides (don't duplicate these here)

- `src/AGENTS.md` — the `ReferenceStateMachine` / `StateMachineTest` traits, the `Sequential` sequence strategy and its shrink runner, and the `prop_state_machine!` macro.
- `examples/AGENTS.md` — the `state_machine_heap` and `state_machine_echo_server` worked examples.

## Features & dependencies

- Features: `default = ["std"]`, and `std = ["proptest/std"]` (the `std` feature only gates verbose transition logging; the strategy itself is effectively std-only — see `src/AGENTS.md`).
- Depends on `proptest` with `fork`, `timeout`, and `bit-set` enabled. `bit-set` is used directly: the shrink bookkeeping tracks which transitions are still included/shrinkable via bit-sets from `proptest::bits`. `fork` (per-case process isolation) and `timeout` (per-case time limit; `timeout` requires `fork`) aren't referenced by this crate's own code — they're enabled so a state-machine test can opt into those `Config` options, which matters because a single case runs an entire transition sequence against a real system (e.g. the echo-server example does blocking socket I/O that can hang).
- Dev-dependency `message-io` (workspace `0.19.0`, features `tcp`/`udp`/`websocket`) is used only by the echo-server example. Dev-dependency `strict-test-support` provides the `ensure*` vocabulary (`ensure`, `ensure_eq`, `ensure_some`, …) used by the crate's tests and examples.
- `proptest-regressions/` holds checked-in failing seeds for this crate's tests — don't delete it. (The examples set `failure_persistence: None`, so running them writes nothing there.)

## Build & test

```sh
cargo test -p proptest-state-machine                       # crate test suite
cargo run  -p proptest-state-machine --example state_machine_heap
```

Examples are example *binaries*, not tests, so `cargo test` does not run them — use `cargo run --example …`. For the full feature / no_std build matrix and the toolchain & formatting commands, defer to the workspace-root `AGENTS.md`.
