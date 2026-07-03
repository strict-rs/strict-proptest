use proc_macro2::TokenStream;
use quote::quote;
use syn::{Block, Ident, Pat, parse_quote, parse2};

use crate::property_test::{options::Options, utils::Argument};

use super::{nth_field_name, struct_name};

/// Generate the new test body by putting the struct and arbitrary impl at the
/// start, then handing the labeled strategy to the strict runner: the final
/// expression is the `<proptest>::strict::ensure_property` (or
/// `ensure_property_with_config`) call, whose verdict is the wrapper's return
/// value
pub(super) fn body(
    block: Block,
    args: &[Argument],
    struct_and_impl: TokenStream,
    fn_name: &Ident,
    options: &Options,
) -> Block {
    let struct_name = struct_name(fn_name);

    // convert each arg to `field0: x`
    let struct_fields = args.iter().enumerate().map(|(index, arg)| {
        let pat = arg.pat_ty.pat.as_ref();
        let field_name = nth_field_name(args, index);

        // If the pattern is an ident, we know that the field name is equal to the pattern name.
        // This means we need to avoid generating: `x: x`, which would trigger a lint suggesting
        // shorthand struct initialization.

        match pat {
            // We need to make sure to handle any mutability modifiers here, i.e. if the user wrote
            // `mut x: i32`, we have to generate `mut x`, not `x: mut x`
            //
            // See https://github.com/proptest-rs/proptest/issues/601
            Pat::Ident(i) => match i.mutability {
                Some(mutability) => quote!(#mutability #field_name,),
                None => quote!(#field_name,),
            },
            _ => quote!(#field_name: #pat,),
        }
    });

    // e.g. FooArgs { field0: x, field1: (y, z), }
    let struct_pattern = quote! {
        #struct_name { #(#struct_fields)* }
    };

    let proptest = options.true_proptest_path();

    let context = quote! {
        concat!(module_path!(), "::", stringify!(#fn_name))
    };

    let property = quote! {
        |#proptest::sugar::NamedArguments(_, #struct_pattern)| #block
    };

    // With an explicit `config = <expr>`, `test_name`/`source_file` are still
    // forced over the caller's expression so failure reports keep naming the
    // annotated test (matching the pre-strict runner glue); the config is then
    // used verbatim by the strict runner. Without one, the strict defaults
    // apply (deterministic `STRICT_TEST_SEED` seeding, persistence disabled).
    let run = match options.config.as_ref() {
        None => quote! {
            #proptest::strict::ensure_property(&strategy, #context, #property)
        },
        Some(config) => quote! {
            #proptest::strict::ensure_property_with_config(
                &strategy,
                #context,
                #proptest::test_runner::Config {
                    test_name: Some(concat!(module_path!(), "::", stringify!(#fn_name))),
                    source_file: Some(file!()),
                    ..#config
                },
                #property,
            )
        },
    };

    let tokens = quote!( {

        #struct_and_impl

        let strategy = #proptest::strategy::Strategy::prop_map(
            #proptest::prelude::any::<#struct_name>(),
            |values| #proptest::sugar::NamedArguments(stringify!(#struct_name), values),
        );

        #run
    } );

    // Every accumulated diagnostic is emitted as a full `compile_error!(...);`
    // statement, so this block always parses; the fallback exists so any
    // future emission bug surfaces as a compile error at the use site rather
    // than a proc-macro panic.
    parse2(tokens).unwrap_or_else(|error| {
        let message = error.to_string();
        parse_quote!({
            ::core::compile_error!(#message);
        })
    })
}
