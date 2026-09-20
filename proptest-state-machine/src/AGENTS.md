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

The triple's third element is `Option<Arc<AtomicUsize>>`, the out-of-band channel between the strategy and the runner. `test_sequential` increments it once per transition, *before* applying, so after a failing run it equals the number of transitions reached (the failing one included). The first `simplify()` consumes it for phase 0; both `simplify()` and `complicate()` then set the field to `Default::default()` — which for `Option<_>` is `None`, not a fresh counter — so the unseen-deletion runs exactly once and every later shrink run passes `None` (no increment). `current()` returns `(last_valid_initial_state, the included accepted transitions, a clone of the counter)` without changing an already-observed counter. Typed fork codecs restore the seen count into the current case before the next shrink step; they must reject mismatched states, transitions, or counter presence.

## `test_runner.rs` — running against the SUT

### `StateMachineTest` (the system you implement)

`StateMachineTest` associates `SystemUnderTest`, `Reference: ReferenceStateMachine`, `Failure`, `TransitionEvidence`, and `InvariantEvidence`. Methods:

- `init_test(ref_state) -> SystemUnderTest` initializes the SUT.
- `apply(state, ref_state, transition) -> TransitionResult<Self>` returns `(SystemUnderTest, TransitionEvidence)` or the concrete hook failure. The reference state has already advanced.
- `check_invariants(&state, &ref_state) -> Result<InvariantEvidence, Failure>` is required and runs initially and after successful applications.
- `teardown(state, ref_state) -> Result<(), Failure>` consumes the final states; the default drops both. It runs only after the sequence succeeds.
- `test_sequential(config, ref_state, transitions, seen_counter) -> SequentialResult<Self>` preserves ordered observations and returns a boxed `SequentialFailure` on failure.

The driver increments the seen counter before applying a transition, advances the model before the SUT, checks invariants, and short-circuits on failure. `SequentialFailure` records the failing stage, original hook error, completed evidence, states still owned by the driver, and the unattempted iterator. A consuming application or teardown hook owns the resources passed into it; its error must retain resources it promises to return. The driver does not add recovery or run teardown on failure.

### `prop_state_machine!`

The syntax is `fn name(sequential <size-range> => TestType[<ty params>]);`, with optional `#![proptest_config($cfg)]` and caller attributes such as `#[test]`. Wrappers return `StateMachinePropertyResult<TestType>` and pass the sequence strategy to `ensure_property`.

The configured arm evaluates `$cfg.__sugar_to_owned()` inside each case and passes it through `strict_state_machine_config_from`; the other arm uses `strict_state_machine_config()`. This inner configuration controls driver behavior such as verbosity. The outer runner uses strict defaults, including `STRICT_TEST_SEED` and disabled persistence. Use `ensure_property_with_config` or `ensure_property_with_transport` directly with the sequence strategy when the outer runner needs explicit configuration or a model-specific codec.

## Feature gating & no_std

The `std` feature gates verbose driver diagnostics. The strategy uses `std::sync::{Arc, atomic}` directly, so this crate is not a standalone `no_std` implementation. `VarBitSet` and `BitSetLike` come from `proptest::bits`, enabled by the crate's proptest dependency.

## Where the behavior is exercised

`strategy.rs`'s adjacent tests exercise direct shrink walks, precondition preservation, unseen-tail deletion, zero-seen cases, and repeated `current()` observations. Its `find_simplest_failure` and `strict_runner_behavior` modules inspect native minimal failures and passing mirrored state machines. `strategy/test/strict_runner_behavior/transport.rs` supplies the concrete counting codec and checks transported success, minimal failing sequences, restoration of the shared counter, malformed frames, and native conversion errors.

`test_runner.rs`'s `macro_test` checks both macro arms. `strict_behavior` exercises a passing stack through the real macro and observes complete successful and failed sequential outcomes, including progress and unattempted transitions.
