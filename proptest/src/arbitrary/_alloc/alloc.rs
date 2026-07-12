//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::hash`.

use core::cmp;
use core::ops::Range;

multiplex_alloc!(::alloc::alloc, ::std::alloc);

use crate::arbitrary::StrategyFor;
use crate::arbitrary::any;
use crate::strategy::FilterMap;
use crate::strategy::Just;
use crate::strategy::Strategy as _;

/// Candidate `(align_power, size)` pair used to build a checked `Layout`.
type LayoutCandidate = (u8, usize);
/// Function pointer used by the `Layout` filter-map strategy.
type LayoutMapper = fn(LayoutCandidate) -> Option<alloc::Layout>;

arbitrary!(alloc::Global; alloc::Global);

// Not Debug.
// lazy_just!(System, || System);

arbitrary!(
    alloc::Layout,
    FilterMap<(Range<u8>, StrategyFor<usize>), LayoutMapper>;
    {
        let mapper: LayoutMapper = |(align_power, size)| {
            // 1. align must be a power of two and <= (1 << 31):
            // 2. "when rounded up to the nearest multiple of align, must not overflow".
            let align = 1_usize.checked_shl(u32::from(align_power))?;
            // TODO: This may only work on 64 bit processors, but previously it was broken
            // even on 64 bit so still an improvement. 63 -> uint size - 1.
            let max_size = (1_usize << (usize::BITS - 1)).checked_sub(align)?;
            // Not quite a uniform distribution due to clamping,
            // but probably good enough.
            let clamped_size = cmp::min(max_size, size);
            alloc::Layout::from_size_align(clamped_size, align).ok()
        };
        (0_u8..32_u8, any::<usize>()).prop_filter_map(
            "layout align and rounded size must be valid",
            mapper,
        )
    }
);

arbitrary!(alloc::AllocError, Just<Self>; Just(alloc::AllocError));
// 2018-07-28 CollectionAllocErr is not currently available outside of using
// the `alloc` crate, which would require a different nightly feature. For now,
// disable.
// arbitrary!(alloc::collections::CollectionAllocErr, TupleUnion<(WeightedStrategy<Just<Self>>,
// WeightedStrategy<Just<Self>>)>; prop_oneof!
// [Just(alloc::collections::CollectionAllocErr::AllocErr),
// Just(alloc::collections::CollectionAllocErr::CapacityOverflow)]);

#[cfg(test)]
mod test {
  use super::*;

  no_panic_test!(
      layout => alloc::Layout,
      alloc_err => alloc::AllocError
      //collection_alloc_err => alloc::collections::CollectionAllocErr
  );
}
