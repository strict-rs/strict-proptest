# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/examples/` — runnable example binaries that accompany the Proptest Book (the mdBook under the repo-root `book/`). Each one backs a specific chapter under `book/src/proptest/` (`getting-started.md`, `forking.md`) or `book/src/proptest/tutorial/`, and those chapters reproduce the example's code and sometimes its exact program output, so an example and its chapter must be changed together. For the crate as a whole see the parent `proptest/AGENTS.md`; for the workspace-wide build/test/lint matrix, no_std rules, copyright headers and commit conventions see the workspace-root `AGENTS.md`.

## Running

```sh
cargo run -p proptest --example fib        # or any name below
```

- `cargo test -p proptest` *compiles* every example (so they must build), but it does not run or assert them — the behaviour each one demonstrates is observed at runtime via `cargo run`, not by the test suite.
- The date-parser executables call the strict runner directly from `main()`. Their shared `dateparser/tutorial.rs` module owns field decoding, fixed cases, and the arbitrary-input property; each executable retains its own admission checks and property selection.

## Examples — what each shows, and whether it passes

Preserve intentionally failing properties: proptest finds a counterexample and shrinks it to a minimal failing input. Do not fix a seeded bug merely to make its executable exit successfully. The date parsers share the marked `6..7` month-slice bug; the book distinguishes its original panic-based walkthrough from the strict executables' native typed outcomes.

- `config-defaults.rs` (PASS) — prints `{:?}` of `proptest::test_runner::Config::default()`. Under the default `std` feature that default is environment-resolved (`contextualize_config`), so the output reflects any `PROPTEST_*` overrides in the environment — e.g. `PROPTEST_CASES=42 cargo run -p proptest --example config-defaults` shows `cases: 42`. A quick way to inspect the runner's effective configuration. Not tied to a specific book chapter.

- `dateparser_v1.rs` (PASS — `book/src/proptest/getting-started.md`, first half) — checks length before fallible byte-range decoding. The shared `doesnt_crash` property feeds it arbitrary non-control strings and retains every input and parser response. Invalid UTF-8 boundaries return `None`. Its fixed examples and crash-resistance property do not expose the intentional one-digit month bug.

- `dateparser_v2.rs` (FAIL by design — `getting-started.md`, second half) — adds an ASCII guard and the digit-shaped-date property over `"[0-9]{4}-[0-9]{2}-[0-9]{2}"`, both still using the shared field decoder. Its round-trip oracle exposes the one-digit month bug and shrinks to `(0, 10, 1)`. `main()` retains the fixed cases and all three complete property runs, then returns their typed assertion failure. No panic or regression-file persistence is required.

- `fib.rs` (FAIL by design — `book/src/proptest/forking.md`) — a deliberately exponential recursive `fib`, run under `#![proptest_config(ProptestConfig { fork: true, timeout: 1000, .. })]` with `assert!(fib(n) >= n)` over `n in prop::num::u64::ANY`. Large `n` runs far too long (caught by the per-case `timeout`), overflows the stack, or panics on integer overflow; `fork` isolates each case in its own subprocess so the run survives those crashes and still shrinks. Demonstrates the `fork`/`timeout` features. The whole module is behind `#[cfg(feature = "timeout")]` *only* so CI can also build with that feature off — the inline comment says you don't need that guard in your own code, and that `timeout` implies `fork`. `timeout` is in the default feature set, so plain `cargo run -p proptest --example fib` exercises it.

- `tutorial-simplify-play.rs` (PASS — `book/src/proptest/tutorial/shrinking-basics.md`) — drives a `ValueTree` by hand: builds one from a regex string strategy via `Strategy::new_tree(&mut runner)`, prints `current()`, then loops on `simplify()` printing each step to show shrinking walking toward a simpler value. Self-labelled "*not* how proptest is normally used".

- `tutorial-strategy-play.rs` (PASS — `book/src/proptest/tutorial/strategy-basics.md`) — the most basic generation demo: builds two `ValueTree`s (an `i32` range `0..100` and a regex string), prints their `current()` values. No shrinking, no assertions. Also self-labelled as not normal usage.
