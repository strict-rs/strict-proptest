use std::mem::take;

use proc_macro2::TokenStream;
use quote::ToTokens as _;
use quote::quote_spanned;
use syn::FnArg;
use syn::ItemFn;
use syn::Meta;
use syn::PatType;
use syn::ReturnType;
use syn::Type;
use syn::spanned::Spanned;

use super::utils::is_strategy;

/// Validate an `ItemFn` for some basic sanity checks
///
/// Many checks are deferred to rustc (e.g. rustc already errors if you make a test function
/// unsafe, so we just transparently pass unsafe through to the generated function and let rustc
/// emit the error)
#[allow(
  clippy::single_call_fn,
  reason = "run the self, attribute, and return-type checks on one annotated fn"
)]
pub(super) fn validate(f: &mut ItemFn) -> Result<(), TokenStream> {
  all_args_non_self(f)?;
  validate_parameter_attrs(f)?;
  returns_strict_result(f)?;

  Ok(())
}

/// The error emitted for property tests whose signature declares no return
/// type or the literal `-> ()`.
const UNIT_RETURN_ERROR: &str =
  "strict property tests must return `Result<A, E>`, not `()`; declare a concrete assertion result or result alias";

/// Reject a property test whose signature returns `()`.
///
/// The generated wrapper drives the body through the strict runner as
/// `Result<A, E>`, so a unit body
/// would otherwise only surface as a cryptic type-inference error inside the
/// generated `ensure_property` call; rejecting it here gives a spanned,
/// actionable diagnostic instead. Like the rest of this module the check is
/// purely syntactic: a type alias that resolves to `()` is not caught here
/// and is left to rustc's type error.
#[allow(
  clippy::single_call_fn,
  reason = "reject a property test signature that returns unit instead of a typed result"
)]
fn returns_strict_result(f: &ItemFn) -> Result<(), TokenStream> {
  match f.sig.output {
    ReturnType::Default => err(&f.sig.ident, UNIT_RETURN_ERROR),
    ReturnType::Type(_, ref ty) => match **ty {
      Type::Tuple(ref tuple) if tuple.elems.is_empty() => err(ty, UNIT_RETURN_ERROR),
      Type::Array(_)
      | Type::FnPtr(_)
      | Type::Group(_)
      | Type::ImplTrait(_)
      | Type::Infer(_)
      | Type::Macro(_)
      | Type::Never(_)
      | Type::Paren(_)
      | Type::Path(_)
      | Type::Ptr(_)
      | Type::Reference(_)
      | Type::Slice(_)
      | Type::TraitObject(_)
      | Type::Tuple(_)
      | Type::Verbatim(_)
      | _ => Ok(()),
    },
  }
}

/// Reject any `self` receiver on the annotated fn.
///
/// Property-test functions are free functions, so a receiver (`self`,
/// `&self`, `self: T`, …) is always an error; this short-circuits on the
/// first one found.
#[allow(
  clippy::single_call_fn,
  reason = "reject any self receiver on the annotated property test function"
)]
fn all_args_non_self(f: &ItemFn) -> Result<(), TokenStream> {
  let first_self_arg = f.sig.inputs.iter().find(|arg| matches!(arg, FnArg::Receiver(_)));

  first_self_arg.map_or_else(|| Ok(()), |arg| err(arg, "`self` parameters are forbidden"))
}

/// Make sure we only have `#[strategy = <expr>]` attributes on function parameters
#[allow(
  clippy::single_call_fn,
  reason = "reject parameter attributes other than a single well-formed strategy override"
)]
fn validate_parameter_attrs(f: &mut ItemFn) -> Result<(), TokenStream> {
  let mut error = quote::quote! {};

  for param in &mut f.sig.inputs {
    let &mut FnArg::Typed(ref mut pat_ty) = param else {
      return err(param, "`self` parameters are forbidden");
    };

    // add error for any non-`strategy` error or inner attributes (i.e. `#![...]` )
    for attr in pat_ty.attrs.iter().filter(|attr| !is_strategy(attr)) {
      error.extend(quote_spanned! {
          attr.span() => compile_error!("only `#[strategy = <expr>]` attributes are allowed here");
      });
    }

    retain_single_strategy_attr(pat_ty, &mut error);
  }

  if error.is_empty() { Ok(()) } else { Err(error) }
}

