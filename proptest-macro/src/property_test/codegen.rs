use proc_macro2::TokenStream;
use quote::{ToTokens as _, quote};
use syn::{Attribute, Ident, ItemFn, Pat, parse_quote, spanned::Spanned as _};

use super::{
    options::Options,
    utils::{Argument, strip_args},
};

/// Builds the params struct's `Arbitrary` impl (unboxed or boxed path).
mod arbitrary;
/// Assembles the strict-runner body that wraps the user's block.
mod test_body;

/// Generate the modified test function
///
/// The rough process is:
///  - strip out the function args from the provided function
///  - turn them into a struct
///  - implement `Arbitrary` for that struct (simple field-wise impl)
///  - run the property through the strict runner and return its verdict
///
///  Currently, any attributes on parameters are ignored - in the future, we probably want to read
///  these for things like customizing strategies
#[allow(
    clippy::single_call_fn,
    reason = "drive the struct, Arbitrary impl, and body generation into the final test fn"
)]
pub(super) fn generate(item_fn: ItemFn, options: &Options) -> TokenStream {
    let (mut argless_fn, args) = match strip_args(item_fn) {
        Ok(stripped) => stripped,
        Err(error) => return error,
    };

    let struct_tokens = generate_struct(&argless_fn.sig.ident, &args);
    let arb_tokens =
        arbitrary::gen_arbitrary_impl(&argless_fn.sig.ident, &args, options);

    let struct_and_arb = quote! {
        #struct_tokens
        #arb_tokens
    };

    let new_body = test_body::body(
        &argless_fn.block,
        &args,
        &struct_and_arb,
        &argless_fn.sig.ident,
        options,
    );

    *argless_fn.block = new_body;
    argless_fn.attrs.push(test_attr());

    // The wrapper's body ends in the strict runner call, so its declared
    // return type is always the strict verdict, resolved through the
    // configured proptest path (the annotated fn's own return type is
    // consumed by validation, not re-emitted).
    let proptest = options.true_proptest_path();
    argless_fn.sig.output = parse_quote! { -> #proptest::strict::TestResult };

    // Recoverable option errors are emitted at item position beside the
    // generated fn, not inside its body: the `#[test]` wrapper is
    // cfg-stripped outside test builds, so a body-level `compile_error!`
    // would silently vanish there (e.g. under trybuild or rustdoc).
    let errors = &options.errors;
    let fn_tokens = argless_fn.to_token_stream();
    quote! {
        #(#errors)*
        #fn_tokens
    }
}

/// Generate the inner struct that represents the arguments of the function
#[allow(
    clippy::single_call_fn,
    reason = "the derived params struct storing one field per property-test argument"
)]
fn generate_struct(fn_name: &Ident, args: &[Argument]) -> TokenStream {
    let struct_name = struct_name(fn_name);

    let fields = args.iter().enumerate().map(|(index, arg)| {
        let field_name = field_name_for_arg(arg, index);
        let ty = &arg.pat_ty.ty;

        quote! { #field_name: #ty, }
    });

    quote! {
        #[derive(Debug)]
        struct #struct_name {
            #(#fields)*
        }
    }
}

/// Convert the name of a function to the name of a struct representing its args
///
/// E.g. `some_function` -> `SomeFunctionArgs`
fn struct_name(fn_name: &Ident) -> Ident {
    use convert_case::{Case, Casing as _};

    let function_name = fn_name.to_string();
    let pascal_name = function_name.to_case(Case::Pascal);
    let struct_name = format!("{pascal_name}Args");
    Ident::new(&struct_name, fn_name.span())
}

/// The rule for field names is:
/// - if the arguments pattern is an ident, we reuse that ident verbatim
/// - otherwise, we use the name `arg<n>`, where `<n>` is the index of the argument (including
///   ident arguments)
///
/// So for example, given the args `foo: i32, (a, b): (i32, bool), baz: bool`, the generated struct
/// would roughly be:
/// ```rust
/// struct Args {
///     foo: i32,
///     arg1: (i32, bool),
///     baz: bool,
/// }
/// ```
///
fn field_name_for_arg(arg: &Argument, index: usize) -> Ident {
    if let Pat::Ident(ref pat_ident) = *arg.pat_ty.pat {
        pat_ident.ident.clone()
    } else {
        Ident::new(&format!("arg{index}"), arg.pat_ty.pat.span())
    }
}

