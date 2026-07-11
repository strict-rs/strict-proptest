//-
// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::convert::TryFrom as _;
use core::num::{
    NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroIsize, NonZeroU8,
    NonZeroU16, NonZeroU32, NonZeroU64, NonZeroUsize,
};
#[cfg(not(target_arch = "wasm32"))]
use core::num::{NonZeroI128, NonZeroU128};

use crate::arbitrary::{Arbitrary, StrategyFor, any};
use crate::strategy::{FilterMap, Strategy as _};

/// Implements `Arbitrary` for a `NonZero` integer type over its primitive.
///
/// Generates the underlying primitive with `any::<$prim>()` and
/// `prop_filter_map`s it through `TryFrom`, rejecting `0` with the message
/// `"must be non zero"` (`Strategy = FilterMap<StrategyFor<$prim>,
/// fn($prim) -> Option<Self>>`).
macro_rules! non_zero_impl {
    ($nz:ty, $prim:ty) => {
        impl Arbitrary for $nz {
            type Parameters = ();
            type Strategy =
                FilterMap<StrategyFor<$prim>, fn($prim) -> Option<Self>>;

            fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
                any::<$prim>().prop_filter_map("must be non zero", |i| {
                    Self::try_from(i).ok()
                })
            }
        }
    };
}

non_zero_impl!(NonZeroU8, u8);
non_zero_impl!(NonZeroU16, u16);
non_zero_impl!(NonZeroU32, u32);
non_zero_impl!(NonZeroU64, u64);
#[cfg(not(target_arch = "wasm32"))]
non_zero_impl!(NonZeroU128, u128);
non_zero_impl!(NonZeroUsize, usize);

non_zero_impl!(NonZeroI8, i8);
non_zero_impl!(NonZeroI16, i16);
non_zero_impl!(NonZeroI32, i32);
non_zero_impl!(NonZeroI64, i64);
#[cfg(not(target_arch = "wasm32"))]
non_zero_impl!(NonZeroI128, i128);
non_zero_impl!(NonZeroIsize, isize);

#[cfg(test)]
mod test {
    use core::num::{
        NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroIsize, NonZeroU8,
        NonZeroU16, NonZeroU32, NonZeroU64, NonZeroUsize,
    };
    #[cfg(not(target_arch = "wasm32"))]
    use core::num::{NonZeroI128, NonZeroU128};

    no_panic_test!(
        u8 => NonZeroU8,
        u16 => NonZeroU16,
        u32 => NonZeroU32,
        u64 => NonZeroU64,
        usize => NonZeroUsize,
        i8 => NonZeroI8,
        i16 => NonZeroI16,
        i32 => NonZeroI32,
        i64 => NonZeroI64,
        isize => NonZeroIsize
    );
    #[cfg(not(target_arch = "wasm32"))]
    no_panic_test!(
        u128 => NonZeroU128,
        i128 => NonZeroI128
    );
}
