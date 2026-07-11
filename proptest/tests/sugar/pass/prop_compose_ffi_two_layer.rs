use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;

prop_compose_ffi! {
    fn bounded_span(base: i32)
        (upper in base + 1..base + 8)
        (lower in base..upper, upper in Just(upper))
    with extern "C" fn span(lower: i32, upper: i32) -> i32 {
        upper - lower
    }
    call span(lower, upper);
}

fn main() -> proptest::strict::TestResult {
    let strategy = bounded_span(3);
    let mut runner = TestRunner::deterministic();
    let value = strict_test_support::ensure_some(
        strategy.new_tree(&mut runner).ok(),
        "two-layer ffi strategy generates a value tree",
    )?
    .current();

    strict_test_support::ensure(
        (1..8).contains(&value),
        "the C-ABI mapper receives dependent generated scalar values",
    )
}
