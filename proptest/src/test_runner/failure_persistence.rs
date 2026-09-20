//-
// Copyright 2017, 2018, 2019 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::any::Any;
use core::fmt::Display;
use core::result::Result;
use core::str::FromStr;

use crate::std_facade::Box;
use crate::std_facade::Vec;
use crate::std_facade::fmt;

/// Share trait-object plumbing while preserving each backend's copy or clone operation.
macro_rules! persistence_object {
  ($receiver:ident => $cloned:expr) => {
    fn box_clone(&$receiver) -> $crate::std_facade::Box<dyn $crate::test_runner::FailurePersistence> {
      $crate::std_facade::Box::new($cloned)
    }

    fn eq(&self, other: &dyn $crate::test_runner::FailurePersistence) -> bool {
      other.as_any().downcast_ref::<Self>().is_some_and(|backend| backend == self)
    }

    fn as_any(&self) -> &dyn ::core::any::Any {
      self
    }
  };
}

/// The `std`-only file-backed backend (`FileFailurePersistence`).
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
mod file;
/// The in-memory `BTreeMap`-backed backend (`MapFailurePersistence`).
mod map;

#[cfg(feature = "std")]
pub use self::file::*;
pub use self::map::*;
use crate::test_runner::Seed;

/// Opaque struct representing a seed which can be persisted.
///
/// The `Display` and `FromStr` implementations go to and from the format
/// Proptest uses for its persistence file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PersistedSeed(pub(crate) Seed);

impl Display for PersistedSeed {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.0.to_persistence())
  }
}

impl FromStr for PersistedSeed {
  type Err = ();

  fn from_str(s: &str) -> Result<Self, ()> {
    Seed::from_persistence(s).map(PersistedSeed).ok_or(())
  }
}

/// Provides external persistence for historical test failures by storing
/// current-format persisted seeds.
pub trait FailurePersistence: Send + Sync + fmt::Debug {
  /// Supply seeds associated with the given `source_file` that may be used
  /// by a `TestRunner`'s random number generator in order to consistently
  /// recreate a previously-failing `Strategy`-provided value.
  fn load_persisted_failures2(&self, source_file: Option<&'static str>) -> Vec<PersistedSeed>;

  /// Store a new failure-generating seed associated with the given `source_file`.
  fn save_persisted_failure2(&mut self, source_file: Option<&'static str>, seed: PersistedSeed, shrunken_value: &dyn fmt::Debug);

  /// Delegate method for producing a trait object usable with `Clone`
  fn box_clone(&self) -> Box<dyn FailurePersistence>;

  /// Equality testing delegate required due to constraints of trait objects.
  fn eq(&self, other: &dyn FailurePersistence) -> bool;

  /// Assistant method for trait object comparison.
  fn as_any(&self) -> &dyn Any;
}

impl<'b> PartialEq<dyn FailurePersistence + 'b> for dyn FailurePersistence + '_ {
  fn eq(&self, other: &(dyn FailurePersistence + 'b)) -> bool {
    FailurePersistence::eq(self, other)
  }
}

impl Clone for Box<dyn FailurePersistence> {
  fn clone(&self) -> Self {
    self.box_clone()
  }
}

#[cfg(test)]
mod tests {
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::FailurePersistence;
  #[cfg(feature = "std")]
  use super::FileFailurePersistence;
  use super::MapFailurePersistence;
  use super::PersistedSeed;
  use crate::std_facade::Box;
  use crate::test_runner::rng::Seed;

  pub(super) const INC_SEED: PersistedSeed = PersistedSeed(Seed::XorShift([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]));

  pub(super) const HI_PATH: Option<&str> = Some("hi");
  pub(super) const UNREL_PATH: Option<&str> = Some("unrelated");

  /// Original backend, its unchanged clone, and a separately mutated clone.
  type MapClones = (MapFailurePersistence, Box<dyn FailurePersistence>, Box<dyn FailurePersistence>);

  #[test]
  fn cloned_map_preserves_seeds_and_changes_independently() -> Result<(), Box<PredicateFailure<MapClones>>> {
    let mut original = MapFailurePersistence::default();
    original.save_persisted_failure2(HI_PATH, INC_SEED, &"first");
    let unchanged = original.box_clone();
    let mut changed = unchanged.clone();
    changed.save_persisted_failure2(UNREL_PATH, INC_SEED, &"second");
    ensure_that(
      (original, unchanged, changed),
      "boxed clones retain map identity and independent contents",
      |observed| {
        observed.1.as_any().downcast_ref::<MapFailurePersistence>() == Some(&observed.0)
          && FailurePersistence::eq(&observed.0, observed.1.as_ref())
          && observed.1.as_ref() != observed.2.as_ref()
          && observed.0.load_persisted_failures2(UNREL_PATH).is_empty()
          && observed.2.load_persisted_failures2(UNREL_PATH) == [INC_SEED]
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  /// File configuration, its boxed clone, and a distinct backend implementation.
  #[cfg(feature = "std")]
  type FileClone = (FileFailurePersistence, Box<dyn FailurePersistence>, MapFailurePersistence);

  #[test]
  #[cfg(feature = "std")]
  fn cloned_file_preserves_configuration_and_rejects_other_backends() -> Result<(), Box<PredicateFailure<FileClone>>> {
    let original = FileFailurePersistence::Direct("regressions.txt");
    let cloned = original.box_clone();
    ensure_that(
      (original, cloned, MapFailurePersistence::default()),
      "file clones compare by concrete backend and configuration",
      |observed| {
        observed.1.as_any().downcast_ref::<FileFailurePersistence>() == Some(&observed.0)
          && FailurePersistence::eq(&observed.0, observed.1.as_ref())
          && !FailurePersistence::eq(observed.1.as_ref(), &FileFailurePersistence::Off)
          && !FailurePersistence::eq(observed.1.as_ref(), &FileFailurePersistence::Direct("other.txt"))
          && !FailurePersistence::eq(observed.1.as_ref(), &observed.2)
          && !FailurePersistence::eq(&observed.2, observed.1.as_ref())
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}
