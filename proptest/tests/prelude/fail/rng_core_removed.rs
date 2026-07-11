use proptest::prelude::*;

fn require_removed_rng_core<R: RngCore + ?Sized>(_rng: &mut R) {}

fn main() {
    let _strategy = Just(0_u8);
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    require_removed_rng_core(runner.rng());
}
