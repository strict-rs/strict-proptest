use proptest::prelude::*;

prop_compose! {
    [extern "C"] fn old_modifier()(sample in 0_i32..1) -> i32 {
        sample
    }
}

fn main() {}
