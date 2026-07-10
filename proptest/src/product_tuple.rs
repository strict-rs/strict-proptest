//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Defines macros for product type creation, extraction, and the type signature
//! itself. This version uses tuples.

/// Expands to the tuple *type* wrapping the given factor types.
///
/// A single factor becomes a one-element tuple (`(T,)`) and several factors
/// become the matching tuple type. Used to name the product type that merges
/// several `Arbitrary` parameter types into one.
macro_rules! product_type {
    ($($factor: ty),+) => {
        ($( $factor, )+)
    };
}

/// Expands to the tuple *value* packing the given factor expressions.
///
/// The value-level counterpart of `product_type!`: one factor packs into
/// `(v,)` and several into the matching tuple. Used to build the product value
/// that carries several merged `Arbitrary` parameters.
macro_rules! product_pack {
    ($($factor: expr),+) => {
        ($( $factor, )+)
    };
}

/// Expands to the tuple *pattern* destructuring the given factor patterns.
///
/// The pattern-level counterpart of `product_type!`/`product_pack!`: it binds
/// the individual factors back out of a packed product value.
macro_rules! product_unpack {
    ($($factor: pat),+) => {
        ($( $factor, )+)
    };
}
