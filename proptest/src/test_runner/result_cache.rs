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

/// Identity of an evaluation owned by the current runner execution.
///
/// A cache stores this identity, never the property's success or failure payload.
/// Identities are valid only within the execution that inserted them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationId(
  /// Position in the execution's ordered evaluation records.
  pub usize,
);

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
  /// Save the identity of the evaluation associated with this input.
  fn put(&mut self, key: u64, evaluation: EvaluationId);
  /// If `put()` has been called with a semantically equivalent `key`, return
  /// the saved result. Otherwise, return `None`.
  fn get(&self, key: u64) -> Option<EvaluationId>;
}

/// The `basic_result_cache` backend: a `HashMap` keyed by input hash.
#[cfg(feature = "std")]
#[derive(Debug, Default, Clone)]
struct BasicResultCache {
  /// Outcomes keyed by the hash of the input's `Debug` string.
  entries: HashMap<u64, EvaluationId>,
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

  fn put(&mut self, key: u64, evaluation: EvaluationId) {
    let _previous = self.entries.insert(key, evaluation);
  }

  fn get(&self, key: u64) -> Option<EvaluationId> {
    self.entries.get(&key).copied()
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
  fn put(&mut self, _: u64, _: EvaluationId) {}
  fn get(&self, _: u64) -> Option<EvaluationId> {
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
  use strict_test_support::ComparisonFailure;
  use strict_test_support::ensure_eq;

  use super::*;

  /// Absent, initial, and replaced evaluation identities.
  type Observations = [Option<EvaluationId>; 3];

  #[test]
  fn basic_result_cache_replaces_existing_key() -> Result<(), ComparisonFailure<Observations, Observations>> {
    let key = 42;
    let mut cache = BasicResultCache::default();

    let absent = cache.get(key);
    cache.put(key, EvaluationId(2));
    let original = cache.get(key);
    cache.put(key, EvaluationId(5));
    ensure_eq(
      [absent, original, cache.get(key)],
      [None, Some(EvaluationId(2)), Some(EvaluationId(5))],
      "cache entries reference evaluations and replace an existing identity",
    )
    .map(drop)
  }
}
