// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Provides `UseTracker` as well as `UseMarkable` which is used to
//! track uses of type variables that need `Arbitrary` bounds in our
//! impls.

use std::borrow::Borrow;
use std::collections::HashSet;

use syn::Token;

use crate::attr;
use crate::error::{Ctx, DeriveResult};
use crate::util;

//==============================================================================
// API: Type variable use tracking
//==============================================================================

/// `UseTracker` tracks what type variables that have used in `any_with::<Type>`
/// or similar and thus needs an `Arbitrary` bound added to them.
pub(crate) struct UseTracker {
    /// Tracks 'usage' of a type variable name.
    /// Allocation of this "map" will happen at once and no further
    /// allocation will happen after that. Only potential updates
    /// will happen after initial allocation.
    /// We need to preserve insertion order, thus using `Vec` instead of
    /// `BTreeMap` or `HashMap`. A potential alternative would be indexmap
    /// crate, but our maps are so small that it would not bring any
    /// significant benefit.
    used_map: Vec<(syn::Ident, bool)>,
    /// Extra types to bound by `Arbitrary` in the `where` clause.
    where_types: HashSet<syn::Type>,
    /// The generics that we are doing this for.
    /// This what we will modify later once we're done.
    generics: syn::Generics,
    /// If set to `true`, then `mark_used` has no effect.
    track: bool,
}

/// Models a thing that may have type variables in it that
/// can be marked as 'used' as defined by `UseTracker`.
pub(crate) trait UseMarkable {
    /// Walk `self` and mark every generic type variable it uses on the
    /// `tracker`, so those variables receive an `Arbitrary` bound.
    fn mark_uses(&self, tracker: &mut UseTracker);
}

impl UseTracker {
    /// Constructs the tracker for the given `generics`.
    #[allow(
        clippy::single_call_fn,
        reason = "constructs the UseTracker, seeding every generic param as initially unused"
    )]
    pub(crate) fn new(generics: syn::Generics) -> Self {
        // Construct the map by setting all type variables as being unused
        // initially. This is the only time we will allocate for the map.
        let used_map = generics
            .type_params()
            .map(|type_param| (type_param.ident.clone(), false))
            .collect();
        Self {
            generics,
            used_map,
            where_types: HashSet::default(),
            track: true,
        }
    }

    /// Stop tracking. `.mark_used` will have no effect.
    pub(crate) fn no_track(&mut self) {
        self.track = false;
    }

    /// Mark the _potential_ type variable `tyvar` as used.
    /// If the tracker does not know about the name, it is not
    /// a type variable and this call has no effect.
    fn use_tyvar(&mut self, tyvar: impl Borrow<syn::Ident>) {
        let tyvar = tyvar.borrow();
        if self.track
            && let Some(used) = self
                .used_map
                .iter_mut()
                .find_map(|(ty, used)| (ty == tyvar).then_some(used))
        {
            *used = true;
        }
    }

    /// Returns true iff the type variable given exists.
    fn has_tyvar(&self, ty_var: impl Borrow<syn::Ident>) -> bool {
        self.used_map.iter().any(|(ty, _)| ty == ty_var.borrow())
    }

    /// Mark the type as used.
    fn use_type(&mut self, ty: syn::Type) {
        let _new_projection = self.where_types.insert(ty);
    }

    /// Adds the bound in `for_used` on used type variables and
    /// the bound in `for_not` (`if .is_some()`) on unused type variables.
    pub(crate) fn add_bounds(
        &mut self,
        ctx: Ctx<'_>,
        for_used: &syn::TypeParamBound,
        for_not: Option<syn::TypeParamBound>,
    ) -> DeriveResult<()> {
        if let Some(for_not) = for_not {
            self.bound_all_params(ctx, for_used, &for_not)?;
        } else {
            self.bound_used_params_only(for_used);
        }

        self.generics.make_where_clause().predicates.extend(
            self.where_types.iter().cloned().map(|ty| {
                syn::WherePredicate::Type(syn::PredicateType {
                    lifetimes: None,
                    bounded_ty: ty,
                    colon_token: <Token![:]>::default(),
                    bounds: ::std::iter::once(for_used.clone()).collect(),
                })
            }),
        );

        Ok(())
    }

    /// Bound every type parameter: used parameters (without
    /// `#[proptest(no_bound)]`) get `for_used`, all others get `for_not`.
    fn bound_all_params(
        &mut self,
        ctx: Ctx<'_>,
        for_used: &syn::TypeParamBound,
        for_not: &syn::TypeParamBound,
    ) -> DeriveResult<()> {
        self.used_map
            .iter()
            .map(|(_, used)| used)
            .zip(self.generics.type_params_mut())
            .try_for_each(|(&used, tv)| {
                // Steal the attributes:
                let no_bound = attr::has_no_bound(ctx, &tv.attrs)?;
                let bound = if used && !no_bound { for_used } else { for_not };
                tv.bounds.push(bound.clone());
                Ok(())
            })
    }

    /// Bound only the used type parameters with `for_used`, leaving unused
    /// parameters unbounded.
    fn bound_used_params_only(&mut self, for_used: &syn::TypeParamBound) {
        self.used_map
            .iter()
            .map(|(_, used)| used)
            .zip(self.generics.type_params_mut())
            .for_each(|(&used, tv)| {
                if used {
                    tv.bounds.push(for_used.clone())
                }
            })
    }

    /// Consumes the (potentially) modified generics that the
    /// tracker was originally constructed with and returns it.
    pub(crate) fn consume(self) -> syn::Generics {
        self.generics
    }
}

