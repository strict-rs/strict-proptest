//! Consumer-owned extension registry for `just x <name>` commands.

use std::ops::ControlFlow;

use template_core::cli::command::CommandSet;
use template_core::cli::parse::ParseReport;

/// Build this repository's intentionally empty extension registry.
///
/// # Errors
///
/// Returns a typed registration error if the controlled `x` router metadata
/// is invalid.
#[allow(
  clippy::single_call_fn,
  reason = "Keep repository-owned command registration separate from guarded execution."
)]
pub fn commands() -> template_stask::Result<CommandSet<ControlFlow<ParseReport>>> {
  template_stask::empty_registry("strict-proptest extensions")
}
