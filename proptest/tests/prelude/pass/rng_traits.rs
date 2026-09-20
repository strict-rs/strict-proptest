use proptest::prelude::*;

fn draw_from_prelude_rng<R: Rng + ?Sized>(rng: &mut R) -> u8 {
    rng.random_range(1..4)
}

fn main() -> Result<(), strict_test_support::PredicateFailure<u8>> {
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let value = draw_from_prelude_rng(runner.rng());

    strict_test_support::ensure_that(
        value,
        "prelude users can name Rng and call RngExt methods",
        |value| (1..4).contains(value),
    ).map(drop)
}
