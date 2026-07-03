fn main() {
    let _ = proptest::arbitrary::any::<std::sync::RwLock<u8>>();
}
