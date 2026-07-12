//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "std")]
use std::error::Error;

use crate::std_facade::Box;
use crate::std_facade::Cow;
use crate::std_facade::String;
use crate::std_facade::fmt;

/// The reason for why something, such as a generated value, was rejected.
///
/// Currently this is merely a wrapper around a message, but more properties
/// may be added in the future.
///
/// This is constructed via `.into()` on a `String`, `&'static str`, or
/// `Box<str>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reason(Cow<'static, str>);

impl Reason {
  /// Return the message for this `Reason`.
  ///
  /// The message is intended for human consumption, and is not guaranteed to
  /// have any format in particular.
  #[must_use]
  pub fn message(&self) -> &str {
    self.0.as_ref()
  }
}

impl From<&'static str> for Reason {
  fn from(message: &'static str) -> Self {
    Self(message.into())
  }
}

impl From<String> for Reason {
  fn from(message: String) -> Self {
    Self(message.into())
  }
}

impl From<Box<str>> for Reason {
  fn from(message: Box<str>) -> Self {
    Self(String::from(message).into())
  }
}

impl fmt::Display for Reason {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(self.message(), f)
  }
}

#[cfg(feature = "std")]
impl Error for Reason {}
