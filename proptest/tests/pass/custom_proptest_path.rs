fn main() {}

// The generated code must reach every proptest item — including the strict
// runner — through the `proptest_path` override, never through a hard-coded
// `::proptest`.
extern crate proptest as aliased_proptest;

#[aliased_proptest::property_test(proptest_path = ::aliased_proptest)]
fn through_aliased_path(x: u8) -> aliased_proptest::strict::TestResult {
    strict_test_support::ensure_eq(&x, &x, "a value equals itself")
}
