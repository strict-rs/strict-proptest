fn main() {}

struct MyTestArgs {
    something_else: String,
}

#[proptest::property_test]
fn my_test(x: i32) -> proptest::strict::TestResult {
    strict_test_support::ensure_eq(&x, &x, "a value equals itself")
}
