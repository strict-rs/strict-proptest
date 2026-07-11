#![deny(improper_ctypes_definitions)]

use std::vec::Vec;

use proptest::prelude::*;

prop_compose_ffi! {
    fn rust_abi_mapper()(values in prop::collection::vec(0_i32..4, 1..3))
    with extern "C" fn vec_len(values: Vec<i32>) -> i32 {
        values.len() as i32
    }
    call vec_len(values);
}

fn main() {
    let _strategy = rust_abi_mapper();
}
