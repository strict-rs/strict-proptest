//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Prints selected fields from the runner's default `Config`.
//!
//! Under the default `std` feature that default is environment-resolved, so
//! `PROPTEST_*` overrides show through — for example `PROPTEST_CASES=42`
//! makes the printed `cases` field read `42`. A quick way to inspect the
//! runner's effective configuration.

use proptest::test_runner::Config;
use std::io::{self, Write as _};

fn main() -> io::Result<()> {
    let config = Config::default();
    writeln!(
        io::stdout().lock(),
        "Default config: cases={}, max_shrink_iters={}, fork={}, timeout={}",
        config.cases,
        config.max_shrink_iters(),
        config.fork(),
        config.timeout()
    )
}
