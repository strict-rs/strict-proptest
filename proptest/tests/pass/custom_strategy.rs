fn main() {}

#[proptest::property_test]
fn digits_only(
    #[strategy = "[0-9]{1,8}"] s: String,
) -> proptest::strict::TestResult {
    strict_test_support::ensure(
        s.chars().all(|c| c.is_ascii_digit()),
        "the regex strategy yields only ASCII digits",
    )
}
