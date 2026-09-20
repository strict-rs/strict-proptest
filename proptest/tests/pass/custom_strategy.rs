fn main() {}

#[proptest::property_test]
fn digits_only(
    #[strategy = "[0-9]{1,8}"] s: String,
) -> Result<String, strict_test_support::PredicateFailure<String>> {
    strict_test_support::ensure_that(
        s,
        "the regex strategy yields only ASCII digits",
        |s| s.chars().all(|c| c.is_ascii_digit()),
    )
}
