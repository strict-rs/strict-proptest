use proc_macro2::TokenStream;
use quote::{ToTokens, quote_spanned};
use syn::{FnArg, ItemFn, Meta, ReturnType, Type, spanned::Spanned};

use super::utils::is_strategy;

/// Validate an `ItemFn` for some basic sanity checks
///
/// Many checks are deferred to rustc (e.g. rustc already errors if you make a test function
/// unsafe, so we just transparently pass unsafe through to the generated function and let rustc
/// emit the error)
pub(super) fn validate(f: &mut ItemFn) -> Result<(), TokenStream> {
    all_args_non_self(f)?;
    validate_parameter_attrs(f)?;
    returns_strict_result(f)?;

    Ok(())
}

/// The error emitted for property tests whose signature declares no return
/// type or the literal `-> ()`.
const UNIT_RETURN_ERROR: &str = "strict property tests must return `Result<(), TestFailure>` (`proptest::strict::TestResult`), not `()`; declare `-> proptest::strict::TestResult` and end the body with `Ok(())`";

/// Reject a property test whose signature returns `()`.
///
/// The generated wrapper drives the body through the strict runner as
/// `Result<(), TestFailure>` (`proptest::strict::TestResult`), so a unit body
/// would otherwise only surface as a cryptic type-inference error inside the
/// generated `ensure_property` call; rejecting it here gives a spanned,
/// actionable diagnostic instead. Like the rest of this module the check is
/// purely syntactic: a type alias that resolves to `()` is not caught here
/// and is left to rustc's type error.
fn returns_strict_result(f: &ItemFn) -> Result<(), TokenStream> {
    match &f.sig.output {
        ReturnType::Default => err(&f.sig.ident, UNIT_RETURN_ERROR),
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Tuple(tuple) if tuple.elems.is_empty() => {
                err(ty, UNIT_RETURN_ERROR)
            }
            _ => Ok(()),
        },
    }
}

fn all_args_non_self(f: &mut ItemFn) -> Result<(), TokenStream> {
    let first_self_arg = f
        .sig
        .inputs
        .iter()
        .find(|arg| matches!(arg, FnArg::Receiver(_)));

    match first_self_arg {
        None => Ok(()),
        Some(arg) => err(arg, "`self` parameters are forbidden"),
    }
}

