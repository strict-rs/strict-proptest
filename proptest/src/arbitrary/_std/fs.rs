//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::fs`.

use std::fs::DirBuilder;

use crate::arbitrary::{SMapped, any};
use crate::strategy::statics::static_map;

// TODO: other parts (figure out workable semantics).

fn configured_dir_builder(recursive: bool) -> DirBuilder {
    let mut db = DirBuilder::new();
    let _builder = db.recursive(recursive);
    db
}

arbitrary!(DirBuilder, SMapped<bool, Self>; {
    static_map(any::<bool>(), configured_dir_builder)
});

#[cfg(test)]
mod test {
    use strict_test_support::{TempDir, TestFailure, ensure, ensure_ok};

    use super::*;

    no_panic_test!(dir_builder => DirBuilder);

    #[test]
    fn recursive_dir_builder_creates_missing_parents() -> Result<(), TestFailure>
    {
        let dir = TempDir::new("dir-builder-recursive")?;
        let nested = dir.child("parent").join("child");

        ensure_ok(
            configured_dir_builder(true).create(&nested),
            "recursive DirBuilder creates missing parents",
        )?;
        ensure(nested.is_dir(), "the nested directory exists")
    }

    #[test]
    fn non_recursive_dir_builder_rejects_missing_parents()
    -> Result<(), TestFailure> {
        let dir = TempDir::new("dir-builder-non-recursive")?;
        let nested = dir.child("other").join("child");

        ensure(
            configured_dir_builder(false).create(&nested).is_err(),
            "non-recursive DirBuilder rejects a missing parent",
        )
    }
}
