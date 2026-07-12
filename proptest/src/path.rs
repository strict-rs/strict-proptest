//! Strategies for generating [`PathBuf`](std::path::PathBuf) and related
//! path types.
//!
//! [`PathParams`] in this module is used as the argument to the
//! [`Arbitrary`](crate::arbitrary::Arbitrary) implementation for
//! [`PathBuf`](std::path::PathBuf).

use crate::collection::SizeRange;
use crate::string::StringParam;

/// Parameters for the [`Arbitrary`](crate::arbitrary::Arbitrary)
/// implementation for [`PathBuf`](std::path::PathBuf).
///
/// By default, this generates paths with 0 to 8 components uniformly at random, each of which is a
/// default [`StringParam`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathParams {
  /// The number of components in the path.
  components:      SizeRange,
  /// The regular expression to generate individual components.
  component_regex: StringParam,
}

impl PathParams {
  /// Gets the number of components in the path.
  #[must_use]
  pub fn components(&self) -> SizeRange {
    self.components.clone()
  }

  /// Sets the number of components in the path.
  #[must_use]
  pub fn with_components(mut self, components: impl Into<SizeRange>) -> Self {
    self.components = components.into();
    self
  }

  /// Gets the regular expression to generate individual components.
  #[must_use]
  pub const fn component_regex(&self) -> StringParam {
    self.component_regex
  }

  /// Sets the regular expression to generate individual components.
  #[must_use]
  pub fn with_component_regex(mut self, component_regex: impl Into<StringParam>) -> Self {
    self.component_regex = component_regex.into();
    self
  }
}

impl Default for PathParams {
  fn default() -> Self {
    Self {
      components:      (0..8).into(),
      // This is the default regex for `any::<String>()`.
      component_regex: StringParam::default(),
    }
  }
}
