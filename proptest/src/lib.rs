//-
// Copyright 2017, 2018 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! # Proptest Reference Documentation
//!
//! This is the reference documentation for the proptest API.
//!
//! For documentation on how to get started with proptest and general usage
//! advice, please refer to the [Proptest Book](https://proptest-rs.github.io/proptest/intro.html).

#![forbid(future_incompatible)]
#![deny(missing_docs, bare_trait_objects)]
#![no_std]
#![cfg_attr(
  all(feature = "unstable", not(feature = "alt-stable")),
  feature(allocator_api, coroutine_trait, never_type)
)]
#![cfg_attr(all(feature = "f16", not(feature = "alt-stable")), feature(f16))]
#![cfg_attr(
  all(feature = "std", feature = "unstable", not(feature = "alt-stable")),
  feature(ip)
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// std_facade is used in a few macros, so it needs to be public.
#[macro_use]
#[doc(hidden)]
pub mod std_facade;

#[cfg(any(feature = "std", test))]
extern crate std;

#[cfg(all(feature = "alloc", not(feature = "std")))]
extern crate alloc;

pub extern crate self as proptest;

#[macro_use]
mod product_tuple;

#[macro_use]
mod macros;

#[doc(hidden)]
#[macro_use]
pub mod sugar;

#[cfg(feature = "alt-stable")]
pub mod alt_stable;
pub mod arbitrary;
pub mod array;
pub mod bits;
pub mod bool;
pub mod char;
pub mod collection;
pub mod num;
#[cfg(feature = "std")]
pub mod range_subset;
pub mod strategy;
pub mod test_runner;
pub mod tuple;

pub mod option;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod path;
pub mod result;
pub mod sample;
#[cfg(feature = "strict-test")]
#[cfg_attr(docsrs, doc(cfg(feature = "strict-test")))]
pub mod strict;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod string;

pub mod prelude;

#[cfg(feature = "attr-macro")]
pub use proptest_macro::property_test;

#[cfg(test)]
mod tests {
  #[cfg(feature = "attr-macro")]
  #[test]
  fn compile_tests() -> Result<(), trybuild::TryBuildError> {
    let mut cases = trybuild::TestCases::new();
    cases.pass("tests/pass/*.rs");
    cases.compile_fail("tests/fail/*.rs");
    cases.run()
  }

  #[test]
  fn sugar_macro_compile_tests() -> Result<(), trybuild::TryBuildError> {
    let mut cases = trybuild::TestCases::new();
    #[cfg(feature = "strict-test")]
    cases.pass("tests/sugar/pass/*.rs");
    cases.compile_fail("tests/sugar/fail/*.rs");
    cases.run()
  }

  #[test]
  fn prelude_compile_tests() -> Result<(), trybuild::TryBuildError> {
    let mut cases = trybuild::TestCases::new();
    #[cfg(feature = "strict-test")]
    cases.pass("tests/prelude/pass/*.rs");
    cases.compile_fail("tests/prelude/fail/*.rs");
    cases.run()
  }
}
