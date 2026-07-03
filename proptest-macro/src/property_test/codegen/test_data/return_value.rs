// Test handling of a test function that spells the strict result type out
// rather than using the `::proptest::strict::TestResult` alias.

fn return_value(
    x: i32,
    y: i32,
) -> Result<(), ::proptest::strict::TestFailure> {
    let _ = (x, y);
    Ok(())
}
