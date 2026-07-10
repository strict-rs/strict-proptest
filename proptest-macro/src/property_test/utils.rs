use syn::{AttrStyle, Attribute, Expr, FnArg, ItemFn, Meta, PatType};

/// A parsed argument, with an optional custom strategy
pub(super) struct Argument {
    /// The parameter's pattern and type (`x: i32`), with any strategy
    /// attribute already stripped off.
    pub pat_ty: PatType,
    /// The parameter's `#[strategy = <expr>]` override, if one was given;
    /// `None` falls back to the type's `Arbitrary` strategy.
    pub strategy: Option<Expr>,
}

/// Convert a function to a zero-arg function, and return the args
///
/// Panics on any invalid function
#[allow(
    clippy::single_call_fn,
    reason = "split a function into its argument-less form plus the extracted argument list"
)]
pub(super) fn strip_args(mut f: ItemFn) -> (ItemFn, Vec<Argument>) {
    let args = std::mem::take(&mut f.sig.inputs);
    let args = args
        .into_iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => strip_strategy(arg),
            FnArg::Receiver(_) => panic!(
                "receivers aren't allowed - should be filtered by `validate`"
            ),
        })
        .collect();

    (f, args)
}

/// Split a parameter into its `#[strategy = <expr>]` override (if any) and the
/// remaining `PatType`.
///
/// Assumes `validate` has already rejected malformed or duplicate strategy
/// attributes, so the `panic!`s here mark internal bugs rather than user
/// error.
#[allow(
    clippy::single_call_fn,
    reason = "split one parameter into its strategy override and its bare pattern type"
)]
fn strip_strategy(mut pat_ty: PatType) -> Argument {
    let (strategies, others) = pat_ty.attrs.into_iter().partition(is_strategy);

    pat_ty.attrs = others;

    let strategy = match &strategies[..] {
        [] => None,
        [attr] => match &attr.meta {
            Meta::NameValue(name_value) => Some(name_value.value.clone()),
            _ => panic!("invalid strategies should be filtered by validate"),
        },
        _ => panic!("multiple strategies should be filtered by validate"),
    };

    Argument { pat_ty, strategy }
}

/// Checks if an attribute counts as a "strategy" attribute
///
/// This means:
///  - it is an outer attribute (i.e. `#[...]` not `#![...]`)
///  - it contains `strategy = <expr>`
pub(super) fn is_strategy(attr: &Attribute) -> bool {
    let path_correct = attr
        .path()
        .get_ident()
        .map(|ident| ident == "strategy")
        .unwrap_or(false);

    let has_equals = matches!(&attr.meta, Meta::NameValue(_));

    let is_outer = matches!(attr.style, AttrStyle::Outer);

    path_correct && has_equals && is_outer
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;
    use strict_test_support::{TestFailure, ensure, ensure_eq, ensure_some};
    use syn::parse_quote;

    use super::*;

    #[test]
    fn strip_args_works() -> Result<(), TestFailure> {
        let f = parse_quote! { fn foo(i: i32) {} };
        let (f, mut args) = strip_args(f);

        ensure_eq(
            &f.to_token_stream().to_string(),
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

    // Kept as a `#[should_panic]` contract test on purpose: it pins
    // `strip_args`'s documented invariant that receivers are rejected by
    // `validate` first, so reaching one here is an internal bug, not a
    // user-facing failure path.
    #[test]
    #[should_panic]
    fn strip_args_panics_with_self() {
        let f = parse_quote! { fn foo(self) {} };
        let _unreachable = strip_args(f);
    }

    #[test]
    fn is_strategy_works() -> Result<(), TestFailure> {
        let attr = parse_quote! { #[strategy = 123] };
        ensure(is_strategy(&attr), "outer name-value strategy is accepted")?;

        let attr = parse_quote! { #![strategy = 123] };
        ensure(!is_strategy(&attr), "inner strategy attribute is rejected")?;

        let attr = parse_quote! { #[not_strategy = 123] };
        ensure(!is_strategy(&attr), "other attribute names are rejected")?;

        let attr = parse_quote! { #[strategy(but, no, equals)] };
        ensure(!is_strategy(&attr), "list-form strategy is rejected")?;

        let attr = parse_quote! { #[strategy] };
        ensure(!is_strategy(&attr), "bare strategy attribute is rejected")
    }

    #[test]
    fn strip_strategy_works() -> Result<(), TestFailure> {
        let f = parse_quote! {fn foo(#[strategy = 123] x: i32) {} };
        let Argument { pat_ty, strategy } = ensure_some(
            strip_args(f).1.pop(),
            "one argument extracted from the fixture",
        )?;
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
