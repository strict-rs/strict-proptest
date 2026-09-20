fn main() {}

// The generated code must reach every proptest item — including the strict
// runner — through the `proptest_path` override, never through a hard-coded
// `::proptest`.
extern crate proptest as aliased_proptest;

#[aliased_proptest::property_test(proptest_path = ::aliased_proptest)]
fn through_aliased_path(x: u8) -> Result<(u8, u8), strict_test_support::ComparisonFailure<u8, u8>> {
    strict_test_support::ensure_eq(x, x, "a value equals itself")
}
