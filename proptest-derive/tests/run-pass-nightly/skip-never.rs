// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(never_type)]

extern crate proptest;
extern crate proptest_derive;

use proptest::arbitrary::Arbitrary;
use proptest_derive::Arbitrary;

#[derive(Debug, Arbitrary)]
enum Ty1 {
    V1,
    _V2(!),
    #[proptest(skip)]
    _V3,
}

#[derive(Debug, Arbitrary)]
enum Ty2 {
    V1,
    V2,
    #[proptest(skip)]
    _V3,
    #[proptest(skip)]
    _V4,
}

fn assert_arbitrary<T: Arbitrary>() {}

fn main() {
    assert_arbitrary::<Ty1>();
    assert_arbitrary::<Ty2>();
}
