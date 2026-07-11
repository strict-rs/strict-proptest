// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Mostly useful utilities for syn used in the crate.

use std::borrow::Borrow;

use syn::{
    Token, parse_quote,
    punctuated::{Pair, Punctuated},
};

//==============================================================================
// General AST manipulation and types
//==============================================================================

/// The payload fields carried by a composite item after derive normalization.
///
/// `syn` distinguishes `Variant`, `Variant()`, and `Variant {}` as three
/// syntactic field forms. For `Arbitrary` derivation, all three forms carry the
/// same payload: no generated fields. This wrapper makes that simplification an
/// explicit derive-internal decision instead of an incidental empty vector.
pub(crate) struct PayloadFields {
    /// The normalized fields which must be generated for this variant.
    fields: Vec<syn::Field>,
}

impl From<syn::Fields> for PayloadFields {
    fn from(fields: syn::Fields) -> Self {
        let normalized_fields = match fields {
            syn::Fields::Named(named_fields) => {
                named_fields.named.into_iter().collect()
            }
            syn::Fields::Unnamed(unnamed_fields) => {
                unnamed_fields.unnamed.into_iter().collect()
            }
            syn::Fields::Unit => Vec::new(),
        };
        Self {
            fields: normalized_fields,
        }
    }
}

impl PayloadFields {
    /// Return whether this variant carries no generated payload fields.
    pub(crate) const fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Borrow the normalized payload fields.
    pub(crate) fn as_slice(&self) -> &[syn::Field] {
        &self.fields
    }

    /// Consume the normalized payload fields.
    pub(crate) fn into_vec(self) -> Vec<syn::Field> {
        self.fields
    }
}

/// Returns true iff the given type is the literal unit type `()`.
/// This is treated the same way by `syn` as a 0-tuple.
#[allow(
    clippy::single_call_fn,
    reason = "recognize the literal unit type () among syn Types"
)]
pub(crate) fn is_unit_type<T: Borrow<syn::Type>>(ty: T) -> bool {
    ty.borrow() == &parse_quote!(())
}

/// Returns the `Self` type (in the literal syntactic sense).
pub(crate) fn self_ty() -> syn::Type {
    parse_quote!(Self)
}

//==============================================================================
// Paths:
//==============================================================================

/// A `::`-separated sequence of path segments — the shape of a simple path's
/// segment list.
type CommaPS = Punctuated<syn::PathSegment, Token![::]>;

/// Returns true iff the path is simple, i.e:
/// just a :: separated list of identifiers.
#[allow(
    clippy::single_call_fn,
    reason = "hold when every path segment is free of generic arguments"
)]
fn is_path_simple(path: &syn::Path) -> bool {
    path.segments.iter().all(|ps| ps.arguments.is_empty())
}

/// Returns true iff lhs matches the rhs.
#[allow(
    clippy::single_call_fn,
    reason = "match a dotted string path against a segment list ident by ident"
)]
fn eq_simple_pathseg(lhs: &str, rhs: &CommaPS) -> bool {
    lhs.split("::")
        .filter(|segment| !segment.trim().is_empty())
        .eq(rhs.iter().map(|ps| ps.ident.to_string()))
}

/// Returns true iff lhs matches the given simple Path.
pub(crate) fn eq_simple_path(mut lhs: &str, rhs: &syn::Path) -> bool {
    if !is_path_simple(rhs) {
        return false;
    }

    if rhs.leading_colon.is_some() {
        let Some(stripped) = lhs.strip_prefix("::") else {
            return false;
        };
        lhs = stripped;
    }

    eq_simple_pathseg(lhs, &rhs.segments)
}

/// Returns true iff the given path matches any of given
/// paths specified as string slices.
pub(crate) fn match_pathsegs(path: &syn::Path, against: &[&str]) -> bool {
    against.iter().any(|needle| eq_simple_path(needle, path))
}

/// Returns true iff the given `PathArguments` is one that has one type
/// applied to it.
#[allow(
    clippy::single_call_fn,
    reason = "hold when a path segment carries exactly one angle-bracketed type argument"
)]
fn pseg_has_single_tyvar(pp: &syn::PathSegment) -> bool {
    use syn::GenericArgument::Type;
    use syn::PathArguments::AngleBracketed;
    if let AngleBracketed(ref ab) = pp.arguments
        && let Some(&Type(_)) = match_singleton(ab.args.iter())
    {
        true
    } else {
        false
    }
}

/// Returns true iff the given type is of the form `PhantomData<TY>` where
/// `TY` can be substituted for any type, including type variables.
#[allow(
    clippy::single_call_fn,
    reason = "recognize PhantomData<T> across its common import spellings"
)]
pub(crate) fn is_phantom_data(path: &syn::Path) -> bool {
    let segs = &path.segments;
    if segs.is_empty() {
        return false;
    }

    let mut prefix_path = path.clone();
    let Some(lseg) = prefix_path.segments.pop().map(Pair::into_value) else {
        return false;
    };

    &lseg.ident == "PhantomData"
        && pseg_has_single_tyvar(&lseg)
        && match_pathsegs(
            &prefix_path,
            &[
                // We hedge a bet that user will never declare
                // their own type named PhantomData.
                // This may give errors, but is worth it usability-wise.
                "",
                "marker",
                "std::marker",
                "core::marker",
                "::std::marker",
                "::core::marker",
            ],
        )
}

/// Extracts a simple non-global path of length 1.
pub(crate) fn extract_simple_path(path: &syn::Path) -> Option<&syn::Ident> {
    match_singleton(&path.segments)
        .filter(|f| !path_is_global(path) && f.arguments.is_empty())
        .map(|f| &f.ident)
}

/// Does the path have a leading `::`?
pub(crate) const fn path_is_global(path: &syn::Path) -> bool {
    path.leading_colon.is_some()
}

//==============================================================================
// General Rust utilities:
//==============================================================================

/// Returns `Some(x)` iff the iterable is singleton and otherwise None.
pub(crate) fn match_singleton<T>(it: impl IntoIterator<Item = T>) -> Option<T> {
    let mut iter = it.into_iter();
    iter.next().filter(|_| iter.next().is_none())
}