/// Keep the first well-formed `#[strategy = <expr>]` attribute on a
/// parameter — a parameter has exactly one generation strategy — diagnosing
/// duplicates and malformed shapes. Malformed attributes are retained so
/// later stages still see them.
#[allow(
  clippy::single_call_fn,
  reason = "keep the first well-formed strategy attribute and flag any duplicates"
)]
fn retain_single_strategy_attr(pat_ty: &mut PatType, error: &mut TokenStream) {
  let mut first_strategy_seen = false;
  let mut final_attrs = Vec::with_capacity(pat_ty.attrs.len());
  let old_attrs = take(&mut pat_ty.attrs);

  // every strategy attr should have the form `#[strategy = <expr>]`
  for attr in old_attrs.into_iter().filter(is_strategy) {
    if !matches!(attr.meta, Meta::NameValue(_)) {
      error.extend(quote_spanned! {
          attr.meta.span() => compile_error!("`strategy` attributes must have the form `#[strategy = <expr>]`");
      });
      final_attrs.push(attr);
      continue;
    }
    if first_strategy_seen {
      // a duplicate "good" strategy - emit an error
      let pat = pat_ty.pat.clone().into_token_stream().to_string();
      let message = format!("{pat} has duplicate `#[strategy = ...] attribute`");
      error.extend(quote_spanned! {
          attr.span() => compile_error!(#message);
      });
      continue;
    }
    final_attrs.push(attr);
    first_strategy_seen = true;
  }

  pat_ty.attrs = final_attrs;
}

/// Helper function to generate `compile_error!()` outputs
///
/// The trailing semicolon matters: the caller returns these tokens as the
/// whole macro output, and `compile_error!(...)` without one is malformed in
/// item position, which would bury the real diagnostic under a delimiter
/// error.
fn err(span: &impl Spanned, message: &str) -> Result<(), TokenStream> {
  Err(quote_spanned! { span.span() => compile_error!(#message); })
}

#[cfg(test)]
mod tests {
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;

  /// The full function after validation and its native diagnostic tokens.
  type Observation = (ItemFn, Result<(), TokenStream>);

  /// Parsing failures and retained validation evidence.
  #[derive(Debug, thiserror::Error)]
  enum ValidationFailure {
    /// The parser rejected a fixture.
    #[error(transparent)]
    Parse(#[from] syn::Error),
    /// The complete validation observations did not match the contract.
    #[error(transparent)]
    Observation(#[from] PredicateFailure<Vec<Observation>>),
  }

  /// Validate every fixture while preserving its transformed syntax and diagnostics.
  fn observations(fixtures: &[&str]) -> Result<Vec<Observation>, syn::Error> {
    fixtures
      .iter()
      .map(|source| {
        let mut function = syn::parse_str(source)?;
        let result = validate(&mut function);
        Ok((function, result))
      })
      .collect()
  }

  #[test]
  fn validate_fails_with_self_arg() -> Result<(), ValidationFailure> {
    let observed = observations(&[
      "fn f(self) {}", "fn f(&self) {}", "fn f(&mut self) {}", "fn f(self: Self) {}", "fn f(self: &Self) {}", "fn f(self: &mut Self) {}",
      "fn f(self: Box<Self>) {}", "fn f(self: Rc<Self>) {}", "fn f(self: Arc<Self>) {}",
    ])?;
    ensure_that(observed, "receivers are rejected before unit return diagnostics", |cases| {
      cases.iter().all(|case| {
        case
          .1
          .as_ref()
          .is_err_and(|error| error.to_string().contains("`self` parameters are forbidden"))
      })
    })
    .map(drop)
    .map_err(ValidationFailure::Observation)
  }

  #[test]
  fn validate_fails_with_duplicate() -> Result<(), ValidationFailure> {
    let observed = observations(&["fn f(#[strategy = 1] #[strategy = 2] x: i32) {}"])?;
    ensure_that(
      observed,
      "duplicate strategies are diagnosed before the return type and retain the first strategy",
      |cases| {
        cases.iter().all(|case| {
          case.1.as_ref().is_err_and(|error| error.to_string().contains("duplicate"))
            && case
              .0
              .sig
              .inputs
              .iter()
              .all(|input| matches!(input, FnArg::Typed(parameter) if parameter.attrs.len() == 1))
        })
      },
    )
    .map(drop)
    .map_err(ValidationFailure::Observation)
  }

  #[test]
  fn validate_accepts_result_returning_fn() -> Result<(), ValidationFailure> {
    let observed = observations(&[
      "fn f(x: i32) -> Result<i32, Failure> { check(x) }",
      "fn f(x: i32) -> AssertionResult { check(x) }",
      "fn f(x: i32) -> fn(i32) -> i32 { identity }",
    ])?;
    ensure_that(
      observed,
      "result aliases are resolved by Rust and syn function-pointer nodes remain valid syntax",
      |cases| cases.iter().all(|case| case.1.is_ok()),
    )
    .map(drop)
    .map_err(ValidationFailure::Observation)
  }

  #[test]
  fn validate_rejects_unit_returning_fn() -> Result<(), ValidationFailure> {
    let observed = observations(&["fn f(x: i32) {}", "fn f(x: i32) -> () {}"])?;
    ensure_that(
      observed,
      "implicit and explicit unit returns receive the typed-result diagnostic",
      |cases| {
        cases
          .iter()
          .all(|case| case.1.as_ref().is_err_and(|error| error.to_string().contains("Result<A, E>")))
      },
    )
    .map(drop)
    .map_err(ValidationFailure::Observation)
  }
}
