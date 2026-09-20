// Eleven alternatives force `prop_oneof!`'s general arm, which expands to
// `Union::new_weighted($crate::std_facade::vec![...])`. Compiling and running
// this fixture from an external crate pins that the facade macro path
// resolves outside the defining crate; the body then checks the union's real
// generation behavior instead of only compiling it.

use std::collections::BTreeSet;

use proptest::strategy::{Just, Strategy, ValueTree};
use proptest::test_runner::TestRunner;

fn main() -> Result<(), impl std::fmt::Debug> {
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
    let samples: Vec<_> = (0..1024).map(|_| strategy.new_tree(&mut runner).map(|tree| tree.current())).collect();
    let property = proptest::strict::ensure_property(&strategy, "prop_oneof_general_arm", |value| {
        strict_test_support::ensure_that(value, "generated values stay within the listed alternatives", |value| *value <= 10)
    });
    strict_test_support::ensure_that((samples, property), "every general-arm alternative is generated and the strict property passes", |(samples, property)| {
        samples.iter().all(Result::is_ok) && samples.iter().filter_map(|result| result.as_ref().ok()).copied().collect::<BTreeSet<_>>()
            == (0u8..=10).collect::<BTreeSet<_>>() && property.is_ok()
    }).map(drop)
}
