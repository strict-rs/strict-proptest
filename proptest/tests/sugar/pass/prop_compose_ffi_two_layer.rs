use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;

prop_compose_ffi! {
    fn bounded_span(base: i32)
        (upper in base.saturating_add(1)..base.saturating_add(8))
        (lower in base..upper, upper in Just(upper))
    with extern "C" fn span(lower: i32, upper: i32) -> i32 {
        upper.saturating_sub(lower)
    }
    call span(lower, upper);
}

fn main() -> Result<(), strict_test_support::PredicateFailure<Result<i32, proptest::test_runner::Reason>>> {
    let strategy = bounded_span(3);
    let mut runner = TestRunner::deterministic();
    let value = strategy.new_tree(&mut runner).map(|tree| tree.current());
    strict_test_support::ensure_that(value, "the C-ABI mapper receives dependent generated scalar values", |value| value.as_ref().is_ok_and(|value| (1..8).contains(value))).map(drop)
}
