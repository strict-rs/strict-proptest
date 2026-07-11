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

fn main() -> proptest::strict::TestResult {
    let config = Config {
        failure_persistence: None,
        ..Config::default()
    };

    strict_test_support::ensure(
        proptest!(&config, |(value in 0_u32..1)| {
            let _observed = value;
        })
        .is_ok(),
        "public closure form runs through the exported macro",
    )?;

    strict_test_support::ensure(
        proptest!(&config, |(value: u8)| {
            let _observed = value;
        })
        .is_ok(),
        "public configured typed closure form runs through the exported macro",
    )
}
