fn return_value(x: i32, y: i32) -> Result<(i32, i32), ComparisonFailure<i32, i32>> {
    ensure_eq(x, y, "values agree")
}
