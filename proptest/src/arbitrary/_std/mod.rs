//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for libstd.

macro_rules! std_arbitrary_with_params {
    ($typ: ty, $strat: ty, $params: ty; $args: ident => $logic: expr) => {
        arbitrary!([] $typ, $strat, $params; $args => $logic);
    };
}

macro_rules! std_wrap_ctor_default {
    ($wrap: ident) => {
        wrap_ctor!($wrap, $wrap::new);
    };
}

mod env;
mod ffi;
mod fs;
mod io;
mod net;
mod panic;
mod path;
mod string;
mod sync;
mod thread;
mod time;
