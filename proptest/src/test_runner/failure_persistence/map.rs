//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{BTreeMap, BTreeSet, Box, Vec, fmt};
use core::any::Any;

use crate::test_runner::failure_persistence::FailurePersistence;
use crate::test_runner::failure_persistence::PersistedSeed;

/// Failure persistence option that loads and saves seeds in memory
/// on the heap. This may be useful when accumulating test failures
/// across multiple `TestRunner` instances for external reporting
/// or batched persistence.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapFailurePersistence {
    /// Backing map, keyed by source_file.
    pub map: BTreeMap<&'static str, BTreeSet<PersistedSeed>>,
}

impl FailurePersistence for MapFailurePersistence {
    fn load_persisted_failures2(
        &self,
        source_file: Option<&'static str>,
    ) -> Vec<PersistedSeed> {
        source_file
            .and_then(|source| self.map.get(source))
            .map(|seeds| seeds.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    }

    fn save_persisted_failure2(
        &mut self,
        source_file: Option<&'static str>,
        seed: PersistedSeed,
        _shrunken_value: &dyn fmt::Debug,
    ) {
        let s = match source_file {
            Some(sf) => sf,
            None => return,
        };
        let set = self.map.entry(s).or_default();
        set.insert(seed);
    }

    fn box_clone(&self) -> Box<dyn FailurePersistence> {
        Box::new(self.clone())
    }

    fn eq(&self, other: &dyn FailurePersistence) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|x| x == self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_runner::failure_persistence::tests::*;
    use strict_test_support::{TestFailure, ensure, ensure_eq, ensure_some};

    #[test]
    fn initial_map_is_empty() -> Result<(), TestFailure> {
        ensure(
            MapFailurePersistence::default()
                .load_persisted_failures2(HI_PATH)
                .is_empty(),
            "a fresh map has no persisted failures",
        )
    }

    #[test]
    fn seeds_recoverable() -> Result<(), TestFailure> {
        let mut p = MapFailurePersistence::default();
        p.save_persisted_failure2(HI_PATH, INC_SEED, &"");
        let restored = p.load_persisted_failures2(HI_PATH);
        ensure_eq(&1, &restored.len(), "one saved seed is restored")?;
        let first =
            ensure_some(restored.first(), "the restored list has a head")?;
        ensure(INC_SEED == *first, "the restored seed equals the saved one")?;

        ensure(
            p.load_persisted_failures2(None).is_empty(),
            "a missing source restores nothing",
        )?;
        ensure(
            p.load_persisted_failures2(UNREL_PATH).is_empty(),
            "an unrelated source restores nothing",
        )
    }

    #[test]
    fn seeds_deduplicated() -> Result<(), TestFailure> {
        let mut p = MapFailurePersistence::default();
        p.save_persisted_failure2(HI_PATH, INC_SEED, &"");
        p.save_persisted_failure2(HI_PATH, INC_SEED, &"");
        let restored = p.load_persisted_failures2(HI_PATH);
        ensure_eq(&1, &restored.len(), "identical seeds are deduplicated")
    }
}
