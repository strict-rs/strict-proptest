use proc_macro2::TokenStream;
use quote::quote;
use syn::Block;
use syn::Ident;
use syn::Pat;
use syn::Type;

use crate::property_test::options::Options;
use crate::property_test::utils::Argument;

/// Compose the argument strategy and preserve the annotated callback's native result.
#[allow(
  clippy::single_call_fn,
  reason = "isolate runtime body construction from the wrapper's type projection and item attributes"
)]
pub(super) fn body(block: &Block, args: &[Argument], result: &Type, name: &Ident, options: &Options) -> TokenStream {
  let proptest = options.true_proptest_path();
  let strategies = args.iter().map(|arg| {
    let ty = &arg.pat_ty.ty;
    arg
      .strategy
      .as_ref()
      .map_or_else(|| quote!(#proptest::prelude::any::<#ty>()), |strategy| quote!(#strategy))
  });
  let strategy = if args.is_empty() {
    quote!(#proptest::prelude::any::<()>())
  } else {
    quote!((#(#strategies,)*))
  };
  let patterns = args.iter().map(|arg| &arg.pat_ty.pat);
  let types = args.iter().map(|arg| &arg.pat_ty.ty);
  let labels: Vec<_> = args
    .iter()
    .map(|arg| {
      if let Pat::Ident(ref pattern) = *arg.pat_ty.pat {
        let ident = &pattern.ident;
        quote!(stringify!(#ident))
      } else {
        let pattern = &arg.pat_ty.pat;
        quote!(stringify!(#pattern))
      }
    })
    .collect();
  let context = quote!(concat!(module_path!(), "::", stringify!(#name)));
  let config = options
    .config
    .as_ref()
    .map_or_else(|| quote!(#proptest::strict::strict_default_config()), |config| quote!(#config));
  let run = options.transport.as_ref().map_or_else(
    || quote!(#proptest::strict::ensure_property_with_config(&strategy, #context, config, property)),
    |transport| {
      let constructor = &transport.value;
      let ty = &transport.ty;
      quote! {
        let transport: #ty = #constructor;
        #proptest::strict::ensure_property_with_transport(&strategy, #context, config, transport, property)
      }
    },
  );
  quote!({
    let strategy = #strategy;
    let config = #proptest::test_runner::Config {
      test_name: Some(#context),
      source_file: Some(file!()),
      ..#config
    };
    let property = |input| {
      let evaluate = |(#(#patterns,)*): (#(#types,)*)| -> #result #block;
      #proptest::test_runner::PropertyReturn::into_result(evaluate(input))
    };
    let mut outcome = { #run };
    match &mut outcome {
      Ok(run) => run.argument_labels = &[#(#labels,)*],
      Err(failure) => failure.run.argument_labels = &[#(#labels,)*],
    }
    outcome
  })
}
