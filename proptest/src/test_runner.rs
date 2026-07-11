//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! State and functions for running proptest tests.
//!
//! You do not normally need to access things in this module directly except
//! when implementing new low-level strategies.

/// Runtime configuration (`Config`) and the `PROPTEST_*` env overlay.
mod config;
#[cfg(feature = "std")]
pub(crate) mod diagnostics;
/// Per-case and whole-test outcome types (`TestCaseError` / `TestError`).
pub(crate) mod errors;
/// Pluggable storage for minimized failing seeds.
mod failure_persistence;
/// The `Reason` wrapper carried by rejects and failures.
mod reason;
/// The fork replay log shared between parent and child processes.
#[cfg(feature = "fork")]
pub(crate) mod replay;
/// Optional caching of case outcomes to skip re-running inputs.
mod result_cache;
/// The seedable, reproducible `TestRng` and its persisted seed codec.
mod rng;
/// The `TestRunner` execution and shrink loop.
mod runner;
/// Scoped panic-hook handling that silences shrink-phase panics.
#[cfg(feature = "std")]
mod scoped_panic_hook;

#[cfg(feature = "std")]
use std::fmt::Arguments;

pub use self::config::*;
pub use self::errors::{
    ProptestResultExt, TestCaseError, TestCaseResult, TestError,
};
pub use self::failure_persistence::*;
pub use self::reason::*;
pub use self::result_cache::*;
pub use self::rng::*;
pub use self::runner::*;

/// Emit the closure-style fork/timeout warning used by the public sugar macro.
#[cfg(feature = "std")]
#[doc(hidden)]
#[allow(
    clippy::single_call_fn,
    reason = "bridge closure-style sugar into the runner's typed fork/timeout diagnostic"
)]
pub(crate) fn emit_closure_fork_unsupported() {
    diagnostics::emit(&diagnostics::RunnerDiagnostic::ClosureForkUnsupported);
}

/// Emit one already-rendered diagnostic line through the runner's best-effort
/// stderr seam.
#[cfg(feature = "std")]
#[doc(hidden)]
pub fn emit_diagnostic_line(args: Arguments<'_>) {
    diagnostics::emit_line(args);
}
