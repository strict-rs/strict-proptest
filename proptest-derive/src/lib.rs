// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This is the API documentation for the `proptest-derive` crate. As this
//! crate does not have an API _per se_, there isn't much to see here.
//!
//! You are probably looking for the [`proptest-derive` section of the Proptest
//! Book](https://proptest-rs.github.io/proptest/proptest-derive/index.html).
//!
//! This crate is only the rustc-facing proc-macro entry point; the derive
//! pipeline itself lives in the `proptest-derive-internal` crate.

use proc_macro::TokenStream;

/// See module level documentation for more information.
#[proc_macro_derive(Arbitrary, attributes(proptest))]
#[allow(
  clippy::single_call_fn,
  reason = "proc_macro_derive shim that converts tokens and delegates to the impl pipeline"
)]
pub fn derive_proptest_arbitrary(input: TokenStream) -> TokenStream {
  // Bootstrap!
  // This function just converts tokens and delegates to the internal crate.
  proptest_derive_internal::derive_arbitrary(input.into()).into()
}
