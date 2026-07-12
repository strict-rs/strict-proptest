//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::sync`.

use core::sync::atomic::AtomicBool;
#[cfg(feature = "unstable")]
use core::sync::atomic::AtomicI8;
#[cfg(feature = "unstable")]
use core::sync::atomic::AtomicI16;
#[cfg(feature = "unstable")]
use core::sync::atomic::AtomicI32;
#[cfg(all(feature = "unstable", feature = "atomic64bit"))]
use core::sync::atomic::AtomicI64;
use core::sync::atomic::AtomicIsize;
#[cfg(feature = "unstable")]
use core::sync::atomic::AtomicU8;
#[cfg(feature = "unstable")]
use core::sync::atomic::AtomicU16;
#[cfg(feature = "unstable")]
use core::sync::atomic::AtomicU32;
#[cfg(all(feature = "unstable", feature = "atomic64bit"))]
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use crate::arbitrary::SMapped;
use crate::arbitrary::any;
use crate::std_facade::Arc;
use crate::strategy::Just;
use crate::strategy::TupleUnion;
use crate::strategy::WeightedStrategy;
use crate::strategy::statics::static_map;

wrap_from!(Arc);

/// Implements `Arbitrary` for atomic integer and bool types.
///
/// Each `AtomicType, base` pair generates an arbitrary value of the primitive
/// `base` and passes it to `AtomicType::new` via `static_map`
/// (`Strategy = SMapped<$base, Self>`).
macro_rules! atomic {
    ($($type: ident, $base: ty);+) => {
        $(arbitrary!($type, SMapped<$base, Self>;
            static_map(any::<$base>(), $type::new)
        );)+
    };
}

// impl_wrap_gen!(AtomicPtr); // We don't have impl Arbitrary for *mut T yet.
atomic!(AtomicBool, bool; AtomicIsize, isize; AtomicUsize, usize);

#[cfg(feature = "unstable")]
atomic!(AtomicI8, i8; AtomicI16, i16; AtomicI32, i32;
        AtomicU8, u8; AtomicU16, u16; AtomicU32, u32);

#[cfg(all(feature = "unstable", feature = "atomic64bit"))]
atomic!(AtomicI64, i64; AtomicU64, u64);

arbitrary!(Ordering,
    TupleUnion<(WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>,
                WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>)>;
    prop_oneof![
        Just(Ordering::Relaxed),
        Just(Ordering::Release),
        Just(Ordering::Acquire),
        Just(Ordering::AcqRel),
        Just(Ordering::SeqCst)
    ]
);

#[cfg(test)]
mod test {
  use super::*;

  no_panic_test!(
      arc => Arc<u8>,
      atomic_bool => AtomicBool,
      atomic_isize => AtomicIsize,
      atomic_usize => AtomicUsize,
      ordering => Ordering
  );

  #[cfg(feature = "unstable")]
  no_panic_test!(
      atomic_i8  => AtomicI8,
      atomic_i16 => AtomicI16,
      atomic_i32 => AtomicI32,
      atomic_u8  => AtomicU8,
      atomic_u16 => AtomicU16,
      atomic_u32 => AtomicU32
  );

  #[cfg(all(feature = "unstable", feature = "atomic64bit"))]
  no_panic_test!(
      atomic_i64 => AtomicI64,
      atomic_u64 => AtomicU64
  );
}
