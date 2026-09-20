//! Consumer-owned repository extension composition.
//!
//! Standard repository workflows execute through the installed `template`
//! binary. This crate compiles only the guarded local `x` registry.

use std::process::ExitCode;

pub mod extensions;

/// Run the guarded repository-specific extension surface.
#[must_use]
pub fn run() -> ExitCode {
  template_stask::run_with_extensions(extensions::commands())
}

#[cfg(test)]
mod tests {
  use std::ops::ControlFlow;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;
  use template_core::cli::command::CommandSet;
  use template_core::cli::command::CommandSurface;
  use template_core::cli::parse::ParseReport;

  use super::extensions;

  /// Complete result of constructing the local extension registry.
  type Registry = template_stask::Result<CommandSet<ControlFlow<ParseReport>>>;

  #[test]
  fn extension_registry_exposes_only_the_local_x_router() -> Result<(), Box<PredicateFailure<Registry>>> {
    ensure_that(
      extensions::commands(),
      "the consumer runner must expose only the local x extension surface",
      |registry| {
        registry.as_ref().is_ok_and(|command_set| {
          let descriptors = command_set.descriptors();
          descriptors.len() == 1
            && descriptors
              .first()
              .is_some_and(|descriptor| descriptor.name() == "x" && descriptor.surface() == CommandSurface::StaskExtension)
        })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }
}
