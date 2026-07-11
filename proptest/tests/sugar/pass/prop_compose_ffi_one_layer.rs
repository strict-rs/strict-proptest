use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;

prop_compose_ffi! {
    fn offset_sample(offset: i32)(sample in 0_i32..4)
    with extern "C" fn add_offset(sample: i32, offset: i32) -> i32 {
        sample + offset
    }
    call add_offset(sample, offset);
}

fn main() -> proptest::strict::TestResult {
    let strategy = offset_sample(10);
    let mut runner = TestRunner::deterministic();
    let value = strict_test_support::ensure_some(
        strategy.new_tree(&mut runner).ok(),
        "one-layer ffi strategy generates a value tree",
    )?
    .current();

    strict_test_support::ensure(
        (10..14).contains(&value),
        "the C-ABI mapper receives the generated sample and builder argument",
    )
}
