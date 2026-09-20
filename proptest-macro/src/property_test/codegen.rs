use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;
use syn::ReturnType;
use syn::Type;
use syn::token::RArrow;

use super::options::Options;
use super::utils::strip_args;

/// Assemble the strategy, typed callback, and runner invocation.
mod test_body;

/// Generate a typed test wrapper whose counterexample is a public argument tuple.
/// The annotated return type is projected through `PropertyReturn`, preserving
/// result aliases and both payload types without interpreting their spelling.
#[allow(
  clippy::single_call_fn,
  reason = "keep typed wrapper generation behind the validated macro expansion entry point"
)]
pub(super) fn generate(item_fn: ItemFn, options: &Options) -> TokenStream {
  let (mut function, args) = match strip_args(item_fn) {
    Ok(stripped) => stripped,
    Err(error) => return error,
  };
  let ReturnType::Type(_, ref result) = function.sig.output else {
    return syn::Error::new_spanned(&function.sig, "property functions must declare Result<A, E>").to_compile_error();
  };
  let proptest = options.true_proptest_path();
  let types = args.iter().map(|arg| &arg.pat_ty.ty);
  let input = quote!((#(#types,)*));
  let success = quote!(<#result as #proptest::test_runner::PropertyReturn>::Success);
  let failure = quote!(<#result as #proptest::test_runner::PropertyReturn>::Failure);
  let transport_error = options.transport.as_ref().map_or_else(
    || quote!(::core::convert::Infallible),
    |transport| {
      let ty = &transport.ty;
      quote!(<#ty as #proptest::test_runner::PropertyTransport<#input, #success, #failure>>::Error)
    },
  );
  let output: Type = match syn::parse2(quote!(#proptest::test_runner::PropertyResult<#input, #success, #failure, #transport_error>)) {
    Ok(output) => output,
    Err(error) => return error.to_compile_error(),
  };
  let body = test_body::body(&function.block, &args, result, &function.sig.ident, options);
  function.sig.output = ReturnType::Type(RArrow::default(), Box::new(output));
  let attributes = &function.attrs;
  let visibility = &function.vis;
  let signature = &function.sig;
  let errors = &options.errors;
  quote! {
    #(#errors)*
    #(#attributes)*
    #[test]
    #visibility #signature #body
  }
}

#[cfg(test)]
mod snapshot_tests {
  use std::path::Path;

  use strict_test_support::TestFailure;
  use strict_test_support::ensure_snapshot;

  use super::*;

  /// Native parsing and snapshot errors remain distinct.
  #[derive(Debug, thiserror::Error)]
  enum ExpansionFailure {
    /// Invalid fixture or generated syntax.
    #[error(transparent)]
    Parse(#[from] syn::Error),
    /// Native snapshot I/O or mismatch.
    #[error(transparent)]
    Snapshot(#[from] TestFailure),
  }

  /// Render the complete expansion through the same parser as rustc-facing output.
  fn expansion(source: &str, options: &str) -> Result<String, syn::Error> {
    let tokens = generate(syn::parse_str(source)?, &syn::parse_str(options)?);
    Ok(prettyplease::unparse(&syn::parse2(tokens)?))
  }

  macro_rules! snapshot_test {
    ($name:ident, $options:literal) => {
      #[test]
      fn $name() -> Result<(), ExpansionFailure> {
        let rendered = expansion(
          include_str!(concat!("codegen/test_data/", stringify!($name), ".rs")),
          $options,
        )?;
        ensure_snapshot(
          &rendered,
          Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/snapshots/",
            stringify!($name),
            ".snap"
          )),
          "generated typed property wrapper",
        )
        .map_err(ExpansionFailure::Snapshot)
      }
    };
  }

  snapshot_test!(simple, "");
  snapshot_test!(many_params, "");
  snapshot_test!(arg_pattern, "");
  snapshot_test!(arg_ident_and_pattern, "");
  snapshot_test!(return_value, "");

  #[test]
  fn renamed_crate_and_transport() -> Result<(), ExpansionFailure> {
    let rendered = expansion(
      "fn transported(x: u32) -> NativeResult { check(x) }",
      "proptest_path = ::renamed, config = configured(), transport = Codec => Codec::new()",
    )?;
    ensure_snapshot(
      &rendered,
      Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/snapshots/renamed_crate_and_transport.snap"
      )),
      "crate override covers typed signature, strategy, callback projection, and transport",
    )
    .map_err(ExpansionFailure::Snapshot)
  }
}
