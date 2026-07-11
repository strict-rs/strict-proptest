use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;

prop_compose_ffi! {
    fn typed_window(offset: u8)
        (sample: u8)
        (window in 0_u8..=sample, sample in Just(sample))
    with extern "C" fn add_window(sample: u8, window: u8, offset: u8) -> u16 {
        u16::from(sample) + u16::from(window) + u16::from(offset)
    }
    call add_window(sample, window, offset);
}

fn main() -> proptest::strict::TestResult {
    let strategy = typed_window(7);
    let mut runner = TestRunner::deterministic();
    let value = strict_test_support::ensure_some(
        strategy.new_tree(&mut runner).ok(),
        "typed ffi strategy generates a value tree",
    )?
    .current();

    strict_test_support::ensure(
        value >= 7,
        "the typed C-ABI mapper receives generated values and builder arguments",
    )
}