/// Build the `#[test]` attribute pushed onto the generated wrapper fn.
#[allow(
    clippy::single_call_fn,
    reason = "the literal #[test] attribute appended to the generated wrapper fn"
)]
fn test_attr() -> Attribute {
    parse_quote! { #[test] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strict_test_support::{
        TestFailure, ensure_contains, ensure_eq, ensure_ok, ensure_some,
    };
    use syn::{ItemStruct, parse_quote, parse_str, parse2};

    fn ensure_stripped_args(
        fixture_fn: ItemFn,
    ) -> Result<(ItemFn, Vec<Argument>), TestFailure> {
        strip_args(fixture_fn).map_err(|error| TestFailure::WasErr {
            context: "fixture args strip",
            cause: error.to_string(),
        })
    }

    /// Parse a function and check the generated struct's name and fields,
    /// comparing a rendered `name: ty` listing so failures cite both sides.
    fn check_struct(
        fn_def: &str,
        expected_name: &'static str,
        expected_fields: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Result<(), TestFailure> {
        let fixture_fn: ItemFn =
            ensure_ok(parse_str(fn_def), "fixture fn parses")?;
        let (stripped_fn, args) = ensure_stripped_args(fixture_fn)?;
        let tokens = generate_struct(&stripped_fn.sig.ident, &args);
        let parsed_struct: ItemStruct =
            ensure_ok(parse2(tokens), "generated struct parses")?;

        ensure_eq(
            &parsed_struct.ident.to_string(),
            &expected_name.to_owned(),
            "generated struct name matches",
        )?;

        let mut rendered_fields = Vec::new();
        for field in parsed_struct.fields {
            let name = ensure_some(field.ident, "generated fields are named")?;
            rendered_fields
                .push(format!("{name}: {}", field.ty.to_token_stream()));
        }
        let rendered = rendered_fields.join(", ");
        let expected = expected_fields
            .into_iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        ensure_eq(&rendered, &expected, "generated struct fields match")
    }

    #[test]
    fn derives_debug() -> Result<(), TestFailure> {
        let fixture_fn: ItemFn =
            ensure_ok(parse_str("fn foo(x: i32) {}"), "fixture fn parses")?;
        let (stripped_fn, args) = ensure_stripped_args(fixture_fn)?;
        let string = generate_struct(&stripped_fn.sig.ident, &args).to_string();

        ensure_contains(&string, "derive", "generated struct has a derive")?;
        ensure_contains(&string, "Debug", "generated struct derives Debug")
    }

    #[test]
    fn generates_correct_struct() -> Result<(), TestFailure> {
        check_struct("fn foo() {}", "FooArgs", [])?;
        check_struct("fn foo(x: i32) {}", "FooArgs", [("x", "i32")])?;
        check_struct(
            "fn foo(a: i32, b: String) {}",
            "FooArgs",
            [("a", "i32"), ("b", "String")],
        )
    }

    #[test]
    fn generates_arbitrary_impl() -> Result<(), TestFailure> {
        let fixture_fn: ItemFn = parse_quote! { fn foo(x: i32, y: u8) {} };
        let (stripped_fn, args) = ensure_stripped_args(fixture_fn)?;
        let arb = arbitrary::gen_arbitrary_impl(
            &stripped_fn.sig.ident,
            &args,
            &Options::default(),
        );

        insta::assert_snapshot!(arb.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use strict_test_support::{TestFailure, ensure_ok};
    use syn::parse_str;

    macro_rules! snapshot_test {
        ($name:ident) => {
            snapshot_test!(
                $name,
                $crate::property_test::options::Options::default()
            );
        };
        ($name:ident, $options:expr) => {
            #[test]
            fn $name() -> Result<(), TestFailure> {
                const TEXT: &str = include_str!(concat!(
                    "codegen/test_data/",
                    stringify!($name),
                    ".rs"
                ));

                let parsed =
                    ensure_ok(parse_str(TEXT), "fixture source parses")?;
                let options = $options;
                let tokens = generate(parsed, &options);
                let file = ensure_ok(
                    syn::parse_file(&tokens.to_string()),
                    "generated code parses as a file",
                )?;
                let formatted = prettyplease::unparse(&file);
                insta::assert_snapshot!(formatted);
                Ok(())
            }
        };
    }

    snapshot_test!(simple);
    snapshot_test!(many_params);
    snapshot_test!(arg_pattern);
    snapshot_test!(arg_ident_and_pattern);
    snapshot_test!(return_value);

    mod with_options {
        use super::*;

        snapshot_test!(
            simple,
            Options {
                proptest_path: Some(ensure_ok(
                    parse_str("::hello::world"),
                    "custom proptest_path fixture parses",
                )?),
                ..Options::default()
            }
        );
    }
}
