fn foo((x, y): (i32, i32)) -> ::proptest::strict::TestResult {
    let _product = x * y;
    Ok(())
}
