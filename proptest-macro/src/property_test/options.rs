use proc_macro2::TokenStream;
use quote::{ToTokens, quote, quote_spanned};
use syn::{
    Expr, Ident, LitStr, MetaNameValue, Path, Token, parse::Parse,
    punctuated::Punctuated, spanned::Spanned,
};

/// Options parsed from the attribute itself (e.g. the config from `#[property_test(config = ...)]`)
#[derive(Default)]
pub(super) struct Options {
    /// Collect compiler errors and emit them later, since errors here are largely recoverable
    pub errors: Vec<TokenStream>,
    /// The user's `config = <expr>`, if given. When present, codegen routes
    /// through `ensure_property_with_config` with `test_name` / `source_file`
    /// forced; when absent, the strict defaults apply.
    pub config: Option<Expr>,
    /// The path to the `proptest` crate from `proptest_path = <path>`, for a
    /// re-exported or renamed proptest. `None` means the default `::proptest`.
    pub proptest_path: Option<Path>,
}

impl Options {
    /// Resolve the crate path codegen prefixes onto every emitted item: the
    /// user's `proptest_path` if set, otherwise `::proptest`.
    pub(super) fn true_proptest_path(&self) -> TokenStream {
        match &self.proptest_path {
            None => quote! { ::proptest },
            Some(path) => path.to_token_stream(),
        }
    }
}

/// Validate a `proptest_path = <value>` attribute value: only a plain,
/// qself-free path can name the proptest crate. Returns the path on success,
/// or the spanned `compile_error!` statement to record as a recoverable
/// diagnostic.
#[allow(
    clippy::single_call_fn,
    reason = "validate that a proptest_path value is a bare path to the proptest crate"
)]
fn parse_proptest_path(attr_value: &Expr) -> Result<Path, TokenStream> {
    let bad_path = |span| {
        quote_spanned!(span =>
            compile_error!("argument to `proptest_path` must be a path to the proptest crate, e.g. `proptest_path = ::path::to::proptest`");
        )
    };
    let Expr::Path(path) = attr_value else {
        return Err(bad_path(attr_value.span()));
    };
    if path.qself.is_some() {
        return Err(bad_path(attr_value.span()));
    }
    Ok(path.path.clone())
}

impl Parse for Options {
    // note: this impl takes only the contents of the attr, not the attr itself
    // e.g. it will get `foo = bar, baz = qux`, not `#[macro(foo = bar, baz = qux)]`
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let pairs =
            Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;

        let mut errors = Vec::new();

        let mut config = None;
        let mut proptest_path = None;

        for MetaNameValue {
            path,
            value: attr_value,
            ..
        } in pairs
        {
            let path_string = path.get_ident().map(Ident::to_string);

            match path_string.as_deref() {
                None => errors.push(quote_spanned!(path.span() => compile_error!("unknown argument");)),
                Some("config") => config = Some(attr_value),
                Some("proptest_path") => {
                    match parse_proptest_path(&attr_value) {
                        Ok(path) => proptest_path = Some(path),
                        Err(error) => errors.push(error),
                    }
                }
                Some(other) => {
                    let error_message = format!("unknown argument: {other}");
                    let error_message = LitStr::new(&error_message, other.span());
                    let error = quote_spanned!(other.span() => compile_error!(#error_message););
                    errors.push(error);
                }
            }
        }

        Ok(Self {
            errors,
            config,
            proptest_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use strict_test_support::{
        TestFailure, ensure, ensure_eq, ensure_ok, ensure_some,
    };
    use syn::parse_str;

    use super::*;

    #[test]
    fn simple_parse_example() -> Result<(), TestFailure> {
        let Options {
            errors,
            config,
            proptest_path,
        } = ensure_ok(
            parse_str("config = (), random = 123, proptest_path = ::foo::bar"),
            "the attribute contents parse recoverably",
        )?;

        let proptest_path =
            ensure_some(proptest_path, "the proptest_path value is captured")?;

        ensure(config.is_some(), "the config expression is captured")?;
        ensure_eq(
            &errors.len(),
            &1_usize,
            "the unknown key records one deferred error",
        )?;
        ensure(
            proptest_path.leading_colon.is_some(),
            "the path keeps its leading colons",
        )?;
        let segments = proptest_path
            .segments
            .iter()
            .map(|seg| seg.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        ensure_eq(
            &segments,
            &"foo::bar".to_owned(),
            "the path segments parse in order",
        )
    }

    #[test]
    fn invalid_proptest_path() -> Result<(), TestFailure> {
        let options = ensure_ok(
            parse_str::<Options>("proptest_path = actually::a::function()"),
            "an invalid proptest_path value stays a recoverable parse",
        )?;
        ensure_eq(
            &options.errors.len(),
            &1_usize,
            "the invalid value records one deferred compile_error",
        )
    }
}
