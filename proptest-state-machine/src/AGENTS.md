# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest-state-machine/src/` — the two modules that implement sequential state-machine testing. `lib.rs` is only `pub use strategy::*;` / `pub use test_runner::*;` plus a doc pointer to the Proptest Book's "State Machine testing" chapter. This is a leaf; crate-level deps and test commands live in `../AGENTS.md`, and shared workspace conventions in the workspace-root `AGENTS.md`.

## `strategy.rs` — the model and the sequence strategy

### `ReferenceStateMachine` (the model you implement)

`trait ReferenceStateMachine: 'static` with associated types `State: Clone + Debug` and `Transition: Clone + Debug` (typically `Transition` is an enum carrying each transition's params). Required methods:

- `init_state() -> BoxedStrategy<Self::State>` — strategy for the initial model state; use `proptest::strategy::Just` for a constant.
- `transitions(state: &Self::State) -> BoxedStrategy<Self::Transition>` — strategy for the transitions valid *from* `state` (despite the doc string saying "initial"). Making the valid set depend on the current state is exactly what distinguishes this from generating a random transition sequence.
- `apply(state: Self::State, transition: &Self::Transition) -> Self::State` — advance the model.

Provided methods (rarely overridden except `preconditions`):

- `preconditions(state, transition) -> bool` (default `true`) — gate which transitions are valid from a state. Checked both during generation and on *every* shrink candidate (see `check_acceptable`). It only needs to encode state-*dependent* invariants that shrinking could break; state-independent invariants don't belong here. Generation rejects a failing transition with `runner.reject_local(...)`, so preconditions that are hard to satisfy slow the test or abort it once the local reject budget is exceeded.
- `sequential_strategy(size: impl Into<SizeRange>) -> SequentialStrategy<Self::State, Self::Transition>` — builds `Sequential::new(size.into(), Self::init_state, Self::preconditions, Self::transitions, Self::apply)`.

`SequentialStrategy<State, Transition>` is the alias `Sequential<State, Transition, BoxedStrategy<State>, BoxedStrategy<Transition>>`; everything is boxed because trait-level `impl Strategy` associated types aren't stable yet (the in-code TODO references rust-lang/rust#63063).

### `Sequential` / `SequentialValueTree` (generation + shrinking)

`Sequential` holds `size: SizeRange` plus `Arc<dyn Fn ...>` copies of `init_state` / `preconditions` / `transitions` / `next` (its `Debug` prints only `size`). Its `Strategy::Value` is the triple `(State, Vec<Transition>, Option<Arc<AtomicUsize>>)` — initial state, the transition sequence, and a shared "seen" counter (below). `SequentialValueTree`'s hand-written `Debug` renders the shrink cursor (`is_initial_state_shrinkable`, the transition count and the included/shrinkable bit-set counts, `max_ix`, `shrink`/`last_shrink`) via `finish_non_exhaustive`, carrying no `Debug` bounds on its generics because it prints none of the generic or `Arc<dyn Fn>` fields.

`new_tree`:

- generate the initial-state tree; remember `last_valid_initial_state = initial_state.current()`.
- pick `max_size = sample_uniform_incl(runner, min, end)` from `size.start_end_incl()`.
- sample transitions until `max_size` are accepted: for each, `new_tree` the `transitions(&state)` strategy, and if `preconditions(&state, &t)` holds push it and advance `state` via `next`, else `reject_local`.
- seed `included_transitions` / `shrinkable_transitions` as `VarBitSet::saturated(max_size)` (from `proptest::bits`), set the first shrink to `DeleteTransition(max_ix)` (back of the list, least likely to break preconditions), and `seen_transitions_counter` to `Some(0)`.

The value tree tracks: `transitions` (the per-transition `ValueTree`s), `acceptable_transitions: Vec<(TransitionState, Transition)>` (the last precondition-accepted value of each transition), the two bit-sets, `last_valid_initial_state`, `shrink` / `last_shrink`, and `seen_transitions_counter`. Two private enums drive it: `Shrink` = `InitialState | DeleteTransition(usize) | Transition(usize)`, and `TransitionState` = `Accepted | SimplifyRejected | ComplicateRejected`.

### Shrink phases (`simplify` / `try_simplify`)

`simplify()` delegates to `try_simplify()` while `can_simplify()` is true (initial state still shrinkable, or some included transition not yet fully rejected); otherwise, if the last op was a `Transition`, it runs the `try_to_find_acceptable_transition` fallback (a wrapping scan for an included transition whose current value newly passes preconditions). The phases, in order:

0. One-shot unseen-deletion (first `simplify()` only): if the test saw fewer transitions than are included, clear the never-executed tail from both bit-sets, then jump `shrink` to `InitialState` / `Transition(0)` / `DeleteTransition(seen - 2)` for 0 / 1 / >1 transitions seen (the last *seen* transition is the one that failed and is always kept). This step is deliberately **not** undone by `complicate`, so replays stay deterministic.
1. `DeleteTransition(ix)`: clear bit `ix` and step toward the front; if `check_acceptable` then fails, restore the bit and retry; accepted deletes also clear `shrinkable_transitions[ix]`. Reaching `ix == 0` advances to `Transition(0)`.
2. `Transition(ix)`: shrink individual transitions front-to-back, wrapping via `next_shrink_transition` (loops back to `Transition(0)` past `max_ix`, because shrinking an earlier transition can make a later one acceptable). `transitions[ix].simplify()` is committed to `acceptable_transitions[ix]` only if `check_acceptable(Some(ix), ...)` passes; otherwise the slot is marked `SimplifyRejected` and dropped from `shrinkable_transitions`. Exhausting `shrinkable_transitions` advances to `InitialState`.
3. `InitialState`: `initial_state.simplify()`, accepted only if the whole sequence still satisfies preconditions from the new state (then `last_valid_initial_state` updates). When it can't shrink further, set `is_initial_state_shrinkable = false` and return `false` — shrinking is done.

`complicate()` undoes `last_shrink`: re-include a deleted transition (then stop), or `complicate()` the transition / initial state and re-validate, keeping `last_shrink` so it can be complicated again. Phase-0 deletions are never complicated.

`check_acceptable(ix, state)` is the precondition re-check: it replays the currently-included transitions (substituting `transitions[ix].current()` when `ix` is `Some`) from `state`, advancing with `next` and requiring `preconditions` at each step. This is *why* deleting/shrinking is safe — every candidate sequence is re-validated against the preconditions, which is why the model only needs to encode state-dependent ones.

### The seen-counter

The triple's third element is `Option<Arc<AtomicUsize>>`, the out-of-band channel between the strategy and the runner. `test_sequential` increments it once per transition, *before* applying, so after a failing run it equals the number of transitions reached (the failing one included). The first `simplify()` consumes it for phase 0; both `simplify()` and `complicate()` then set the field to `Default::default()` — which for `Option<_>` is `None`, not a fresh counter — so the unseen-deletion runs exactly once and every later shrink run passes `None` (no increment). `current()` returns `(last_valid_initial_state, the included accepted transitions, a clone of the counter)` and panics with the message `Unexpected non-zero seen_transitions_counter` if read while the counter is `Some` and non-zero, guarding against re-reading an already-executed value without simplifying.

## `test_runner.rs` — running against the SUT

### `StateMachineTest` (the system you implement)

`trait StateMachineTest` with `type SystemUnderTest` (the concrete state) and `type Reference: ReferenceStateMachine`. Methods:

- `init_test(ref_state) -> SystemUnderTest` — build the SUT; mirror a non-constant initial model state here.
- `apply(state, ref_state, transition) -> Result<SystemUnderTest, TestFailure>` — apply to the SUT and check post-conditions via the `strict_test_support` `ensure*` vocabulary (return `Err(TestFailure)` instead of asserting). `ref_state` is the model state *after* the transition.
- `check_invariants(&state, &ref_state) -> proptest::strict::TestResult` (default `Ok(())`) — run after every transition (and once on the initial state).
- `teardown(state, ref_state) -> proptest::strict::TestResult` (default drops both and returns `Ok(())`) — per-case cleanup.
- `test_sequential(config: Config, ref_state, transitions, seen_counter: Option<Arc<AtomicUsize>>) -> proptest::strict::TestResult` — the driver (rarely overridden).

`test_sequential` loop: `init_test`, `check_invariants(...)?` on the initial state, then per transition — bump `seen_counter` if `Some`, advance the **model first** (`Reference::apply`), then the SUT (`concrete_state = Self::apply(...)?`, which receives the post-transition `ref_state` and the transition by value), then `check_invariants(...)?` — and finally `teardown(...)?; Ok(())`. A failing check short-circuits the sequence with its `TestFailure`; advancing the model before the SUT is what lets `apply` / `check_invariants` compare the SUT against the post-transition reference.

### `prop_state_machine!`

`#[macro_export]` macro; syntax `fn name(sequential <size-range> => TestType[<ty params>]);`, with an optional leading `#![proptest_config($cfg)]` and standard `#[meta]` / `#[test]` attributes. Each declared test expands to an ordinary fn (the caller's `#[test]` rides along in the metas) returning `::proptest::strict::TestResult`: it builds `sequential_strategy(size)` and passes it to `::proptest::strict::ensure_property(&strategy, stringify!(name), |(initial_state, transitions, seen_counter)| TestType::test_sequential(config, initial_state, transitions, seen_counter))`. `config` is `$cfg.__sugar_to_owned()` in the config arm, else `Config::default()`, constructed inside the closure per case — it feeds only `test_sequential`'s verbose logging, since `ensure_property` builds its own strict runner (deterministic `STRICT_TEST_SEED` seeding, persistence off). Only `sequential` is supported; the in-source doc example shows the real strict expansion.

## Feature gating & no_std

The only `#[cfg(feature = "std")]` is in `test_sequential`, gating the `INFO_LOG` / `eprintln!` verbose output (the no_std arms consume the otherwise-unused values — `drop((config, trans_len))`, and `let _ = ix` for the `Copy` index). Otherwise `strategy.rs` uses `std::sync::{Arc, atomic}` directly (only `Vec` / `fmt` come from `proptest::std_facade`), so the strategy module is effectively std-only. `VarBitSet` / `BitSetLike` come from `proptest::bits` (the parent crate enables proptest's `bit-set` feature).

## Where the behavior is exercised

`#[cfg(test)] mod test` in `strategy.rs` defines a `HeapStateMachine` reference model and drives the shrink algorithm directly (`number_of_sequential_value_tree_simplifications`, `test_state_machine_sequential_value_tree`, plus the unseen-deletion and zero-seen optimization tests); `mod find_simplest_failure` shows driving a `TestRunner` manually and threading the 3-tuple by hand — its model's `apply` fails via `ensure`, and the manual closure converts the `TestFailure` through `TestCaseError::fail`, the same conversion the strict runner uses. `mod test::strict_runner_behavior` (strategy.rs) pins the strict verdicts end to end: an intentionally failing counter model returns `TestFailure::PropertyFalsified` whose report carries the minimal three-transition sequence (proving shrinking plus the seen-transition optimization), and a precondition-gated model passes with a mirrored SUT. `test_runner.rs`'s `mod macro_test` checks that both `prop_state_machine!` arms expand hygienically, and its `mod strict_behavior` proves a passing stack model runs through the real macro expansion and that a hand-built valid sequence returns `Ok(())` from `test_sequential`.
