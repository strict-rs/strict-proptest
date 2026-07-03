fn main() {}

#[proptest::property_test]
fn unit_body(x: i32) {
    let _ = x;
}
