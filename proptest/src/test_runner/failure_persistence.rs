//-
// Copyright 2017, 2018, 2019 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{Box, Vec, fmt};
use core::any::Any;
use core::fmt::Display;
use core::result::Result;
use core::str::FromStr;

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
    fn load_persisted_failures2(
        &self,
        source_file: Option<&'static str>,
    ) -> Vec<PersistedSeed>;

    /// Store a new failure-generating seed associated with the given `source_file`.
    fn save_persisted_failure2(
        &mut self,
        source_file: Option<&'static str>,
        seed: PersistedSeed,
        shrunken_value: &dyn fmt::Debug,
    );

    /// Delegate method for producing a trait object usable with `Clone`
    fn box_clone(&self) -> Box<dyn FailurePersistence>;

    /// Equality testing delegate required due to constraints of trait objects.
    fn eq(&self, other: &dyn FailurePersistence) -> bool;

    /// Assistant method for trait object comparison.
    fn as_any(&self) -> &dyn Any;
}

impl<'b> PartialEq<dyn FailurePersistence + 'b>
    for dyn FailurePersistence + '_
{
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
    use super::PersistedSeed;
    use crate::test_runner::rng::Seed;

    pub(super) const INC_SEED: PersistedSeed = PersistedSeed(Seed::XorShift([
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ]));

    pub(super) const HI_PATH: Option<&str> = Some("hi");
    pub(super) const UNREL_PATH: Option<&str> = Some("unrelated");
}
