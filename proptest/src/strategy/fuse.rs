//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::strategy::NewTree;
use crate::strategy::Strategy;
use crate::strategy::ValueTree;
#[cfg(test)]
use crate::strategy::check_strategy_sanity;
use crate::test_runner::TestRunner;

/// Adaptor for `Strategy` and `ValueTree` which guards `simplify()` and
/// `complicate()` to avoid contract violations.
///
/// This can be used as an intermediate when the caller would otherwise need
/// its own separate state tracking, or as a workaround for a broken
/// `ValueTree` implementation.
///
/// This wrapper specifically has the following effects:
///
/// - Calling `complicate()` before `simplify()` was ever called does nothing and returns `false`.
///
/// - Calling `simplify()` after it has returned `false` and no calls to `complicate()` returned
///   `true` does nothing and returns `false`.
///
/// - Calling `complicate()` after it has returned `false` and no calls to `simplify()` returned
///   `true` does nothing and returns `false`.
///
/// There is also limited functionality to alter the internal state to assist
/// in its usage as a state tracker.
///
/// Wrapping a `Strategy` in `Fuse` simply causes its `ValueTree` to also be
/// wrapped in `Fuse`.
///
/// While this is similar to `std::iter::Fuse`, it is not exposed as a method
/// on `Strategy` since the vast majority of proptest should never need this
/// functionality; it mainly concerns implementors of strategies.
#[derive(Debug, Clone, Copy)]
#[must_use = "strategies do nothing unless used"]
pub struct Fuse<T> {
  /// The wrapped strategy or value tree whose calls are being guarded.
  inner:          T,
  /// Whether a `simplify()` call may currently be productive; cleared once
  /// `simplify()` returns `false`, restored when `complicate()` succeeds.
  may_simplify:   bool,
  /// Whether a `complicate()` call may currently be productive; cleared once
  /// `complicate()` returns `false`, restored when `simplify()` succeeds.
  may_complicate: bool,
}

impl<T> Fuse<T> {
  /// Wrap the given `T` in `Fuse`.
  pub const fn new(inner: T) -> Self {
    Self {
      inner,
      may_simplify: true,
      may_complicate: false,
    }
  }
}

impl<T: Strategy> Strategy for Fuse<T> {
  type Tree = Fuse<T::Tree>;
  type Value = T::Value;

  fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
    self.inner.new_tree(runner).map(Fuse::new)
  }
}

impl<T: ValueTree> Fuse<T> {
  /// Return whether a call to `simplify()` may be productive.
  ///
  /// Formally, this is true if one of the following holds:
  ///
  /// - `simplify()` has never been called.
  /// - The most recent call to `simplify()` returned `true`.
  /// - `complicate()` has been called more recently than `simplify()` and the last call returned
  ///   `true`.
  pub const fn may_simplify(&self) -> bool {
    self.may_simplify
  }

  /// Disallow any further calls to `simplify()` until a call to
  /// `complicate()` returns `true`.
  pub const fn disallow_simplify(&mut self) {
    self.may_simplify = false;
  }

  /// Return whether a call to `complicate()` may be productive.
  ///
  /// Formally, this is true if one of the following holds:
  ///
  /// - The most recent call to `complicate()` returned `true`.
  /// - `simplify()` has been called more recently than `complicate()` and the last call returned
  ///   `true`.
  pub const fn may_complicate(&self) -> bool {
    self.may_complicate
  }

  /// Disallow any further calls to `complicate()` until a call to
  /// `simplify()` returns `true`.
  pub const fn disallow_complicate(&mut self) {
    self.may_complicate = false;
  }

  /// Prevent any further shrinking operations from occurring.
  pub const fn freeze(&mut self) {
    self.disallow_simplify();
    self.disallow_complicate();
  }
}

impl<T: ValueTree> ValueTree for Fuse<T> {
  type Value = T::Value;

  fn current(&self) -> T::Value {
    self.inner.current()
  }

  fn simplify(&mut self) -> bool {
    if self.may_simplify {
      if self.inner.simplify() {
        self.may_complicate = true;
        true
      } else {
        self.may_simplify = false;
        false
      }
    } else {
      false
    }
  }

  fn complicate(&mut self) -> bool {
    if self.may_complicate {
      if self.inner.complicate() {
        self.may_simplify = true;
        true
      } else {
        self.may_complicate = false;
        false
      }
    } else {
      false
    }
  }
}

#[cfg(test)]
mod test {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure_all;

  use super::*;
  use crate::test_runner::Reason;

  // NOTE: this fixture reports no progress when the guard contract is
  // violated. `Fuse` exists to suppress the calls that would hit those
  // paths, so the surrounding sanity check remains the detection mechanism.
  struct StrictValueTree {
    min:   u32,
    curr:  u32,
    max:   u32,
    ready: bool,
  }

  impl StrictValueTree {
    #[allow(
      clippy::single_call_fn,
      reason = "test-only ValueTree fixture constructor seeding the Fuse guard contract checks"
    )]
    fn new(start: u32) -> Self {
      Self {
        min:   0,
        curr:  start,
        max:   start,
        ready: false,
      }
    }
  }

  impl ValueTree for StrictValueTree {
    type Value = u32;

    fn current(&self) -> u32 {
      self.curr
    }

    fn simplify(&mut self) -> bool {
      if self.min > self.curr {
        return false;
      }
      if self.curr > self.min {
        self.max = self.curr;
        self.curr = self.curr.saturating_sub(1);
        self.ready = true;
        true
      } else {
        self.min = self.min.saturating_add(1);
        false
      }
    }

    fn complicate(&mut self) -> bool {
      if self.max < self.curr || !self.ready {
        return false;
      }
      if self.curr < self.max {
        self.curr = self.curr.saturating_add(1);
        true
      } else {
        self.max = self.max.saturating_sub(1);
        false
      }
    }
  }

  #[test]
  fn test_sanity() -> Result<(), Reason> {
    check_strategy_sanity(Fuse::new(0_i32..100_i32), None)
  }

  #[test]
  fn guards_bad_transitions() -> Result<(), TestFailure> {
    let mut vt = Fuse::new(StrictValueTree::new(5));
    ensure_all(&[
      (!vt.complicate(), "complicate before simplify is guarded"),
      (5 == vt.current(), "the initial value is untouched"),
      (vt.simplify(), "simplify steps down"), // 0, 4, 5
      (vt.simplify(), "simplify steps down"), // 0, 3, 4
      (vt.simplify(), "simplify steps down"), // 0, 2, 3
      (vt.simplify(), "simplify steps down"), // 0, 1, 2
      (vt.simplify(), "simplify steps down"), // 0, 0, 1
      (0 == vt.current(), "simplification reached zero"),
      (!vt.simplify(), "an exhausted tree cannot simplify"),        // 1, 0, 1
      (!vt.simplify(), "repeated simplify after false is guarded"), // 1, 0, 1
      (0 == vt.current(), "the value is unchanged after guards"),
      (vt.complicate(), "the tree complicates once"), // 1, 1, 1
      (1 == vt.current(), "complicate stepped back up"),
      (!vt.complicate(), "an exhausted tree cannot complicate"),        // 1, 1, 0
      (!vt.complicate(), "repeated complicate after false is guarded"), // 1, 1, 0
      (1 == vt.current(), "the value is unchanged after guards"),
    ])
  }
}
