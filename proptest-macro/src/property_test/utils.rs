use std::mem::take;

use proc_macro2::TokenStream;
use quote::quote_spanned;
use syn::AttrStyle;
use syn::Attribute;
use syn::Expr;
use syn::FnArg;
use syn::ItemFn;
use syn::Meta;
use syn::PatType;
use syn::spanned::Spanned as _;

/// A parsed argument, with an optional custom strategy
pub(super) struct Argument {
  /// The parameter's pattern and type (`x: i32`), with any strategy
  /// attribute already stripped off.
  pub pat_ty:   PatType,
  /// The parameter's `#[strategy = <expr>]` override, if one was given;
  /// `None` falls back to the type's `Arbitrary` strategy.
  pub strategy: Option<Expr>,
}

/// Convert a function to a zero-arg function, and return the args
#[allow(
  clippy::single_call_fn,
  reason = "split a function into its argument-less form plus the extracted argument list"
)]
pub(super) fn strip_args(mut f: ItemFn) -> Result<(ItemFn, Vec<Argument>), TokenStream> {
  let inputs = take(&mut f.sig.inputs);
  let mut arguments = Vec::new();

  for fn_arg in inputs {
    let FnArg::Typed(typed_arg) = fn_arg else {
      return Err(quote_spanned! {
          fn_arg.span() => compile_error!("`self` parameters are forbidden");
      });
    };
    arguments.push(strip_strategy(typed_arg));
  }

  Ok((f, arguments))
}

/// Split a parameter into its `#[strategy = <expr>]` override (if any) and the
/// remaining `PatType`.
#[allow(
  clippy::single_call_fn,
  reason = "split one parameter into its strategy override and its bare pattern type"
)]
fn strip_strategy(mut pat_ty: PatType) -> Argument {
  let (strategies, others) = pat_ty.attrs.into_iter().partition(is_strategy);

  pat_ty.attrs = others;

  let strategy = strategies.iter().find_map(|attr| match attr.meta {
    Meta::NameValue(ref name_value) => Some(name_value.value.clone()),
    Meta::List(_) | Meta::Path(_) => None,
  });

  Argument {
    pat_ty,
    strategy,
  }
}

/// Checks if an attribute counts as a "strategy" attribute
///
/// This means:
///  - it is an outer attribute (i.e. `#[...]` not `#![...]`)
///  - it contains `strategy = <expr>`
pub(super) fn is_strategy(attr: &Attribute) -> bool {
  let path_correct = attr.path().get_ident().is_some_and(|ident| ident == "strategy");

  let has_equals = matches!(&attr.meta, Meta::NameValue(_));

  let is_outer = matches!(attr.style, AttrStyle::Outer);

  path_correct && has_equals && is_outer
}

#[cfg(test)]
mod tests {
  use quote::ToTokens as _;
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_some;
  use syn::parse_quote;

  use super::*;

  fn ensure_stripped_args(fixture_fn: ItemFn) -> Result<(ItemFn, Vec<Argument>), TestFailure> {
    strip_args(fixture_fn).map_err(|error| TestFailure::WasErr {
      context: "fixture args strip",
      cause:   error.to_string(),
    })
  }

  #[test]
  fn strip_args_works() -> Result<(), TestFailure> {
    let fixture_fn = parse_quote! { fn foo(i: i32) {} };
    let (stripped_fn, mut args) = ensure_stripped_args(fixture_fn)?;

    ensure_eq(
      &stripped_fn.to_token_stream().to_string(),
      &"fn foo () { }".to_owned(),
      "stripped fn renders without arguments",
    )?;

    ensure_eq(&args.len(), &1_usize, "exactly one argument extracted")?;
    let arg = ensure_some(args.pop(), "extracted argument is present")?;
    ensure_eq(
      &arg.pat_ty.to_token_stream().to_string(),
      &"i : i32".to_owned(),
      "extracted argument keeps its pattern and type",
    )?;
    ensure(arg.strategy.is_none(), "no strategy attribute extracted")
  }

  #[test]
  fn strip_args_reports_self() -> Result<(), TestFailure> {
    let fixture_fn = parse_quote! { fn foo(self) {} };
    ensure(strip_args(fixture_fn).is_err(), "receiver extraction emits a compile_error")
  }

  #[test]
  fn is_strategy_works() -> Result<(), TestFailure> {
    let outer_name_value = parse_quote! { #[strategy = 123] };
    ensure(is_strategy(&outer_name_value), "outer name-value strategy is accepted")?;

    let inner_name_value = parse_quote! { #![strategy = 123] };
    ensure(!is_strategy(&inner_name_value), "inner strategy attribute is rejected")?;

    let other_name = parse_quote! { #[not_strategy = 123] };
    ensure(!is_strategy(&other_name), "other attribute names are rejected")?;

    let list_form = parse_quote! { #[strategy(but, no, equals)] };
    ensure(!is_strategy(&list_form), "list-form strategy is rejected")?;

    let bare_name = parse_quote! { #[strategy] };
    ensure(!is_strategy(&bare_name), "bare strategy attribute is rejected")
  }

  #[test]
  fn strip_strategy_works() -> Result<(), TestFailure> {
    let fixture_fn = parse_quote! {fn foo(#[strategy = 123] x: i32) {} };
    let Argument {
      pat_ty,
      strategy,
    } = ensure_some(ensure_stripped_args(fixture_fn)?.1.pop(), "one argument extracted from the fixture")?;
    ensure_eq(
      &pat_ty.to_token_stream().to_string(),
      &"x : i32".to_owned(),
      "strategy attribute is stripped from the parameter",
    )?;
    ensure_eq(
      &strategy.to_token_stream().to_string(),
      &"123".to_owned(),
      "strategy expression is extracted",
    )
  }
}
