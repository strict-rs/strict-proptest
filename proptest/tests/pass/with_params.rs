fn main() {}

#[proptest::property_test(
    config = proptest::test_runner::Config {
        cases: 10,
        ..Default::default()
    }
)]
fn no_trailing_comma(x: i32) -> proptest::strict::TestResult {
    strict_test_support::ensure_eq(&x, &x, "a value equals itself")
}

#[proptest::property_test(
    config = proptest::test_runner::Config {
        cases: 10,
        ..Default::default()
    }
)]
fn trailing_comma(x: i32,) -> proptest::strict::TestResult {
    strict_test_support::ensure_eq(&x, &x, "a value equals itself")
}
