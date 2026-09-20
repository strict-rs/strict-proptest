use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;

prop_compose_ffi! {
    fn typed_window(offset: u8)
        (sample: u8)
        (window in 0_u8..=sample, sample in Just(sample))
    with extern "C" fn add_window(sample: u8, window: u8, offset: u8) -> u16 {
        u16::from(sample).saturating_add(u16::from(window)).saturating_add(u16::from(offset))
    }
    call add_window(sample, window, offset);
}

fn main() -> Result<(), strict_test_support::PredicateFailure<Result<u16, proptest::test_runner::Reason>>> {
    let strategy = typed_window(7);
    let mut runner = TestRunner::deterministic();
    let value = strategy.new_tree(&mut runner).map(|tree| tree.current());
    strict_test_support::ensure_that(value, "the typed C-ABI mapper receives generated values and builder arguments", |value| value.as_ref().is_ok_and(|value| *value >= 7)).map(drop)
}
