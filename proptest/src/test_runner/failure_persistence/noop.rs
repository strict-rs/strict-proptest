//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{Box, Vec, fmt};
use core::any::Any;

use crate::test_runner::failure_persistence::{
    FailurePersistence, PersistedSeed,
};

/// Failure persistence option that loads and saves nothing at all.
#[derive(Debug, Default, PartialEq)]
#[allow(dead_code)]
struct NoopFailurePersistence;

impl FailurePersistence for NoopFailurePersistence {
    fn load_persisted_failures2(
        &self,
        _source_file: Option<&'static str>,
    ) -> Vec<PersistedSeed> {
        Vec::new()
    }

    fn save_persisted_failure2(
        &mut self,
        _source_file: Option<&'static str>,
        _seed: PersistedSeed,
        _shrunken_value: &dyn fmt::Debug,
    ) {
    }

    fn box_clone(&self) -> Box<dyn FailurePersistence> {
        Box::new(NoopFailurePersistence)
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
    use strict_test_support::{TestFailure, ensure, ensure_all};

    #[test]
    fn default_load_is_empty() -> Result<(), TestFailure> {
        ensure(
            NoopFailurePersistence
                .load_persisted_failures2(None)
                .is_empty(),
            "the noop backend loads nothing without a source",
        )?;
        ensure(
            NoopFailurePersistence
                .load_persisted_failures2(HI_PATH)
                .is_empty(),
            "the noop backend loads nothing for a source",
        )
    }

    #[test]
    fn seeds_not_recoverable() -> Result<(), TestFailure> {
        let mut p = NoopFailurePersistence;
        p.save_persisted_failure2(HI_PATH, INC_SEED, &"");
        ensure_all(&[
            (
                p.load_persisted_failures2(HI_PATH).is_empty(),
                "a saved seed is not recoverable for its source",
            ),
            (
                p.load_persisted_failures2(None).is_empty(),
                "nothing is recoverable without a source",
            ),
            (
                p.load_persisted_failures2(UNREL_PATH).is_empty(),
                "nothing is recoverable for an unrelated source",
            ),
        ])
    }
}
