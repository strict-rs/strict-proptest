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
#[cfg_attr(test, derive(Debug))]
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
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;

  /// Every native extraction result, including rejected receiver syntax.
  type Extraction = Result<(ItemFn, Vec<Argument>), TokenStream>;

  /// Parsing failures or the complete rejected extraction.
  #[derive(Debug, thiserror::Error)]
  enum ExtractionFailure {
    /// Fixture parser error.
    #[error(transparent)]
    Parse(#[from] syn::Error),
    /// Original extraction subject.
    #[error(transparent)]
    Extraction(#[from] Box<PredicateFailure<Extraction>>),
    /// Complete attribute observations.
    #[error(transparent)]
    Attributes(#[from] PredicateFailure<Vec<(Attribute, bool)>>),
  }

  /// Parse a function and check its complete argument-extraction outcome.
  fn check_extraction(
    source: &str,
    context: &'static str,
    predicate: impl FnOnce(&Extraction) -> bool,
  ) -> Result<Extraction, ExtractionFailure> {
    ensure_that(strip_args(syn::parse_str(source)?), context, predicate)
      .map_err(Box::new)
      .map_err(ExtractionFailure::Extraction)
  }

  #[test]
  fn strip_args_works() -> Result<(), ExtractionFailure> {
    check_extraction(
      "fn foo(mut i: i32, (x, y): (u8, u8)) -> Alias { check(i, x, y) }",
      "stripping retains function return, parameter order, patterns and types",
      |result| {
        let Ok((ref stripped, ref args)) = *result else {
          return false;
        };
        stripped.sig.inputs.is_empty()
          && stripped.sig.ident == "foo"
          && matches!(stripped.sig.output, syn::ReturnType::Type(_, _))
          && args.len() == 2
          && args.first().is_some_and(|arg| {
            matches!(arg.pat_ty.pat.as_ref(), syn::Pat::Ident(pattern)
          if pattern.ident == "i" && pattern.mutability.is_some())
              && arg.strategy.is_none()
          })
          && args
            .get(1)
            .is_some_and(|arg| matches!(arg.pat_ty.pat.as_ref(), syn::Pat::Tuple(tuple) if tuple.elems.len() == 2))
      },
    )
    .map(drop)
  }

  #[test]
  fn strip_args_reports_self() -> Result<(), ExtractionFailure> {
    check_extraction("fn foo(self) {}", "receiver extraction emits its targeted diagnostic", |result| {
      result
        .as_ref()
        .is_err_and(|error| error.to_string().contains("`self` parameters are forbidden"))
    })
    .map(drop)
  }

  #[test]
  fn is_strategy_works() -> Result<(), ExtractionFailure> {
    use syn::parse::Parser as _;
    let fixtures = [
      ("#[strategy = 123]", true),
      ("#[not_strategy = 123]", false),
      ("#[strategy(a)]", false),
      ("#[strategy]", false),
    ];
    let mut observed = Vec::new();
    for (source, expected) in fixtures {
      observed.extend(
        Attribute::parse_outer
          .parse_str(source)?
          .into_iter()
          .map(|attribute| (attribute, expected)),
      );
    }
    observed.extend(
      Attribute::parse_inner
        .parse_str("#![strategy = 123]")?
        .into_iter()
        .map(|attribute| (attribute, false)),
    );
    ensure_that(
      observed,
      "only outer strategy name-value attributes select a strategy",
      |attributes| {
        attributes
          .iter()
          .all(|&(ref attribute, expected)| is_strategy(attribute) == expected)
      },
    )
    .map(drop)
    .map_err(ExtractionFailure::Attributes)
  }

  #[test]
  fn strip_strategy_works() -> Result<(), ExtractionFailure> {
    check_extraction(
      "fn foo(#[strategy = 123] #[retained] x: i32) {}",
      "strategy extraction keeps unrelated attributes and the native expression",
      |result| {
        let Ok((_, ref args)) = *result else {
          return false;
        };
        let [ref arg] = *args.as_slice() else {
          return false;
        };
        let [ref attribute] = *arg.pat_ty.attrs.as_slice() else {
          return false;
        };
        attribute.path().is_ident("retained") && matches!(arg.strategy, Some(Expr::Lit(_)))
      },
    )
    .map(drop)
  }
}
