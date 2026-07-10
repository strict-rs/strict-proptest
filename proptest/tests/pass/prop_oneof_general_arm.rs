// Eleven alternatives force `prop_oneof!`'s general arm, which expands to
// `Union::new_weighted($crate::std_facade::vec![...])`. Compiling and running
// this fixture from an external crate pins that the facade macro path
// resolves outside the defining crate; the body then checks the union's real
// generation behavior instead of only compiling it.

use std::collections::BTreeSet;

use proptest::strategy::{Just, Strategy, ValueTree};
use proptest::test_runner::TestRunner;

fn main() -> proptest::strict::TestResult {
    let strategy = proptest::prop_oneof![
        1 => Just(0u8),
        1 => Just(1),
        1 => Just(2),
        1 => Just(3),
        1 => Just(4),
        1 => Just(5),
        1 => Just(6),
        1 => Just(7),
        1 => Just(8),
        3 => Just(9),
        3 => Just(10),
    ];

    let mut runner = TestRunner::deterministic();
    let mut seen = BTreeSet::new();
    for _ in 0..1024 {
        let tree = strict_test_support::ensure_some(
            strategy.new_tree(&mut runner).ok(),
            "the eleven-arm union generates a value tree",
        )?;
        let _newly_seen = seen.insert(tree.current());
    }
    let expected = (0u8..=10).collect::<BTreeSet<_>>();
    strict_test_support::ensure(
        seen == expected,
        "every alternative of the vec-backed union is generated",
    )?;

    proptest::strict::ensure_property(&strategy, "prop_oneof_general_arm", |value| {
        strict_test_support::ensure(
            value <= 10,
            "generated values stay within the listed alternatives",
        )
    })
}
