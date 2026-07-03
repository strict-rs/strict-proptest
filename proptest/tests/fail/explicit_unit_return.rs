fn main() {}

#[proptest::property_test]
fn explicit_unit_return(x: i32) -> () {
    let _ = x;
}
