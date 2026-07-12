//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Plays with value generation by hand: builds a `ValueTree` from a strategy
//! and reads its `current` value.
//!
//! Backs the Proptest Book's tutorial `strategy-basics` chapter. It calls
//! `Strategy::new_tree` on an `i32` range and a regex string strategy, then
//! prints each tree's generated value. This is not how proptest is normally
//! used; it only illustrates the generation layer.

// Shows how to pick values from a strategy.
//
// This is *not* how proptest is normally used; it is simply used to play
// around with value generation.

use std::error::Error;
use std::io;
use std::io::Write as _;

use proptest::strategy::Strategy as _;
use proptest::strategy::ValueTree as _;
use proptest::test_runner::TestRunner;

fn main() -> Result<(), Box<dyn Error>> {
  let mut runner = TestRunner::default();
  let int_val = (0..100_i32).new_tree(&mut runner)?;
  let str_val = "[a-z]{1,4}\\p{Cyrillic}{1,4}\\p{Greek}{1,4}".new_tree(&mut runner)?;
  writeln!(
    io::stdout().lock(),
    "int_val = {}, str_val = {}",
    int_val.current(),
    str_val.current()
  )?;
  Ok(())
}
