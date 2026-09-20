// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// revisions: stable nightly

use proptest_derive::Arbitrary;

fn main() {}

#[derive(Debug)]
struct T0;

#[derive(Debug, Arbitrary)]
//[stable]~^ the trait bound `T0: Arbitrary` is not satisfied [E0277]
//[nightly]~^^ the trait bound `T0: Arbitrary` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T1 as Arbitrary>::Parameters == _` [E0271]
//[nightly]~| type mismatch resolving `<T1 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<T0 as Arbitrary>::Strategy, fn(T0) -> T1>` is not well-formed
struct T1 {
    f0: T0,
    //[stable]~^ the trait bound `T0: Arbitrary` is not satisfied [E0277]
    //[nightly]~^^ the trait bound `T0: Arbitrary` is not satisfied [E0277]
}
