//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::BTreeMap;
use crate::std_facade::BTreeSet;
use crate::std_facade::Vec;
use crate::std_facade::fmt;
use crate::test_runner::failure_persistence::FailurePersistence;
use crate::test_runner::failure_persistence::PersistedSeed;

/// In-memory failure persistence backed by a heap map.
///
/// Loads and saves seeds in memory rather than on disk. This may be
/// useful when accumulating test failures across multiple `TestRunner`
/// instances for external reporting or batched persistence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapFailurePersistence {
  /// Backing map, keyed by `source_file`.
  pub map: BTreeMap<&'static str, BTreeSet<PersistedSeed>>,
}

impl FailurePersistence for MapFailurePersistence {
  fn load_persisted_failures2(&self, source_file: Option<&'static str>) -> Vec<PersistedSeed> {
    source_file
      .and_then(|source| self.map.get(source))
      .map(|seeds| seeds.iter().cloned().collect::<Vec<_>>())
      .unwrap_or_default()
  }

  fn save_persisted_failure2(&mut self, source_file: Option<&'static str>, seed: PersistedSeed, _shrunken_value: &dyn fmt::Debug) {
    let Some(source) = source_file else {
      return;
    };
    let set = self.map.entry(source).or_default();
    let _inserted = set.insert(seed);
  }

  persistence_object!(self => self.clone());
}

#[cfg(test)]
mod tests {
  use strict_test_support::ComparisonFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::test_runner::failure_persistence::tests::*;

  /// Native seeds returned by the persistence backend.
  type Seeds = Vec<PersistedSeed>;
  /// Backend state and observations for the saved, missing, and unrelated sources.
  type Recovery = (MapFailurePersistence, [Seeds; 3]);

  #[test]
  fn initial_map_is_empty() -> Result<(), ComparisonFailure<Seeds, Seeds>> {
    ensure_eq(
      MapFailurePersistence::default().load_persisted_failures2(HI_PATH),
      Vec::new(),
      "a fresh map has no persisted failures",
    )
    .map(drop)
  }

  #[test]
  fn seeds_recoverable() -> Result<(), PredicateFailure<Recovery>> {
    let mut persistence = MapFailurePersistence::default();
    persistence.save_persisted_failure2(HI_PATH, INC_SEED, &"");
    let restored = [
      persistence.load_persisted_failures2(HI_PATH),
      persistence.load_persisted_failures2(None),
      persistence.load_persisted_failures2(UNREL_PATH),
    ];
    ensure_that(
      (persistence, restored),
      "only the source used to save a seed restores it",
      |observed| {
        let [ref saved, ref missing, ref unrelated] = observed.1;
        saved == &[INC_SEED] && missing.is_empty() && unrelated.is_empty()
      },
    )
    .map(drop)
  }

  #[test]
  fn seeds_deduplicated() -> Result<(), ComparisonFailure<Seeds, [PersistedSeed; 1]>> {
    let mut persistence = MapFailurePersistence::default();
    persistence.save_persisted_failure2(HI_PATH, INC_SEED, &"");
    persistence.save_persisted_failure2(HI_PATH, INC_SEED, &"");
    ensure_eq(
      persistence.load_persisted_failures2(HI_PATH),
      [INC_SEED],
      "identical seeds are deduplicated",
    )
    .map(drop)
  }
}
