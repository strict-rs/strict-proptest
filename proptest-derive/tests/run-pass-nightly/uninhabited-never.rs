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
    _V2(!),
    _V3([!; 1]),
    _V4([!; 2 - 1]),
    _V5([!; 2 * 3]),
    V1,
}

macro_rules! tymac {
    ($ignore:ty) => {
        u8
    };
}

#[derive(Debug, Arbitrary)]
struct TyMac {
    _field: tymac!(!),
}

trait Fun {
    type Prj;
}

impl Fun for ! {
    type Prj = u8;
}

#[derive(Debug, Arbitrary)]
enum UsePrj {
    V0(<! as Fun>::Prj),
}

fn assert_arbitrary<T: Arbitrary>() {}

fn main() {
    assert_arbitrary::<Ty1>();
    assert_arbitrary::<TyMac>();
    assert_arbitrary::<UsePrj>();
}
