#[cfg(test)]
mod tests {
    use proptest::test_runner::{Config, PropertyResult, TestRunner};

    #[test]
    fn persists_failure_beside_its_crate() -> PropertyResult<u32, (), u32> {
        TestRunner::new(Config::with_source_file(file!()))
            .run_typed(&(0_u32..100), Err)
    }
}