//==============================================================================
// Impls
//==============================================================================

impl UseMarkable for syn::Type {
    fn mark_uses(&self, ut: &mut UseTracker) {
        syn::visit::visit_type(&mut PathVisitor(ut), self);
    }
}

/// The generic-usage walker behind `mark_uses`: it marks simple-path
/// identifiers as used type variables, records associated-type projections
/// of a generic for `where` bounds, and deliberately skips macro bodies and
/// `PhantomData<T>` innards.
struct PathVisitor<'ut>(&'ut mut UseTracker);

impl syn::visit::Visit<'_> for PathVisitor<'_> {
    fn visit_macro(&mut self, _: &syn::Macro) {}

    fn visit_type_path(&mut self, tpath: &syn::TypePath) {
        if matches_prj_tyvar(self.0, tpath) {
            self.0.use_type(adjust_simple_prj(tpath).into());
            return;
        }
        syn::visit::visit_type_path(self, tpath);
    }

    fn visit_path(&mut self, path: &syn::Path) {
        // If path is PhantomData<T> do not mark innards.
        if util::is_phantom_data(path) {
            return;
        }

        if let Some(ident) = util::extract_simple_path(path) {
            self.0.use_tyvar(ident);
        }

        syn::visit::visit_path(self, path);
    }
}

/// Returns true iff `tpath` is an associated-type projection rooted at a
/// tracked generic (e.g. `T::Assoc` or `<T as Trait>::Assoc`), which needs a
/// `where` bound rather than a bound on the parameter itself.
fn matches_prj_tyvar(ut: &mut UseTracker, tpath: &syn::TypePath) -> bool {
    let path = &tpath.path;
    let segs = &path.segments;

    if let Some(qself) = &tpath.qself {
        // < $qself > :: $path
        if let Some(sub_tp) = extract_path(&qself.ty) {
            return sub_tp.qself.is_none()
                && util::match_singleton(segs.iter().skip(qself.position))
                    .filter(|ps| ps.arguments.is_empty())
                    .and_then(|_| util::extract_simple_path(&sub_tp.path))
                    .filter(|&ident| ut.has_tyvar(ident))
                    .is_some() // < $tyvar as? $path? > :: $path
                || matches_prj_tyvar(ut, sub_tp);
        }

        false
    } else {
        // true => $tyvar :: $projection
        !util::path_is_global(path)
            && segs.len() == 2
            && ut.has_tyvar(&segs[0].ident)
            && segs[0].arguments.is_empty()
            && segs[1].arguments.is_empty()
    }
}

/// Normalize a projection written with a bare qself (`<T>::Assoc`) into the
/// equivalent qself-free path (`T::Assoc`) so it can be stored as a single
/// `where`-bounded type; any other path is returned unchanged.
#[allow(
    clippy::single_call_fn,
    reason = "normalizes a bare-qself associated-type projection into a qself-free path"
)]
fn adjust_simple_prj(tpath: &syn::TypePath) -> syn::TypePath {
    let segments = tpath
        .qself
        .as_ref()
        .filter(|qp| qp.as_token.is_none())
        .and_then(|qp| extract_path(&qp.ty))
        .filter(|tp| tp.qself.is_none())
        .map(|tp| &tp.path.segments);

    if let Some(segments) = segments {
        let tpath = tpath.clone();
        let mut segments = segments.clone();
        segments.push_punct(<Token![::]>::default());
        segments.extend(tpath.path.segments.into_pairs());
        syn::TypePath {
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments,
            },
        }
    } else {
        tpath.clone()
    }
}

/// Returns the underlying `TypePath` if `ty` is a path type, else `None`.
fn extract_path(ty: &syn::Type) -> Option<&syn::TypePath> {
    if let syn::Type::Path(tpath) = ty {
        Some(tpath)
    } else {
        None
    }
}
