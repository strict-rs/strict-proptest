//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for allocator APIs.

use core::cmp;
use core::ops::Range;

#[cfg(feature = "alt-stable")]
use allocator_api2::alloc::AllocError as AllocatorApi2AllocError;
#[cfg(feature = "alt-stable")]
use allocator_api2::alloc::Global as AllocatorApi2Global;

multiplex_alloc!(::alloc::alloc, ::std::alloc);

use crate::arbitrary::StrategyFor;
use crate::arbitrary::any;
use crate::strategy::FilterMap;
#[cfg(any(all(feature = "unstable", not(feature = "alt-stable")), feature = "alt-stable"))]
use crate::strategy::Just;
use crate::strategy::Strategy as _;

/// Candidate `(align_power, size)` pair used to build a checked `Layout`.
type LayoutCandidate = (u8, usize);
/// Function pointer used by the `Layout` filter-map strategy.
type LayoutMapper = fn(LayoutCandidate) -> Option<alloc::Layout>;

#[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
arbitrary!(alloc::Global; alloc::Global);

#[cfg(feature = "alt-stable")]
arbitrary!(AllocatorApi2Global; AllocatorApi2Global);

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

#[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
arbitrary!(alloc::AllocError, Just<Self>; Just(alloc::AllocError));

#[cfg(feature = "alt-stable")]
arbitrary!(AllocatorApi2AllocError, Just<Self>; Just(AllocatorApi2AllocError));
// 2018-07-28 CollectionAllocErr is not currently available outside of using
// the `alloc` crate, which would require a different nightly feature. For now,
// disable.
// arbitrary!(alloc::collections::CollectionAllocErr, TupleUnion<(WeightedStrategy<Just<Self>>,
// WeightedStrategy<Just<Self>>)>; prop_oneof!
// [Just(alloc::collections::CollectionAllocErr::AllocErr),
// Just(alloc::collections::CollectionAllocErr::CapacityOverflow)]);

#[cfg(test)]
mod test {
  #[cfg(feature = "alt-stable")]
  use allocator_api2::alloc::Layout as AllocatorApi2Layout;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::std_facade::Vec;
  use crate::strategy::NewTree;
  use crate::strategy::ValueTree as _;
  use crate::test_runner::TestRunner;

  no_panic_test!(
      layout => alloc::Layout
      //collection_alloc_err => alloc::collections::CollectionAllocErr
  );

  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  no_panic_test!(
      alloc_global => alloc::Global,
      alloc_err => alloc::AllocError
  );

  #[cfg(feature = "alt-stable")]
  no_panic_test!(
      allocator_api2_global => AllocatorApi2Global,
      allocator_api2_alloc_error => AllocatorApi2AllocError,
      allocator_api2_layout => AllocatorApi2Layout
  );

  /// Preserve every native generation outcome in the deterministic layout sample.
  type LayoutSamples = Vec<NewTree<StrategyFor<alloc::Layout>>>;

  #[test]
  fn generated_layouts_have_valid_alignment_and_size() -> Result<(), PredicateFailure<LayoutSamples>> {
    let mut runner = TestRunner::deterministic();
    let strategy = any::<alloc::Layout>();
    let samples = (0..64).map(|_| strategy.new_tree(&mut runner)).collect();
    let valid_layout = |sample: &NewTree<StrategyFor<alloc::Layout>>| {
      let Ok(ref tree) = *sample else {
        return false;
      };
      let layout = tree.current();
      layout.align().is_power_of_two() && layout.size() <= usize::try_from(isize::MAX).unwrap_or(usize::MAX)
    };
    ensure_that(
      samples,
      "generated layouts have power-of-two alignment and bounded allocation size",
      |subjects: &LayoutSamples| subjects.iter().all(valid_layout),
    )
    .map(drop)
  }
}
