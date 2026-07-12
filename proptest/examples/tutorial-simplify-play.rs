//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Plays with value generation and shrinking by hand: builds a `ValueTree`,
//! then walks it toward simpler values.
//!
//! Backs the Proptest Book's tutorial `shrinking-basics` chapter. It builds a
//! tree from a regex string strategy via `Strategy::new_tree`, prints
//! `current`, then loops on `simplify` printing each step. This is not how
//! proptest is normally used; it only illustrates the shrinking layer.

// Shows how to pick values from a strategy and simplify them.
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
  let mut str_val = "[a-z]{1,4}\\p{Cyrillic}{1,4}\\p{Greek}{1,4}".new_tree(&mut runner)?;
  let mut output = io::stdout().lock();
  writeln!(output, "str_val = {}", str_val.current())?;
  while str_val.simplify() {
    writeln!(output, "        = {}", str_val.current())?;
  }
  Ok(())
}
