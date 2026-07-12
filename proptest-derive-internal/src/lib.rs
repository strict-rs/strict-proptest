// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Implementation detail of `proptest-derive`.
//!
//! This crate holds the parsing, attribute interpretation, bound inference,
//! and code generation behind `#[derive(Arbitrary)]`. It is intentionally
//! separate from the `proptest-derive` proc-macro entry point so the pipeline
//! can be exercised as ordinary library code. Nothing here is a stable API;
//! depend on `proptest-derive` instead.

// # Known issues
//
// ## Fields with `[T; N]` where `N > 32`
//
// We can't derive for fields having arrays with sizes over 32.
// While proptest only supports in UniformArrayStrategy arrays of sizes up to
// 32, we can overcome that restriction by generating custom types on the
// fly here. What we can't overcome is that `T: Arbititrary |- T: Debug` due
// to the requirement by proptest. Since `T: Debug` must hold, we must also
// ensure that arrays with sizes over 33 are also Debug. We can't do this.
// Doing so would create orphan instances, which Rust does not allow to preserve
// coherence. Therefore, until const generics lands in stable or when
// we can remove the `T: Debug` bound on Arbitrary, we can not support arrays
// sized over 32.
//
// # Recursive types
//
// We can't handle self-recursive or mutually recursive types at all right now.

pub mod ast;
pub mod attr;
pub mod derive;
pub mod error;
/// A tiny compile-time interpreter (`eval_expr`) over constant integer
/// expressions, used to evaluate `weight` attributes and array lengths `N`.
pub mod interp;
pub mod use_tracking;
pub mod util;
pub mod void;

/// Expand `#[derive(Arbitrary)]` for the annotated item.
///
/// Parses `input` into a `syn::DeriveInput` and runs it through the derive
/// pipeline, returning either the generated `impl` or a `compile_error!`
/// invocation describing what went wrong. A malformed `input` — which rustc
/// never hands a real `#[proc_macro_derive]` — is itself surfaced as a
/// `compile_error!`, so there is no panic path.
#[must_use]
#[allow(
  clippy::single_call_fn,
  reason = "library entry point invoked once by the proptest-derive proc-macro shim"
)]
pub fn derive_arbitrary(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
  match syn::parse2(input) {
    Ok(ast) => derive::impl_proptest_arbitrary(ast),
    Err(error) => error.to_compile_error(),
  }
}

#[cfg(test)]
mod tests;
