use proptest::prelude::*;
use proptest::test_runner::Config;

proptest! {
    #![proptest_config(Config {
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn public_block_macro_compiles(value in 0_u32..1) {
        let _observed = value;
    }
}

fn main() -> Result<(), impl std::fmt::Debug> {
    let config = Config {
        failure_persistence: None,
        ..Config::default()
    };

    let ranged = proptest!(&config, |(value in 0_u32..1)| { let _observed = value; });
    let typed = proptest!(&config, |(value: u8)| { let _observed = value; });
    strict_test_support::ensure_that((ranged, typed), "both public closure forms run through the exported macro", |(ranged, typed)| {
        ranged.is_ok() && typed.is_ok()
    }).map(drop)
}
