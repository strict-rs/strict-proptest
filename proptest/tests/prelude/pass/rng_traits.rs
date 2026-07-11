use proptest::prelude::*;

fn draw_from_prelude_rng<R: Rng + ?Sized>(rng: &mut R) -> u8 {
    rng.random_range(1..4)
}

fn main() -> proptest::strict::TestResult {
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let value = draw_from_prelude_rng(runner.rng());

    strict_test_support::ensure(
        (1..4).contains(&value),
        "prelude users can name Rng and call RngExt methods",
    )
}