/// Make sure we only have `#[strategy = <expr>]` attributes on function parameters
fn validate_parameter_attrs(f: &mut ItemFn) -> Result<(), TokenStream> {
    let mut error = quote::quote! {};

    for param in &mut f.sig.inputs {
        let FnArg::Typed(pat_ty) = param else {
            unreachable!("should be impossible due to `all_args_non_self`");
        };

        // add error for any non-`strategy` error or inner attributes (i.e. `#![...]` )
        for attr in pat_ty.attrs.iter().filter(|a| !is_strategy(a)) {
            error.extend(quote_spanned! {
                attr.span() => compile_error!("only `#[strategy = <expr>]` attributes are allowed here");
            });
        }

        let mut first_strategy_seen = false;
        let mut final_attrs = Vec::with_capacity(pat_ty.attrs.len());
        let old_attrs = std::mem::take(&mut pat_ty.attrs);

        // every strategy attr should have the form `#[strategy = <expr>]`
        for attr in old_attrs.into_iter().filter(is_strategy) {
            match attr.meta {
                // a "good" strategy - if we see more than one, emit an error
                Meta::NameValue(_) => {
                    if first_strategy_seen {
                        let pat =
                            pat_ty.pat.clone().into_token_stream().to_string();
                        let message = format!(
                            "{pat} has duplicate `#[strategy = ...] attribute`"
                        );
                        error.extend(quote_spanned! {
                            attr.span() => compile_error!(#message);
                        });
                    } else {
                        final_attrs.push(attr);
                        first_strategy_seen = true;
                    }
                }
                _ => {
                    error.extend(quote_spanned! {
                        attr.meta.span() => compile_error!("`strategy` attributes must have the form `#[strategy = <expr>]`");
                    });
                    final_attrs.push(attr);
                }
            }
        }

        pat_ty.attrs = final_attrs;
    }

    if error.is_empty() { Ok(()) } else { Err(error) }
}

/// Helper function to generate `compile_error!()` outputs
///
/// The trailing semicolon matters: the caller returns these tokens as the
/// whole macro output, and `compile_error!(...)` without one is malformed in
/// item position, which would bury the real diagnostic under a delimiter
/// error.
fn err(span: impl Spanned, s: &str) -> Result<(), TokenStream> {
    Err(quote_spanned! { span.span() => compile_error!(#s); })
}

#[cfg(test)]
mod tests {
    use strict_test_support::{
        TestFailure, ensure, ensure_contains, ensure_some,
    };
    use syn::parse_quote;

    use super::*;

    #[test]
    fn validate_fails_with_self_arg() -> Result<(), TestFailure> {
        let invalids = [
            parse_quote! {fn foo(self) {}},
            parse_quote! {fn foo(&self) {}},
            parse_quote! {fn foo(&mut self) {}},
            parse_quote! {fn foo(self: Self) {}},
            parse_quote! {fn foo(self: &Self) {}},
            parse_quote! {fn foo(self: &mut Self) {}},
            parse_quote! {fn foo(self: Box<Self>) {}},
            parse_quote! {fn foo(self: Rc<Self>) {}},
            parse_quote! {fn foo(self: Arc<Self>) {}},
        ];

        for mut invalid in invalids {
            ensure(
                validate(&mut invalid).is_err(),
                "every self-receiver form is rejected",
            )?;
        }
        Ok(())
    }

    #[test]
    fn validate_fails_with_duplicate() -> Result<(), TestFailure> {
        let mut function = parse_quote! {
            fn foo(#[strategy = 1] #[strategy = 2] x: i32) {}
        };

        let error = ensure_some(
            validate(&mut function).err(),
            "duplicate strategy attributes are rejected",
        )?;
        ensure_contains(
            &error.to_string(),
            "compile_error",
            "the duplicate rejection emits a compile_error",
        )
    }

    #[test]
    fn validate_accepts_result_returning_fn() -> Result<(), TestFailure> {
        let mut valid: syn::ItemFn = parse_quote! {
            fn foo(x: i32) -> proptest::strict::TestResult {
                Ok(())
            }
        };
        ensure(
            validate(&mut valid).is_ok(),
            "a TestResult-returning fn is accepted",
        )?;

        let mut spelled_out: syn::ItemFn = parse_quote! {
            fn foo(x: i32) -> Result<(), TestFailure> {
                Ok(())
            }
        };
        ensure(
            validate(&mut spelled_out).is_ok(),
            "a spelled-out Result-returning fn is accepted",
        )
    }

    /// Check one unit-returning fixture: validate must reject it with a
    /// `compile_error` naming the strict return type.
    fn ensure_unit_fixture_rejected(
        mut fixture: syn::ItemFn,
        rejection_context: &'static str,
    ) -> Result<(), TestFailure> {
        let error =
            ensure_some(validate(&mut fixture).err(), rejection_context)?;
        let rendered = error.to_string();
        ensure_contains(
            &rendered,
            "compile_error",
            "the unit rejection emits a compile_error",
        )?;
        ensure_contains(
            &rendered,
            "proptest::strict::TestResult",
            "the unit rejection names the strict return type",
        )
    }

    #[test]
    fn validate_rejects_unit_returning_fn() -> Result<(), TestFailure> {
        ensure_unit_fixture_rejected(
            parse_quote! {
                fn foo(x: i32) {}
            },
            "an implicit unit return is rejected",
        )?;
        ensure_unit_fixture_rejected(
            parse_quote! {
                fn foo(x: i32) -> () {}
            },
            "an explicit unit return is rejected",
        )
    }
}
