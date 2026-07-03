fn main() {
    let _ = proptest::arbitrary::any::<std::sync::Mutex<u8>>();
}
