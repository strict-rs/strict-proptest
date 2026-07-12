//-
// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "std")]
use std::collections::HashMap;

use crate::std_facade::Box;
use crate::std_facade::fmt;
use crate::test_runner::errors::TestCaseResult;

/// A key used for the result cache.
///
/// The capabilities of this structure are currently quite limited; all one can
/// do with safe code is get the `&dyn Debug` of the test input value. This may
/// improve in the future, particularly at such a time that specialisation
/// becomes stable.
#[derive(Debug)]
pub struct ResultCacheKey<'a> {
  /// The test input, exposed only as `&dyn Debug` (see `value_debug`).
  value: &'a dyn fmt::Debug,
}

impl<'a> ResultCacheKey<'a> {
  /// Wrap a test input value as a cache key.
  #[allow(
    clippy::single_call_fn,
    reason = "an opaque result-cache key holding one test input value"
  )]
  pub(crate) fn new(case_value: &'a dyn fmt::Debug) -> Self {
    Self {
      value: case_value
    }
  }

  /// Return the test input value as an `&dyn Debug`.
  #[must_use]
  pub fn value_debug(&self) -> &dyn fmt::Debug {
    self.value
  }
}

/// Display adapter for hashing a cache key by the wrapped value's `Debug`
/// representation.
#[cfg(feature = "std")]
struct DebugDisplay<'a>(&'a dyn fmt::Debug);

#[cfg(feature = "std")]
impl fmt::Display for DebugDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(self.0, f)
  }
}

/// An object which can cache the outcomes of tests.
pub trait ResultCache {
  /// Convert the given cache key into a `u64` representing that value. The
  /// u64 is used as the key below.
  ///
  /// This is a separate step so that ownership of the key value can be
  /// handed off to user code without needing to be able to clone it.
  fn key(&self, key: &ResultCacheKey<'_>) -> u64;
  /// Save `result` as the outcome associated with the test input in `key`.
  ///
  /// `result` is passed as a reference so that the decision to clone depends
  /// on whether the cache actually plans on storing it.
  fn put(&mut self, key: u64, result: &TestCaseResult);
  /// If `put()` has been called with a semantically equivalent `key`, return
  /// the saved result. Otherwise, return `None`.
  fn get(&self, key: u64) -> Option<&TestCaseResult>;
}

/// The `basic_result_cache` backend: a `HashMap` keyed by input hash.
#[cfg(feature = "std")]
#[derive(Debug, Default, Clone)]
struct BasicResultCache {
  /// Outcomes keyed by the hash of the input's `Debug` string.
  entries: HashMap<u64, TestCaseResult>,
}

#[cfg(feature = "std")]
impl ResultCache for BasicResultCache {
  fn key(&self, cache_key: &ResultCacheKey<'_>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher as _;

    use crate::std_facade::fmt::Write as _;

    struct HashWriter(DefaultHasher);
    impl fmt::Write for HashWriter {
      fn write_str(&mut self, fragment: &str) -> fmt::Result {
        self.0.write(fragment.as_bytes());
        Ok(())
      }
    }

    let mut hash = HashWriter(DefaultHasher::default());
    if write!(hash, "{}", DebugDisplay(cache_key.value_debug())).is_err() {
      return 0;
    }
    hash.0.finish()
  }

  fn put(&mut self, key: u64, result: &TestCaseResult) {
    let _previous = self.entries.insert(key, result.clone());
  }

  fn get(&self, key: u64) -> Option<&TestCaseResult> {
    self.entries.get(&key)
  }
}

/// A basic result cache.
///
/// Values are identified by their `Debug` string representation.
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[must_use]
#[allow(
  clippy::single_call_fn,
  reason = "the HashMap-backed ResultCache that Config::result_cache installs"
)]
pub fn basic_result_cache() -> Box<dyn ResultCache> {
  Box::new(BasicResultCache::default())
}

/// The `noop_result_cache` backend: caches nothing.
struct NoOpResultCache;
impl ResultCache for NoOpResultCache {
  fn key(&self, _: &ResultCacheKey<'_>) -> u64 {
    0
  }
  fn put(&mut self, _: u64, _: &TestCaseResult) {}
  fn get(&self, _: u64) -> Option<&TestCaseResult> {
    None
  }
}

/// A result cache that does nothing.
///
/// This is the default value of `ProptestConfig.result_cache`.
#[must_use]
#[allow(
  clippy::single_call_fn,
  reason = "the do-nothing ResultCache used as Config's out-of-the-box default"
)]
pub fn noop_result_cache() -> Box<dyn ResultCache> {
  Box::new(NoOpResultCache)
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_some;

  use super::*;
  use crate::test_runner::TestCaseError;

  #[test]
  fn basic_result_cache_replaces_existing_key() -> Result<(), TestFailure> {
    let key = 42;
    let mut cache = BasicResultCache::default();

    cache.put(key, &Ok(()));
    ensure(
      ensure_some(cache.get(key), "the first cache result is stored")?.is_ok(),
      "the first cached result is successful",
    )?;

    cache.put(key, &Err(TestCaseError::fail("replacement")));
    ensure(
      matches!(
        ensure_some(cache.get(key), "the replacement cache result is stored")?,
        Err(TestCaseError::Fail(_))
      ),
      "the replacement result overwrites the original entry",
    )
  }
}
