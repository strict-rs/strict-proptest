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

fn make_regex() -> &'static str {
    "a|b"
}

// struct:

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T0 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<() as proptest::string::StrategyFromRegex>::Strategy, fn(()) -> T0>` is not well-formed
struct T0 {
    #[proptest(regex = "a+")]
    f0: (),
    //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
    //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T1 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<u8 as proptest::string::StrategyFromRegex>::Strategy, fn(u8) -> T1>` is not well-formed
struct T1 {
    #[proptest(regex("a*"))]
    f0: u8,
    //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
    //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T2 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<Vec<u16> as proptest::string::StrategyFromRegex>::Strategy, fn(Vec<u16>) -> T2>` is not well-formed
struct T2 {
    #[proptest(regex(make_regex))]
    f0: Vec<u16>,
    //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
    //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T3 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<() as proptest::string::StrategyFromRegex>::Strategy, fn(()) -> T3>` is not well-formed
struct T3(
    #[proptest(regex = "a+")]
    (),
    //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
    //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
);

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T4 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<u8 as proptest::string::StrategyFromRegex>::Strategy, fn(u8) -> T4>` is not well-formed
struct T4(
    #[proptest(regex("a*"))]
    u8,
    //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
    //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
);

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T5 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<Vec<u16> as proptest::string::StrategyFromRegex>::Strategy, fn(Vec<u16>) -> T5>` is not well-formed
struct T5(
    #[proptest(regex(make_regex))]
    Vec<u16>,
    //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
    //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
);

// enum:

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T6 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<() as proptest::string::StrategyFromRegex>::Strategy, fn(()) -> T6>` is not well-formed
enum T6 {
    V0 {
        #[proptest(regex = "a+")]
        f0: (),
        //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
        //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
    }
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T7 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<u8 as proptest::string::StrategyFromRegex>::Strategy, fn(u8) -> T7>` is not well-formed
enum T7 {
    V0 {
        #[proptest(regex("a*"))]
        f0: u8,
        //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
        //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
    }
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T8 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<Vec<u16> as proptest::string::StrategyFromRegex>::Strategy, fn(Vec<u16>) -> T8>` is not well-formed
enum T8 {
    V0 {
        #[proptest(regex(make_regex))]
        f0: Vec<u16>,
        //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
        //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
    }
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T9 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<() as proptest::string::StrategyFromRegex>::Strategy, fn(()) -> T9>` is not well-formed
enum T9 {
    V0(
        #[proptest(regex = "a+")]
        (),
        //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
        //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
    )
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T10 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<u8 as proptest::string::StrategyFromRegex>::Strategy, fn(u8) -> T10>` is not well-formed
enum T10 {
    V0(
        #[proptest(regex("a*"))]
        u8,
        //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
        //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
    )
}

#[derive(Debug, Arbitrary)]
//[stable]~^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
//[nightly]~| type mismatch resolving `<T11 as Arbitrary>::Strategy == _` [E0271]
//[nightly]~| the type `proptest::strategy::Map<<Vec<u16> as proptest::string::StrategyFromRegex>::Strategy, fn(Vec<u16>) -> T11>` is not well-formed
enum T11 {
    V0(
        #[proptest(regex(make_regex))]
        Vec<u16>,
        //[stable]~^ StrategyFromRegex` is not satisfied [E0277]
        //[nightly]~^^ StrategyFromRegex` is not satisfied [E0277]
    )
}
