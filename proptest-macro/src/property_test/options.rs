use proc_macro2::TokenStream;
use quote::ToTokens;
use quote::quote;
use syn::Expr;
use syn::Path;
use syn::Type;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::token::Comma;
use syn::token::Eq as EqToken;
use syn::token::FatArrow;

/// A declared transport type and the expression constructing its codec.
#[cfg_attr(test, derive(Debug))]
pub(super) struct Transport {
  /// Concrete codec type used in the generated return signature.
  pub ty:    Type,
  /// Codec expression evaluated once when the wrapper runs.
  pub value: Expr,
}

/// Options parsed from the property-test attribute.
#[derive(Default)]
#[cfg_attr(test, derive(Debug))]
pub(super) struct Options {
  /// Recoverable option diagnostics, emitted beside the generated item.
  pub errors:        Vec<TokenStream>,
  /// Explicit runner configuration, retaining all caller-selected fields.
  pub config:        Option<Expr>,
  /// Renamed or re-exported proptest crate path.
  pub proptest_path: Option<Path>,
  /// Explicit typed fork codec, written `transport = Type => expression`.
  pub transport:     Option<Transport>,
}

impl Options {
  /// Resolve every emitted proptest reference through the selected crate path.
  pub(super) fn true_proptest_path(&self) -> TokenStream {
    self
      .proptest_path
      .as_ref()
      .map_or_else(|| quote!(::proptest), ToTokens::to_token_stream)
  }
}

/// Validate a qself-free path naming the proptest crate.
#[allow(
  clippy::single_call_fn,
  reason = "keep crate-path validation and its targeted diagnostic separate from option-list parsing"
)]
fn parse_proptest_path(expression: &Expr) -> Result<Path, TokenStream> {
  if let Expr::Path(ref path) = *expression
    && path.qself.is_none()
  {
    return Ok(path.path.clone());
  }
  Err(
    syn::Error::new_spanned(
      expression,
      "argument to `proptest_path` must be a path to the proptest crate, e.g. `proptest_path = ::path::to::proptest`",
    )
    .to_compile_error(),
  )
}

impl Parse for Options {
  fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
    let mut options = Self::default();
    while !input.is_empty() {
      let path: Path = input.parse()?;
      let EqToken {
        ..
      } = input.parse()?;
      if path.is_ident("transport") {
        let ty = input.parse()?;
        let FatArrow {
          ..
        } = input.parse()?;
        let constructor = input.parse()?;
        options.transport = Some(Transport {
          ty,
          value: constructor,
        });
      } else if path.is_ident("config") {
        options.config = Some(input.parse()?);
      } else if path.is_ident("proptest_path") {
        match parse_proptest_path(&input.parse()?) {
          Ok(crate_path) => options.proptest_path = Some(crate_path),
          Err(error) => options.errors.push(error),
        }
      } else {
        drop(input.parse::<Expr>()?);
        let message = path
          .get_ident()
          .map_or_else(|| "unknown argument".to_owned(), |name| format!("unknown argument: {name}"));
        options.errors.push(syn::Error::new_spanned(path, message).to_compile_error());
      }
      if !input.is_empty() {
        let Comma {
          ..
        } = input.parse()?;
      }
    }
    Ok(options)
  }
}

#[cfg(test)]
mod tests {
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;

  /// Full parser outcome, including every recoverable diagnostic.
  type Observation = Result<Options, syn::Error>;

  #[test]
  fn simple_parse_example() -> Result<(), Box<PredicateFailure<Observation>>> {
    ensure_that(
      syn::parse_str("config = (), random = 123, proptest_path = ::foo::bar"),
      "options retain configuration, one recoverable error, and the complete crate path",
      |result: &Observation| {
        let Ok(ref options) = *result else {
          return false;
        };
        options.config.is_some()
          && options.errors.len() == 1
          && options.proptest_path.as_ref().is_some_and(|path| {
            path.leading_colon.is_some() && path.segments.iter().map(|segment| segment.ident.to_string()).eq(["foo", "bar"])
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn invalid_proptest_path() -> Result<(), Box<PredicateFailure<Observation>>> {
    ensure_that(
      syn::parse_str("proptest_path = actually::a::function()"),
      "an expression cannot silently become a crate path",
      |result: &Observation| {
        result
          .as_ref()
          .is_ok_and(|options| options.proptest_path.is_none() && options.errors.len() == 1)
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn transport_preserves_type_and_constructor() -> Result<(), Box<PredicateFailure<Observation>>> {
    ensure_that(
      syn::parse_str("transport = Codec<u32> => Codec::new(7), config = configured(),"),
      "transport keeps its declared type and independently evaluated constructor",
      |result: &Observation| {
        let Ok(ref options) = *result else {
          return false;
        };
        options.errors.is_empty()
          && options.config.is_some()
          && options.transport.as_ref().is_some_and(|transport| {
            matches!(&transport.ty, Type::Path(path)
            if path.path.segments.first().is_some_and(|segment| segment.ident == "Codec"))
              && matches!(transport.value, Expr::Call(_))
          })
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn transport_requires_a_declared_type() -> Result<(), Box<PredicateFailure<Observation>>> {
    ensure_that(
      syn::parse_str("transport = Codec::new()"),
      "transport without a type and constructor separator is rejected",
      Result::is_err,
    )
    .map(drop)
    .map_err(Box::new)
  }
}
