fn main() {}

#[proptest::property_test]
fn my_test(x: i32) -> Result<(i32, i32), strict_test_support::ComparisonFailure<i32, i32>> {
    strict_test_support::ensure_eq(x, x, "a value equals itself")
}
