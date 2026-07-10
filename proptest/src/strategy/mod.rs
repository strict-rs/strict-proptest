//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Defines the core traits used by Proptest.

/// The `Filter` combinator (`Strategy::prop_filter`): rejection sampling
/// that discards generated values a predicate does not accept.
mod filter;
/// The `FilterMap` combinator (`Strategy::prop_filter_map`): a fused map and
/// filter that keeps only the values its closure maps to `Some`.
mod filter_map;
/// The flat-map combinators (`Flatten`, `IndFlatten`, `IndFlattenMap`) that
/// derive a new strategy from each generated value and pick from it.
mod flatten;
/// The `Fuse` adaptor which guards a `ValueTree` against out-of-order or
/// post-`false` `simplify()`/`complicate()` calls.
mod fuse;
/// The constant strategies `Just` and `LazyJust`, which always produce the
/// same value and never shrink.
mod just;
/// `LazyValueTree`, a value tree whose generation is deferred until the first
/// time it is used, letting unions skip branches they never pick.
mod lazy;
/// The value-transforming combinators `Map`, `MapInto`, and `Perturb`, which
/// reshape generated values while shrinking in the source's terms.
mod map;
/// The `Recursive` combinator (`Strategy::prop_recursive`) for generating
/// self-nesting structures with a bounded depth and size.
mod recursive;
/// The `Shuffle` combinator (`Strategy::prop_shuffle`) which permutes the
/// contents of the collections a strategy produces.
mod shuffle;
/// The core `Strategy` and `ValueTree` traits, their boxing adaptors, and the
/// `check_strategy_sanity` contract checker.
mod traits;
/// The weighted-choice combinators `Union` and `TupleUnion` backing
/// `prop_oneof!` and `Strategy::prop_union`.
mod unions;

pub use self::filter::*;
pub use self::filter_map::*;
pub use self::flatten::*;
pub use self::fuse::*;
pub use self::just::*;
pub use self::lazy::*;
pub use self::map::*;
pub use self::recursive::*;
pub use self::shuffle::*;
pub use self::traits::*;
pub use self::unions::*;

pub mod statics;
