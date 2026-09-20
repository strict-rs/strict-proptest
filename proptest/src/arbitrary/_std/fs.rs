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

use crate::arbitrary::SMapped;
use crate::arbitrary::any;
use crate::strategy::statics::static_map;

// TODO: other parts (figure out workable semantics).

/// Construct a `DirBuilder` configured with the requested recursive flag.
#[allow(
  clippy::single_call_fn,
  reason = "construct a DirBuilder from the generated recursive flag and share that runtime contract with tests"
)]
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
  use std::fs::Metadata;
  use std::fs::metadata;
  use std::io;
  use std::path::PathBuf;

  use strict_test_support::PredicateFailure;
  use strict_test_support::TempDir;
  use strict_test_support::TestFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Box;

  no_panic_test!(dir_builder => DirBuilder);

  /// Retain the temporary owner, requested path, builder, creation, and metadata.
  type DirectoryCreation = Result<(TempDir, PathBuf, DirBuilder, io::Result<()>, io::Result<Metadata>), TestFailure>;

  #[test]
  fn recursive_dir_builder_creates_missing_parents() -> Result<(), Box<PredicateFailure<DirectoryCreation>>> {
    let observed = TempDir::new("dir-builder-recursive").map(|dir| {
      let nested = dir.child("parent").join("child");
      let builder = configured_dir_builder(true);
      let created = builder.create(&nested);
      let inspected = metadata(&nested);
      (dir, nested, builder, created, inspected)
    });
    ensure_that(
      observed,
      "recursive DirBuilder creates missing parents and the requested directory",
      |subject| {
        subject
          .as_ref()
          .is_ok_and(|reached| reached.3.is_ok() && reached.4.as_ref().is_ok_and(Metadata::is_dir))
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn non_recursive_dir_builder_rejects_missing_parents() -> Result<(), Box<PredicateFailure<DirectoryCreation>>> {
    let observed = TempDir::new("dir-builder-non-recursive").map(|dir| {
      let nested = dir.child("other").join("child");
      let builder = configured_dir_builder(false);
      let created = builder.create(&nested);
      let inspected = metadata(&nested);
      (dir, nested, builder, created, inspected)
    });
    ensure_that(
      observed,
      "non-recursive DirBuilder rejects missing parents without creating the child",
      |subject| {
        let Ok(reached) = subject.as_ref() else {
          return false;
        };
        reached.3.as_ref().is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
          && reached.4.as_ref().is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}
