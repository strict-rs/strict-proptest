# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/examples/` — runnable example binaries that accompany the Proptest Book (the mdBook under the repo-root `book/`). Each one backs a specific chapter under `book/src/proptest/` (`getting-started.md`, `forking.md`) or `book/src/proptest/tutorial/`, and those chapters reproduce the example's code and sometimes its exact program output, so an example and its chapter must be changed together. For the crate as a whole see the parent `proptest/AGENTS.md`; for the workspace-wide build/test/lint matrix, no_std rules, copyright headers and commit conventions see the workspace-root `AGENTS.md`.

## Running

```sh
cargo run -p proptest --example fib        # or any name below
```

- `cargo test -p proptest` *compiles* every example (so they must build), but it does not run or assert them — the behaviour each one demonstrates is observed at runtime via `cargo run`, not by the test suite.
- Each `proptest!` block here deliberately omits `#[test]` (the files note this in an `NB` comment) so that `main()` can call the generated test function directly when the example runs as a binary.

## Examples — what each shows, and whether it passes

Important: three of these examples are *meant to panic / abort when run* — that is the whole point: proptest finds a counterexample and shrinks it to a minimal failing input. Do not "fix" the seeded bug to make the binary exit cleanly; the bug is the lesson, the dateparser bugs are even flagged inline with `// !`, and the book quotes the resulting failure output. The intentionally-failing ones are `dateparser_v1.rs`, `dateparser_v2.rs`, and `fib.rs`; the rest just print output and pass.

- `config-defaults.rs` (PASS) — prints `{:?}` of `proptest::test_runner::Config::default()`. Under the default `std` feature that default is environment-resolved (`contextualize_config`), so the output reflects any `PROPTEST_*` overrides in the environment — e.g. `PROPTEST_CASES=42 cargo run -p proptest --example config-defaults` shows `cases: 42`. A quick way to inspect the runner's effective configuration. Not tied to a specific book chapter.

- `dateparser_v1.rs` (FAIL by design — `book/src/proptest/getting-started.md`, first half) — a naive `parse_date` that byte-slices a length-10 `&str` without checking UTF-8 char boundaries. The `doesnt_crash(s in "\\PC*")` property feeds it arbitrary non-control strings; a multi-byte string whose *byte* length is 10 gets sliced mid-character and panics. proptest shrinks to a minimal crashing input (the book shows `s = "aAௗ0㌀0"`). `doesnt_crash` is the only property, so the observed failure is this panic. The two seeded bugs are marked `// !`.

- `dateparser_v2.rs` (FAIL by design — `getting-started.md`, second half) — adds `if !s.is_ascii() { return None; }`, which fixes the crash, so `doesnt_crash` and the added `parses_all_valid_dates` (input `"[0-9]{4}-[0-9]{2}-[0-9]{2}"`) both pass. But it keeps the `let month = &s[6..7]; // !` bug (one digit instead of `&s[5..7]`), and the new round-trip / oracle property `parses_date_back_to_original` catches it: any month >= 10 fails to round-trip, and proptest shrinks to the minimal `y = 0, m = 10, d = 1` — the book quotes exactly this, citing `examples/dateparser_v2.rs:46`. Demonstrates an oracle property finding a logic bug the crash-only property missed.

- `fib.rs` (FAIL by design — `book/src/proptest/forking.md`) — a deliberately exponential recursive `fib`, run under `#![proptest_config(ProptestConfig { fork: true, timeout: 1000, .. })]` with `assert!(fib(n) >= n)` over `n in prop::num::u64::ANY`. Large `n` runs far too long (caught by the per-case `timeout`), overflows the stack, or panics on integer overflow; `fork` isolates each case in its own subprocess so the run survives those crashes and still shrinks. Demonstrates the `fork`/`timeout` features. The whole module is behind `#[cfg(feature = "timeout")]` *only* so CI can also build with that feature off — the inline comment says you don't need that guard in your own code, and that `timeout` implies `fork`. `timeout` is in the default feature set, so plain `cargo run -p proptest --example fib` exercises it.

- `tutorial-simplify-play.rs` (PASS — `book/src/proptest/tutorial/shrinking-basics.md`) — drives a `ValueTree` by hand: builds one from a regex string strategy via `Strategy::new_tree(&mut runner)`, prints `current()`, then loops on `simplify()` printing each step to show shrinking walking toward a simpler value. Self-labelled "*not* how proptest is normally used".

- `tutorial-strategy-play.rs` (PASS — `book/src/proptest/tutorial/strategy-basics.md`) — the most basic generation demo: builds two `ValueTree`s (an `i32` range `0..100` and a regex string), prints their `current()` values. No shrinking, no assertions. Also self-labelled as not normal usage.
