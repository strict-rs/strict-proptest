use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;

prop_compose_ffi! {
    fn offset_sample(offset: i32)(sample in 0_i32..4)
    with extern "C" fn add_offset(sample: i32, offset: i32) -> i32 {
        sample.saturating_add(offset)
    }
    call add_offset(sample, offset);
}

fn main() -> Result<(), strict_test_support::PredicateFailure<Result<i32, proptest::test_runner::Reason>>> {
    let strategy = offset_sample(10);
    let mut runner = TestRunner::deterministic();
    let value = strategy.new_tree(&mut runner).map(|tree| tree.current());
    strict_test_support::ensure_that(value, "the C-ABI mapper receives the generated sample and builder argument", |value| value.as_ref().is_ok_and(|value| (10..14).contains(value))).map(drop)
}
