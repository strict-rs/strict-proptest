fn main() {}

#[proptest::property_test(proptest_path = actually::a::function())]
fn invalid_proptest_path(x: i32) -> Result<(), core::convert::Infallible> {
    let _ = x;
    Ok(())
}
