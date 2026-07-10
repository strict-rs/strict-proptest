//-
// Copyright 2017, 2018, 2026 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies to generate numeric values (as opposed to integers used as bit
//! fields).
//!
//! All strategies in this module shrink by binary searching towards 0.

mod float_samplers;

use crate::test_runner::TestRunner;
use bitflags::bitflags;
use rand::distr::uniform::{SampleUniform, Uniform};
use rand::distr::{Distribution, StandardUniform};

/// Generate a random value of `X`, sampled uniformly from the half
/// open range `[low, high)` (excluding `high`). Panics if `low >= high`.
pub(crate) fn sample_uniform<X: SampleUniform>(
    run: &mut TestRunner,
    start: X,
    end: X,
) -> X {
    Uniform::new(start, end)
        .expect("not uniform")
        .sample(run.rng())
}

/// Generate a random value of `X`, sampled uniformly from the closed
/// range `[low, high]` (inclusive).
///
/// # Panics
///
/// Panics if `low > high`, i.e. the range is empty and has no value to draw.
pub fn sample_uniform_incl<X: SampleUniform>(
    run: &mut TestRunner,
    start: X,
    end: X,
) -> X {
    Uniform::new_inclusive(start, end)
        .expect("not uniform")
        .sample(run.rng())
}

/// Defines a pair of uniform samplers that go through a wider integer type.
///
/// For a target type `$to` sampled via `$from` (e.g. `usize` via `u64`), emits
/// `$name` (half-open `[start, end)`) and `$incl` (closed `[start, end]`),
/// which cast through `$from` before sampling to reach types `rand` cannot
/// sample directly.
macro_rules! sample_uniform {
    ($name: ident, $incl:ident, $from:ty, $to:ty) => {
        fn $name(run: &mut TestRunner, start: $to, end: $to) -> $to {
            Uniform::<$from>::new(start as $from, end as $from)
                .expect("not uniform")
                .sample(run.rng()) as $to
        }

        fn $incl(run: &mut TestRunner, start: $to, end: $to) -> $to {
            Uniform::<$from>::new_inclusive(start as $from, end as $from)
                .expect("not uniform")
                .sample(run.rng()) as $to
        }
    };
}

/// Dispatches to a uniform sampler in either `generic` or `plain` mode.
///
/// In `generic` mode the bounds are `.into()`-converted to the sample type
/// (used by the primitive ranges); in `plain` mode they are forwarded as-is
/// (used where the value type already is the sample type).
macro_rules! sample_uniform_value {
    (
        generic,
        $uniform:ident,
        $sample_typ:ty,
        $runner:expr,
        $start:expr,
        $end:expr $(,)?
    ) => {
        $crate::num::$uniform::<$sample_typ>(
            $runner,
            $start.into(),
            $end.into(),
        )
    };
    (
        plain,
        $uniform:ident,
        $sample_typ:ty,
        $runner:expr,
        $start:expr,
        $end:expr $(,)?
    ) => {
        $crate::num::$uniform($runner, $start, $end)
    };
}

#[cfg(target_pointer_width = "64")]
sample_uniform!(usize_sample_uniform, usize_sample_uniform_incl, u64, usize);
#[cfg(target_pointer_width = "32")]
sample_uniform!(usize_sample_uniform, usize_sample_uniform_incl, u32, usize);
#[cfg(target_pointer_width = "16")]
sample_uniform!(usize_sample_uniform, usize_sample_uniform_incl, u16, usize);

#[cfg(target_pointer_width = "64")]
sample_uniform!(isize_sample_uniform, isize_sample_uniform_incl, i64, isize);
#[cfg(target_pointer_width = "32")]
sample_uniform!(isize_sample_uniform, isize_sample_uniform_incl, i32, isize);
#[cfg(target_pointer_width = "16")]
sample_uniform!(isize_sample_uniform, isize_sample_uniform_incl, i16, isize);

/// Draws a fully arbitrary integer straight from the RNG's `random()`.
///
/// Used for the integer types `rand` samples natively.
macro_rules! supported_int_any {
    ($runner:ident, $typ:ty) => {
        $runner.rng().random()
    };
}

/// Draws a fully arbitrary integer for a type the RNG cannot sample directly.
///
/// Falls back to a raw `next_u64` word cast to the target type, used for
/// `usize`/`isize` on 64-bit targets.
#[cfg(target_pointer_width = "64")]
macro_rules! unsupported_int_any {
    ($runner:ident, $typ:ty) => {
        $runner.rng().next_u64() as $typ
    };
}

/// Draws a fully arbitrary integer for a type the RNG cannot sample directly.
///
/// Falls back to a raw `next_u32` word cast to the target type, used for
/// `usize`/`isize` on non-64-bit targets.
#[cfg(not(target_pointer_width = "64"))]
macro_rules! unsupported_int_any {
    ($runner:ident, $typ:ty) => {
        $runner.rng().next_u32() as $typ
    };
}

/// Defines the `Any` strategy type and its `ANY` constant for one integer
/// type.
///
/// The generated `Any` produces completely arbitrary values (via `$int_any`)
/// and shrinks them through the module's `BinarySearch` toward `0`.
macro_rules! int_any {
    ($typ: ident, $int_any: ident) => {
        /// Type of the `ANY` constant.
        #[derive(Clone, Copy, Debug)]
        #[must_use = "strategies do nothing unless used"]
        pub struct Any(());
        /// Generates integers with completely arbitrary values, uniformly
        /// distributed over the whole range.
        pub const ANY: Any = Any(());

        impl Strategy for Any {
            type Tree = BinarySearch;
            type Value = $typ;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                Ok(BinarySearch::new($int_any!(runner, $typ)))
            }
        }
    };
}

/// Implements `Strategy` for every `Range*` shape over one numeric type.
///
/// A single invocation wires up `Range`, `RangeInclusive`, `RangeFrom`,
/// `RangeTo`, and `RangeToInclusive`, so that (for example) `0..10` is directly
/// usable as a strategy; each samples uniformly within its bounds and shrinks
/// via `BinarySearch`. The leading selector chooses the sampling mode and the
/// concrete uniform helpers.
macro_rules! numeric_api {
    (@with_mode
        $sample_mode:ident,
        $typ:ident,
        $sample_typ:ty,
        $epsilon:expr,
        $uniform:ident,
        $incl:ident
    ) => {
        impl Strategy for ::core::ops::Range<$typ> {
            type Tree = BinarySearch;
            type Value = $typ;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                if self.is_empty() {
                    panic!(
                        "Invalid use of empty range {}..{}.",
                        self.start, self.end
                    );
                }

                Ok(BinarySearch::new_clamped(
                    self.start,
                    sample_uniform_value!(
                        $sample_mode,
                        $uniform,
                        $sample_typ,
                        runner,
                        self.start,
                        self.end,
                    )
                    .into(),
                    self.end - $epsilon,
                ))
            }
        }

        impl Strategy for ::core::ops::RangeInclusive<$typ> {
            type Tree = BinarySearch;
            type Value = $typ;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                if self.is_empty() {
                    panic!(
                        "Invalid use of empty range {}..={}.",
                        self.start(),
                        self.end()
                    );
                }

                Ok(BinarySearch::new_clamped(
                    *self.start(),
                    sample_uniform_value!(
                        $sample_mode,
                        $incl,
                        $sample_typ,
                        runner,
                        *self.start(),
                        *self.end(),
                    )
                    .into(),
                    *self.end(),
                ))
            }
        }

        impl Strategy for ::core::ops::RangeFrom<$typ> {
            type Tree = BinarySearch;
            type Value = $typ;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                Ok(BinarySearch::new_clamped(
                    self.start,
                    sample_uniform_value!(
                        $sample_mode,
                        $incl,
                        $sample_typ,
                        runner,
                        self.start,
                        <$typ>::MAX,
                    )
                    .into(),
                    <$typ>::MAX,
                ))
            }
        }

        impl Strategy for ::core::ops::RangeTo<$typ> {
            type Tree = BinarySearch;
            type Value = $typ;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                Ok(BinarySearch::new_clamped(
                    <$typ>::MIN,
                    sample_uniform_value!(
                        $sample_mode,
                        $uniform,
                        $sample_typ,
                        runner,
                        <$typ>::MIN,
                        self.end,
                    )
                    .into(),
                    self.end,
                ))
            }
        }

        impl Strategy for ::core::ops::RangeToInclusive<$typ> {
            type Tree = BinarySearch;
            type Value = $typ;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                Ok(BinarySearch::new_clamped(
                    <$typ>::MIN,
                    sample_uniform_value!(
                        $sample_mode,
                        $incl,
                        $sample_typ,
                        runner,
                        <$typ>::MIN,
                        self.end,
                    )
                    .into(),
                    self.end,
                ))
            }
        }
    };
}

/// Defines the complete strategy submodule for one signed integer type.
///
/// Emits the type's `pub mod` containing its `Any`/`ANY`, the toward-zero
/// `BinarySearch` value tree (whose shrinking tracks magnitude across the sign
/// boundary), and the `numeric_api!` range implementations.
macro_rules! signed_integer_bin_search {
    ($typ:ident) => {
        signed_integer_bin_search!(@with_mode
            generic,
            $typ,
            supported_int_any,
            sample_uniform,
            sample_uniform_incl
        );
    };
    ($typ:ident, $int_any: ident, $uniform: ident, $incl: ident) => {
        signed_integer_bin_search!(@with_mode plain, $typ, $int_any, $uniform, $incl);
    };
    (@with_mode
        $sample_mode:ident,
        $typ:ident,
        $int_any: ident,
        $uniform: ident,
        $incl: ident
    ) => {
        #[allow(missing_docs)]
        pub mod $typ {
            #[allow(unused_imports)]
            use rand::{Rng, RngExt};

            use crate::strategy::*;
            use crate::test_runner::TestRunner;

            int_any!($typ, $int_any);

            /// Shrinks an integer towards 0, using binary search to find
            /// boundary points.
            #[derive(Clone, Copy, Debug)]
            pub struct BinarySearch {
                lo: $typ,
                curr: $typ,
                hi: $typ,
            }
            impl BinarySearch {
                /// Creates a new binary searcher starting at the given value.
                #[allow(clippy::single_call_fn, reason = "seed the signed-integer binary-search shrinker at its initial generated value")]
                pub fn new(start: $typ) -> Self {
                    BinarySearch {
                        lo: 0,
                        curr: start,
                        hi: start,
                    }
                }

                /// Creates a new binary searcher which will not produce values
                /// on the other side of `lo` or `hi` from `start`. `lo` is
                /// inclusive, `hi` is exclusive.
                fn new_clamped(lo: $typ, start: $typ, hi: $typ) -> Self {
                    use core::cmp::{max, min};

                    BinarySearch {
                        lo: if start < 0 {
                            min(0, hi - 1)
                        } else {
                            max(0, lo)
                        },
                        hi: start,
                        curr: start,
                    }
                }

                fn reposition(&mut self) -> bool {
                    // Won't ever overflow since lo starts at 0 and advances
                    // towards hi.
                    let interval = self.hi - self.lo;
                    let new_mid = self.lo + interval / 2;

                    if new_mid == self.curr {
                        false
                    } else {
                        self.curr = new_mid;
                        true
                    }
                }

                fn magnitude_greater(lhs: $typ, rhs: $typ) -> bool {
                    if 0 == lhs {
                        false
                    } else if lhs < 0 {
                        lhs < rhs
                    } else {
                        lhs > rhs
                    }
                }
            }
            impl ValueTree for BinarySearch {
                type Value = $typ;

                fn current(&self) -> $typ {
                    self.curr
                }

                fn simplify(&mut self) -> bool {
                    if !BinarySearch::magnitude_greater(self.hi, self.lo) {
                        return false;
                    }

                    self.hi = self.curr;
                    self.reposition()
                }

                fn complicate(&mut self) -> bool {
                    if !BinarySearch::magnitude_greater(self.hi, self.lo) {
                        return false;
                    }

                    self.lo = self.curr + if self.hi < 0 { -1 } else { 1 };

                    self.reposition()
                }
            }

            numeric_api!(@with_mode $sample_mode, $typ, $typ, 1, $uniform, $incl);
        }
    };
}

/// Defines the complete strategy submodule for one unsigned integer type.
///
/// Like `signed_integer_bin_search!` but for unsigned types: the `BinarySearch`
/// shrinks toward `0` (or a clamped lower bound) and adds `new_above`.
macro_rules! unsigned_integer_bin_search {
    ($typ:ident) => {
        unsigned_integer_bin_search!(@with_mode
            generic,
            $typ,
            supported_int_any,
            sample_uniform,
            sample_uniform_incl
        );
    };
    ($typ:ident, $int_any: ident, $uniform: ident, $incl: ident) => {
        unsigned_integer_bin_search!(@with_mode plain, $typ, $int_any, $uniform, $incl);
    };
    (@with_mode
        $sample_mode:ident,
        $typ:ident,
        $int_any: ident,
        $uniform: ident,
        $incl: ident
    ) => {
        #[allow(missing_docs)]
        pub mod $typ {
            #[allow(unused_imports)]
            use rand::{Rng, RngExt};

            use crate::strategy::*;
            use crate::test_runner::TestRunner;

            int_any!($typ, $int_any);

            /// Shrinks an integer towards 0, using binary search to find
            /// boundary points.
            #[derive(Clone, Copy, Debug)]
            pub struct BinarySearch {
                lo: $typ,
                curr: $typ,
                hi: $typ,
            }
            impl BinarySearch {
                /// Creates a new binary searcher starting at the given value.
                #[allow(clippy::single_call_fn, reason = "seed the unsigned-integer binary-search shrinker at its initial generated value")]
                pub fn new(start: $typ) -> Self {
                    BinarySearch {
                        lo: 0,
                        curr: start,
                        hi: start,
                    }
                }

                /// Creates a new binary searcher which will not search below
                /// the given `lo` value.
                fn new_clamped(lo: $typ, start: $typ, _hi: $typ) -> Self {
                    BinarySearch {
                        lo,
                        curr: start,
                        hi: start,
                    }
                }

                /// Creates a new binary searcher which will not search below
                /// the given `lo` value.
                #[allow(clippy::single_call_fn, reason = "clamp the unsigned binary-search shrinker so it never searches below a floor value")]
                pub fn new_above(lo: $typ, start: $typ) -> Self {
                    BinarySearch::new_clamped(lo, start, start)
                }

                fn reposition(&mut self) -> bool {
                    let interval = self.hi - self.lo;
                    let new_mid = self.lo + interval / 2;

                    if new_mid == self.curr {
                        false
                    } else {
                        self.curr = new_mid;
                        true
                    }
                }
            }
            impl ValueTree for BinarySearch {
                type Value = $typ;

                fn current(&self) -> $typ {
                    self.curr
                }

                fn simplify(&mut self) -> bool {
                    if self.hi <= self.lo {
                        return false;
                    }

                    self.hi = self.curr;
                    self.reposition()
                }

                fn complicate(&mut self) -> bool {
                    if self.hi <= self.lo {
                        return false;
                    }

                    self.lo = self.curr + 1;
                    self.reposition()
                }
            }

            numeric_api!(@with_mode $sample_mode, $typ, $typ, 1, $uniform, $incl);
        }
    };
}

signed_integer_bin_search!(i8);
signed_integer_bin_search!(i16);
signed_integer_bin_search!(i32);
signed_integer_bin_search!(i64);
signed_integer_bin_search!(i128);
signed_integer_bin_search!(
    isize,
    unsupported_int_any,
    isize_sample_uniform,
    isize_sample_uniform_incl
);
unsigned_integer_bin_search!(u8);
unsigned_integer_bin_search!(u16);
unsigned_integer_bin_search!(u32);
unsigned_integer_bin_search!(u64);
unsigned_integer_bin_search!(u128);
unsigned_integer_bin_search!(
    usize,
    unsupported_int_any,
    usize_sample_uniform,
    usize_sample_uniform_incl
);

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub(crate) struct FloatTypes: u32 {
        const POSITIVE          = 0b0000_0001;
        const NEGATIVE          = 0b0000_0010;
        const NORMAL            = 0b0000_0100;
        const SUBNORMAL         = 0b0000_1000;
        const ZERO              = 0b0001_0000;
        const INFINITE          = 0b0010_0000;
        const QUIET_NAN         = 0b0100_0000;
        const SIGNALING_NAN     = 0b1000_0000;
        const ANY =
            Self::POSITIVE.bits() |
            Self::NEGATIVE.bits() |
            Self::NORMAL.bits() |
            Self::SUBNORMAL.bits() |
            Self::ZERO.bits() |
            Self::INFINITE.bits() |
            Self::QUIET_NAN.bits();
    }
}

impl FloatTypes {
    /// Fills in the implied classes an `Any` left unspecified.
    ///
    /// If no sign was requested, `POSITIVE` is added; if no value class was
    /// requested, `NORMAL` is added, matching the documented `Any` defaults.
    fn normalise(mut self) -> Self {
        if !self.intersects(FloatTypes::POSITIVE | FloatTypes::NEGATIVE) {
            self |= FloatTypes::POSITIVE;
        }

        if !self.intersects(
            FloatTypes::NORMAL
                | FloatTypes::SUBNORMAL
                | FloatTypes::ZERO
                | FloatTypes::INFINITE
                | FloatTypes::QUIET_NAN
                | FloatTypes::SIGNALING_NAN,
        ) {
            self |= FloatTypes::NORMAL;
        }
        self
    }
}

/// Describes the IEEE 754 bit layout of a float type for `Any` generation.
///
/// Exposes the integer `Bits` representation plus the masks isolating the sign,
/// exponent, and mantissa, letting the float strategies assemble a value of a
/// chosen class by manipulating raw bits.
trait FloatLayout
where
    StandardUniform: Distribution<Self::Bits>,
{
    /// Unsigned integer type holding this float's raw bit pattern.
    type Bits: Copy;

    /// Mask isolating the sign bit.
    const SIGN_MASK: Self::Bits;
    /// Mask isolating the exponent field.
    const EXP_MASK: Self::Bits;
    /// Exponent field of `1.0`, substituted when an edge exponent is
    /// disallowed.
    const EXP_ZERO: Self::Bits;
    /// Mask isolating the mantissa field.
    const MANTISSA_MASK: Self::Bits;
}

#[cfg(feature = "f16")]
impl FloatLayout for f16 {
    type Bits = u16;

    const SIGN_MASK: u16 = 0x8000;
    const EXP_MASK: u16 = 0x7c00;
    const EXP_ZERO: u16 = f16::to_bits(1.0);
    const MANTISSA_MASK: u16 =
        !(<Self as FloatLayout>::SIGN_MASK | Self::EXP_MASK);
}

impl FloatLayout for f32 {
    type Bits = u32;

    const SIGN_MASK: u32 = 0x8000_0000;
    const EXP_MASK: u32 = 0x7F80_0000;
    const EXP_ZERO: u32 = 0x3F80_0000;
    const MANTISSA_MASK: u32 = 0x007F_FFFF;
}

impl FloatLayout for f64 {
    type Bits = u64;

    const SIGN_MASK: u64 = 0x8000_0000_0000_0000;
    const EXP_MASK: u64 = 0x7FF0_0000_0000_0000;
    const EXP_ZERO: u64 = 0x3FF0_0000_0000_0000;
    const MANTISSA_MASK: u64 = 0x000F_FFFF_FFFF_FFFF;
}

/// Defines the float-class `Any` strategy surface for one float type.
///
/// Emits the `Any` type, the per-class constants (`POSITIVE`, `NORMAL`,
/// `INFINITE`, the NaN classes, `ANY`, …) that OR together, and the `Strategy`
/// impl that samples a value of the chosen classes by masking random bits.
macro_rules! float_any {
    ($typ:ident) => {
        /// Strategies which produce floating-point values from particular
        /// classes. See the various `Any`-typed constants in this module.
        ///
        /// Note that this usage is fairly advanced and primarily useful to
        /// implementors of algorithms that need to handle wild values in a
        /// particular way. For testing things like graphics processing or game
        /// physics, simply using ranges (e.g., `-1.0..2.0`) will often be more
        /// practical.
        ///
        /// `Any` can be OR'ed to combine multiple classes. For example,
        /// `POSITIVE | INFINITE` will generate arbitrary positive, non-NaN
        /// floats, including positive infinity (but not negative infinity, of
        /// course).
        ///
        /// If neither `POSITIVE` nor `NEGATIVE` has been OR'ed into an `Any`
        /// but a type to be generated requires a sign, `POSITIVE` is assumed.
        /// If no classes are OR'ed into an `Any` (i.e., only `POSITIVE` and/or
        /// `NEGATIVE` are given), `NORMAL` is assumed.
        ///
        /// The various float classes are assigned fixed weights for generation
        /// which are believed to be reasonable for most applications. Roughly:
        ///
        /// - If `POSITIVE | NEGATIVE`, the sign is evenly distributed between
        ///   both options.
        ///
        /// - Classes are weighted as follows, in descending order:
        ///   `NORMAL` > `ZERO` > `SUBNORMAL` > `INFINITE` > `QUIET_NAN` =
        ///   `SIGNALING_NAN`.
        #[derive(Clone, Copy, Debug)]
        #[must_use = "strategies do nothing unless used"]
        pub struct Any(FloatTypes);

        #[cfg(test)]
        impl Any {
            pub(crate) fn from_bits(bits: u32) -> Self {
                Any(FloatTypes::from_bits_truncate(bits))
            }

            pub(crate) fn normal_bits(&self) -> FloatTypes {
                self.0.normalise()
            }
        }

        impl ops::BitOr for Any {
            type Output = Self;

            fn bitor(self, rhs: Self) -> Self {
                Any(self.0 | rhs.0)
            }
        }

        impl ops::BitOrAssign for Any {
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0
            }
        }

        /// Generates positive floats
        ///
        /// By itself, implies the `NORMAL` class, unless another class is
        /// OR'ed in. That is, using `POSITIVE` as a strategy by itself will
        /// generate arbitrary values between the type's `MIN_POSITIVE` and
        /// `MAX`, while `POSITIVE | INFINITE` would only allow generating
        /// positive infinity.
        pub const POSITIVE: Any = Any(FloatTypes::POSITIVE);
        /// Generates negative floats.
        ///
        /// By itself, implies the `NORMAL` class, unless another class is
        /// OR'ed in. That is, using `POSITIVE` as a strategy by itself will
        /// generate arbitrary values between the type's `MIN` and
        /// `-MIN_POSITIVE`, while `NEGATIVE | INFINITE` would only allow
        /// generating positive infinity.
        pub const NEGATIVE: Any = Any(FloatTypes::NEGATIVE);
        /// Generates "normal" floats.
        ///
        /// These are finite values where the first bit of the mantissa is an
        /// implied `1`. When positive, this represents the range
        /// `MIN_POSITIVE` through `MAX`, both inclusive.
        ///
        /// Generated values are uniform over the discrete floating-point
        /// space, which means the numeric distribution is an inverse
        /// exponential step function. For example, values between 1.0 and 2.0
        /// are generated with the same frequency as values between 2.0 and
        /// 4.0, even though the latter covers twice the numeric range.
        ///
        /// If neither `POSITIVE` nor `NEGATIVE` is OR'ed with this constant,
        /// `POSITIVE` is implied.
        pub const NORMAL: Any = Any(FloatTypes::NORMAL);
        /// Generates subnormal floats.
        ///
        /// These are finite non-zero values where the first bit of the
        /// mantissa is not an implied zero. When positive, this represents the
        /// range `MIN`, inclusive, through `MIN_POSITIVE`, exclusive.
        ///
        /// Subnormals are generated with a uniform distribution both in terms
        /// of discrete floating-point space and numerically.
        ///
        /// If neither `POSITIVE` nor `NEGATIVE` is OR'ed with this constant,
        /// `POSITIVE` is implied.
        pub const SUBNORMAL: Any = Any(FloatTypes::SUBNORMAL);
        /// Generates zero-valued floats.
        ///
        /// Note that IEEE floats support both positive and negative zero, so
        /// this class does interact with the sign flags.
        ///
        /// If neither `POSITIVE` nor `NEGATIVE` is OR'ed with this constant,
        /// `POSITIVE` is implied.
        pub const ZERO: Any = Any(FloatTypes::ZERO);
        /// Generates infinity floats.
        ///
        /// If neither `POSITIVE` nor `NEGATIVE` is OR'ed with this constant,
        /// `POSITIVE` is implied.
        pub const INFINITE: Any = Any(FloatTypes::INFINITE);
        /// Generates "Quiet NaN" floats.
        ///
        /// Operations on quiet NaNs generally simply propagate the NaN rather
        /// than invoke any exception mechanism.
        ///
        /// The payload of the NaN is uniformly distributed over the possible
        /// values which safe Rust allows, including the sign bit (as
        /// controlled by `POSITIVE` and `NEGATIVE`).
        ///
        /// Note however that in Rust 1.23.0 and earlier, this constitutes only
        /// one particular payload due to apparent issues with particular MIPS
        /// and PA-RISC processors which fail to implement IEEE 754-2008
        /// correctly.
        ///
        /// On Rust 1.24.0 and later, this does produce arbitrary payloads as
        /// documented.
        ///
        /// On platforms where the CPU and the IEEE standard disagree on the
        /// format of a quiet NaN, values generated conform to the hardware's
        /// expectations.
        pub const QUIET_NAN: Any = Any(FloatTypes::QUIET_NAN);
        /// Generates "Signaling NaN" floats if allowed by the platform.
        ///
        /// On most platforms, signalling NaNs by default behave the same as
        /// quiet NaNs, but it is possible to configure the OS or CPU to raise
        /// an asynchronous exception if an operation is performed on a
        /// signalling NaN.
        ///
        /// In Rust 1.23.0 and earlier, this silently behaves the same as
        /// [`QUIET_NAN`](const.QUIET_NAN.html).
        ///
        /// On platforms where the CPU and the IEEE standard disagree on the
        /// format of a quiet NaN, values generated conform to the hardware's
        /// expectations.
        ///
        /// Note that certain platforms — most notably, x86/AMD64 — allow the
        /// architecture to turn a signalling NaN into a quiet NaN with the
        /// same payload. Whether this happens can depend on what registers the
        /// compiler decides to use to pass the value around, what CPU flags
        /// are set, and what compiler settings are in use.
        pub const SIGNALING_NAN: Any = Any(FloatTypes::SIGNALING_NAN);

        /// Generates literally arbitrary floating-point values, including
        /// infinities and quiet NaNs (but not signaling NaNs).
        ///
        /// Equivalent to `POSITIVE | NEGATIVE | NORMAL | SUBNORMAL | ZERO |
        /// INFINITE | QUIET_NAN`.
        ///
        /// See [`SIGNALING_NAN`](const.SIGNALING_NAN.html) if you also want to
        /// generate signalling NaNs. This signalling NaNs are not included by
        /// default since in most contexts they either make no difference, or
        /// if the process enabled the relevant CPU mode, result in
        /// hardware-triggered exceptions that usually just abort the process.
        ///
        /// Before proptest 0.4.1, this erroneously generated values in the
        /// range 0.0..1.0.
        pub const ANY: Any = Any(FloatTypes::ANY);

        impl Strategy for Any {
            type Tree = BinarySearch;
            type Value = $typ;

            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                let flags = self.0.normalise();
                let sign_mask = if flags.contains(FloatTypes::NEGATIVE) {
                    <$typ as FloatLayout>::SIGN_MASK
                } else {
                    0
                };
                let sign_or = if flags.contains(FloatTypes::POSITIVE) {
                    0
                } else {
                    <$typ as FloatLayout>::SIGN_MASK
                };

                macro_rules! weight {
                    ($case:ident, $weight:expr) => {
                        if flags.contains(FloatTypes::$case) {
                            $weight
                        } else {
                            0
                        }
                    }
                }

                // A few CPUs disagree with IEEE about the meaning of the
                // signalling bit. Assume the `NAN` constant is a quiet NaN as
                // interpreted by the hardware and generate values based on
                // that.
                let quiet_or = $typ::NAN.to_bits() &
                    ($typ::EXP_MASK | ($typ::EXP_MASK >> 1));
                let signaling_or = (quiet_or ^ ($typ::EXP_MASK >> 1)) |
                    $typ::EXP_MASK;

                let (class_mask, class_or, allow_edge_exp, allow_zero_mant) =
                    prop_oneof![
                        weight!(NORMAL, 20) => Just(
                            ($typ::EXP_MASK | <$typ as FloatLayout>::MANTISSA_MASK, 0,
                             false, true)),
                        weight!(SUBNORMAL, 3) => Just(
                            (<$typ as FloatLayout>::MANTISSA_MASK, 0, true, false)),
                        weight!(ZERO, 4) => Just(
                            (0, 0, true, true)),
                        weight!(INFINITE, 2) => Just(
                            (0, $typ::EXP_MASK, true, true)),
                        weight!(QUIET_NAN, 1) => Just(
                            (<$typ as FloatLayout>::MANTISSA_MASK >> 1, quiet_or,
                             true, false)),
                        weight!(SIGNALING_NAN, 1) => Just(
                            (<$typ as FloatLayout>::MANTISSA_MASK >> 1, signaling_or,
                             true, false)),
                    ].new_tree(runner)?.current();

                let mut generated_value: <$typ as FloatLayout>::Bits =
                    runner.rng().random();
                generated_value &= sign_mask | class_mask;
                generated_value |= sign_or | class_or;
                let exp = generated_value & $typ::EXP_MASK;
                if !allow_edge_exp && (0 == exp || $typ::EXP_MASK == exp) {
                    generated_value &= !$typ::EXP_MASK;
                    generated_value |= $typ::EXP_ZERO;
                }
                if !allow_zero_mant &&
                    0 == generated_value & <$typ as FloatLayout>::MANTISSA_MASK
                {
                    generated_value |= 1;
                }

                Ok(BinarySearch::new_with_types(
                    $typ::from_bits(generated_value), flags))
            }
        }
    }
}

/// Defines the complete strategy submodule for one float type.
///
/// Emits the type's `pub mod` containing its `float_any!` surface, the
/// toward-zero `BinarySearch` value tree (non-finite values shrink straight to
/// `0`, and shrinking stays within the originally allowed classes), and the
/// `numeric_api!` range implementations. `$sample_typ` names the custom
/// uniform sampler from `float_samplers`.
macro_rules! float_bin_search {
    ($typ:ident, $sample_typ:ident) => {
        #[allow(missing_docs)]
        pub mod $typ {
            use super::float_samplers::$sample_typ;

            use core::ops;
            use rand::RngExt;

            use super::{FloatLayout, FloatTypes};
            use crate::strategy::*;
            use crate::test_runner::TestRunner;

            float_any!($typ);

            /// Shrinks a float towards 0, using binary search to find boundary
            /// points.
            ///
            /// Non-finite values immediately shrink to 0.
            #[derive(Clone, Copy, Debug)]
            pub struct BinarySearch {
                lo: $typ,
                curr: $typ,
                hi: $typ,
                allowed: FloatTypes,
            }

            impl BinarySearch {
                /// Creates a new binary searcher starting at the given value.
                pub fn new(start: $typ) -> Self {
                    BinarySearch {
                        lo: 0.0,
                        curr: start,
                        hi: start,
                        allowed: FloatTypes::all(),
                    }
                }

                #[allow(clippy::single_call_fn, reason = "restrict a float BinarySearch shrinker to a caller-chosen subset of FloatTypes")]
                fn new_with_types(start: $typ, allowed: FloatTypes) -> Self {
                    BinarySearch {
                        lo: 0.0,
                        curr: start,
                        hi: start,
                        allowed,
                    }
                }

                /// Creates a new binary searcher which will not produce values
                /// on the other side of `lo` or `hi` from `start`. `lo` is
                /// inclusive, `hi` is exclusive.
                fn new_clamped(lo: $typ, start: $typ, hi: $typ) -> Self {
                    BinarySearch {
                        lo: if start.is_sign_negative() {
                            hi.min(0.0)
                        } else {
                            lo.max(0.0)
                        },
                        hi: start,
                        curr: start,
                        allowed: FloatTypes::all(),
                    }
                }

                fn current_allowed(&self) -> bool {
                    use core::num::FpCategory::*;

                    // Don't reposition if the new value is not allowed
                    let class_allowed = match self.curr.classify() {
                        Nan =>
                        // We don't need to inspect whether the
                        // signallingness of the NaN matches the allowed
                        // set, as we never try to switch between them,
                        // instead shrinking to 0.
                        {
                            self.allowed.contains(FloatTypes::QUIET_NAN)
                                || self
                                    .allowed
                                    .contains(FloatTypes::SIGNALING_NAN)
                        }
                        Infinite => self.allowed.contains(FloatTypes::INFINITE),
                        Zero => self.allowed.contains(FloatTypes::ZERO),
                        Subnormal => {
                            self.allowed.contains(FloatTypes::SUBNORMAL)
                        }
                        Normal => self.allowed.contains(FloatTypes::NORMAL),
                    };
                    let signum = self.curr.signum();
                    let sign_allowed = if signum > 0.0 {
                        self.allowed.contains(FloatTypes::POSITIVE)
                    } else if signum < 0.0 {
                        self.allowed.contains(FloatTypes::NEGATIVE)
                    } else {
                        true
                    };

                    class_allowed && sign_allowed
                }

                fn ensure_acceptable(&mut self) {
                    while !self.current_allowed() {
                        if !self.complicate_once() {
                            panic!(
                                "Unable to complicate floating-point back \
                                 to acceptable value"
                            );
                        }
                    }
                }

                fn reposition(&mut self) -> bool {
                    let interval = self.hi - self.lo;
                    let interval =
                        if interval.is_finite() { interval } else { 0.0 };
                    let new_mid = self.lo + interval / 2.0;

                    let new_mid = if new_mid == self.curr || 0.0 == interval {
                        new_mid
                    } else {
                        self.lo
                    };

                    if new_mid == self.curr {
                        false
                    } else {
                        self.curr = new_mid;
                        true
                    }
                }

                fn done(lo: $typ, hi: $typ) -> bool {
                    (lo.abs() > hi.abs() && !hi.is_nan()) || lo.is_nan()
                }

                fn complicate_once(&mut self) -> bool {
                    if BinarySearch::done(self.lo, self.hi) {
                        return false;
                    }

                    self.lo = if self.curr == self.lo {
                        self.hi
                    } else {
                        self.curr
                    };

                    self.reposition()
                }
            }
            impl ValueTree for BinarySearch {
                type Value = $typ;

                fn current(&self) -> $typ {
                    self.curr
                }

                fn simplify(&mut self) -> bool {
                    if BinarySearch::done(self.lo, self.hi) {
                        return false;
                    }

                    self.hi = self.curr;
                    if self.reposition() {
                        self.ensure_acceptable();
                        true
                    } else {
                        false
                    }
                }

                fn complicate(&mut self) -> bool {
                    if self.complicate_once() {
                        self.ensure_acceptable();
                        true
                    } else {
                        false
                    }
                }
            }

            numeric_api!(
                @with_mode
                generic,
                $typ,
                $sample_typ,
                0.0,
                sample_uniform,
                sample_uniform_incl
            );
        }
    };
}

#[cfg(feature = "f16")]
float_bin_search!(f16, F16U);
float_bin_search!(f32, F32U);
float_bin_search!(f64, F64U);

#[cfg(test)]
mod test {
    use strict_test_support::{TestFailure, ensure, ensure_eq, ensure_some};

    use crate::strategy::*;
    use crate::test_runner::*;

    use super::*;

    #[test]
    fn u8_inclusive_end_included() -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();
        let mut ok = 0;
        for _ in 0..20 {
            let tree = ensure_some(
                (0..=1i32).new_tree(&mut runner).ok(),
                "inclusive range generates a value tree",
            )?;
            let test = runner.run_one(tree, |candidate| {
                if candidate == 1 {
                    Ok(())
                } else {
                    Err(TestCaseError::fail("not the inclusive end"))
                }
            });
            if test.is_ok() {
                ok += 1;
            }
        }
        ensure(ok > 1, "the inclusive end is included")
    }

    #[test]
    fn u8_inclusive_to_end_included() -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();
        let mut ok = 0;
        for _ in 0..20 {
            let tree = ensure_some(
                (..=1u8).new_tree(&mut runner).ok(),
                "inclusive-to range generates a value tree",
            )?;
            let test = runner.run_one(tree, |candidate| {
                if candidate == 1 {
                    Ok(())
                } else {
                    Err(TestCaseError::fail("not the inclusive end"))
                }
            });
            if test.is_ok() {
                ok += 1;
            }
        }
        ensure(ok > 1, "the inclusive end is included")
    }

    #[test]
    fn i8_binary_search_always_converges() -> Result<(), TestFailure> {
        fn ensure_converges<P: Fn(i32) -> bool>(
            start: i8,
            pass: P,
        ) -> Result<(), TestFailure> {
            let mut state = i8::BinarySearch::new(start);
            loop {
                if !pass(state.current() as i32) {
                    if !state.simplify() {
                        break;
                    }
                } else {
                    if !state.complicate() {
                        break;
                    }
                }
            }

            ensure(
                !pass(state.current() as i32),
                "the converged value still fails",
            )?;
            ensure(
                pass(state.current() as i32 - 1)
                    || pass(state.current() as i32 + 1),
                "a neighbour of the converged value passes",
            )
        }

        for start in -128..0 {
            for target in start + 1..1 {
                ensure_converges(start as i8, |probe| probe > target)?;
            }
        }

        for start in 0..128 {
            for target in 0..start {
                ensure_converges(start as i8, |probe| probe < target)?;
            }
        }
        Ok(())
    }

    #[test]
    fn u8_binary_search_always_converges() -> Result<(), TestFailure> {
        fn ensure_converges<P: Fn(u32) -> bool>(
            start: u8,
            pass: P,
        ) -> Result<(), TestFailure> {
            let mut state = u8::BinarySearch::new(start);
            loop {
                if !pass(state.current() as u32) {
                    if !state.simplify() {
                        break;
                    }
                } else {
                    if !state.complicate() {
                        break;
                    }
                }
            }

            ensure(
                !pass(state.current() as u32),
                "the converged value still fails",
            )?;
            ensure(
                pass(state.current() as u32 - 1),
                "the predecessor of the converged value passes",
            )
        }

        for start in 0..255 {
            for target in 0..start {
                ensure_converges(start as u8, |probe| probe <= target)?;
            }
        }
        Ok(())
    }

    #[test]
    fn signed_integer_range_including_zero_converges_to_zero()
    -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        for _ in 0..100 {
            let mut state = ensure_some(
                (-42i32..64i32).new_tree(&mut runner).ok(),
                "signed range generates a value tree",
            )?;
            let init_value = state.current();
            ensure(
                (-42..64).contains(&init_value),
                "the initial value is in bounds",
            )?;

            while state.simplify() {
                let simplified = state.current();
                ensure(
                    (-42..64).contains(&simplified),
                    "every simplified value stays in bounds",
                )?;
            }

            ensure_eq(
                &0,
                &state.current(),
                "a range containing zero converges to zero",
            )?;
        }
        Ok(())
    }

    #[test]
    fn negative_integer_range_stays_in_bounds() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        for _ in 0..100 {
            let mut state = ensure_some(
                (..-42i32).new_tree(&mut runner).ok(),
                "negative range generates a value tree",
            )?;
            let init_value = state.current();
            ensure(init_value < -42, "the initial value is in bounds")?;

            while state.simplify() {
                ensure(
                    state.current() < -42,
                    "every simplified value stays in bounds",
                )?;
            }

            ensure_eq(
                &-43,
                &state.current(),
                "the range converges to its upper bound",
            )?;
        }
        Ok(())
    }

    #[test]
    fn positive_signed_integer_range_stays_in_bounds() -> Result<(), TestFailure>
    {
        let mut runner = TestRunner::default();
        for _ in 0..100 {
            let mut state = ensure_some(
                (42i32..).new_tree(&mut runner).ok(),
                "positive range generates a value tree",
            )?;
            let init_value = state.current();
            ensure(init_value >= 42, "the initial value is in bounds")?;

            while state.simplify() {
                ensure(
                    state.current() >= 42,
                    "every simplified value stays in bounds",
                )?;
            }

            ensure_eq(
                &42,
                &state.current(),
                "the range converges to its lower bound",
            )?;
        }
        Ok(())
    }

    #[test]
    fn unsigned_integer_range_stays_in_bounds() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        for _ in 0..100 {
            let mut state = ensure_some(
                (42u32..56u32).new_tree(&mut runner).ok(),
                "unsigned range generates a value tree",
            )?;
            let init_value = state.current();
            ensure(
                (42..56).contains(&init_value),
                "the initial value is in bounds",
            )?;

            while state.simplify() {
                ensure(
                    state.current() >= 42,
                    "every simplified value stays in bounds",
                )?;
            }

            ensure_eq(
                &42,
                &state.current(),
                "the range converges to its lower bound",
            )?;
        }
        Ok(())
    }

    mod contract_sanity {
        macro_rules! contract_sanity {
            ($t:tt, $forty_two:expr, $fifty_six:expr) => {
                mod $t {
                    use crate::strategy::check_strategy_sanity;

                    const FORTY_TWO: $t = $forty_two;
                    const FIFTY_SIX: $t = $fifty_six;

                    #[test]
                    fn range() {
                        check_strategy_sanity(FORTY_TWO..FIFTY_SIX, None);
                    }

                    #[test]
                    fn range_inclusive() {
                        check_strategy_sanity(FORTY_TWO..=FIFTY_SIX, None);
                    }

                    #[test]
                    fn range_to() {
                        check_strategy_sanity(..FIFTY_SIX, None);
                    }

                    #[test]
                    fn range_to_inclusive() {
                        check_strategy_sanity(..=FIFTY_SIX, None);
                    }

                    #[test]
                    fn range_from() {
                        check_strategy_sanity(FORTY_TWO.., None);
                    }
                }
            };
        }
        contract_sanity!(u8, 42, 56);
        contract_sanity!(i8, 42, 56);
        contract_sanity!(u16, 42, 56);
        contract_sanity!(i16, 42, 56);
        contract_sanity!(u32, 42, 56);
        contract_sanity!(i32, 42, 56);
        contract_sanity!(u64, 42, 56);
        contract_sanity!(i64, 42, 56);
        contract_sanity!(usize, 42, 56);
        contract_sanity!(isize, 42, 56);
        #[cfg(feature = "f16")]
        contract_sanity!(f16, 42.0, 56.0);
        contract_sanity!(f32, 42.0, 56.0);
        contract_sanity!(f64, 42.0, 56.0);
    }

    #[test]
    fn unsigned_integer_binsearch_simplify_complicate_contract_upheld() {
        check_strategy_sanity(0u32..1000u32, None);
        check_strategy_sanity(0u32..1u32, None);
    }

    #[test]
    fn signed_integer_binsearch_simplify_complicate_contract_upheld() {
        check_strategy_sanity(0i32..1000i32, None);
        check_strategy_sanity(0i32..1i32, None);
    }

    #[test]
    fn positive_float_simplifies_to_zero() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        let mut value = ensure_some(
            (0.0f64..2.0).new_tree(&mut runner).ok(),
            "float range generates a value tree",
        )?;

        while value.simplify() {}

        ensure_eq(&0.0, &value.current(), "the range shrinks to zero")
    }

    #[test]
    fn positive_float_simplifies_to_base() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        let mut value = ensure_some(
            (1.0f64..2.0).new_tree(&mut runner).ok(),
            "float range generates a value tree",
        )?;

        while value.simplify() {}

        ensure_eq(&1.0, &value.current(), "the range shrinks to its base")
    }

    #[test]
    fn negative_float_simplifies_to_zero() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        let mut value = ensure_some(
            (-2.0f64..0.0).new_tree(&mut runner).ok(),
            "float range generates a value tree",
        )?;

        while value.simplify() {}

        ensure_eq(&0.0, &value.current(), "the range shrinks to zero")
    }

    #[test]
    fn positive_float_complicates_to_original() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        let mut value = ensure_some(
            (1.0f64..2.0).new_tree(&mut runner).ok(),
            "float range generates a value tree",
        )?;
        let orig = value.current();

        ensure(value.simplify(), "the tree simplifies once")?;
        while value.complicate() {}

        ensure_eq(
            &orig,
            &value.current(),
            "complicating restores the original",
        )
    }

    #[test]
    fn positive_infinity_simplifies_directly_to_zero() -> Result<(), TestFailure>
    {
        let mut value = f64::BinarySearch::new(f64::INFINITY);

        ensure(value.simplify(), "infinity simplifies once")?;
        ensure_eq(&0.0, &value.current(), "infinity simplifies to zero")?;
        ensure(value.complicate(), "the zero complicates back")?;
        ensure_eq(
            &f64::INFINITY,
            &value.current(),
            "complicating restores infinity",
        )?;
        ensure(
            !value.clone().complicate(),
            "the restored tree cannot complicate",
        )?;
        ensure(
            !value.clone().simplify(),
            "the restored tree cannot simplify",
        )
    }

    #[test]
    fn negative_infinity_simplifies_directly_to_zero() -> Result<(), TestFailure>
    {
        let mut value = f64::BinarySearch::new(f64::NEG_INFINITY);

        ensure(value.simplify(), "negative infinity simplifies once")?;
        ensure_eq(&0.0, &value.current(), "it simplifies to zero")?;
        ensure(value.complicate(), "the zero complicates back")?;
        ensure_eq(
            &f64::NEG_INFINITY,
            &value.current(),
            "complicating restores negative infinity",
        )?;
        ensure(
            !value.clone().complicate(),
            "the restored tree cannot complicate",
        )?;
        ensure(
            !value.clone().simplify(),
            "the restored tree cannot simplify",
        )
    }

    #[test]
    fn nan_simplifies_directly_to_zero() -> Result<(), TestFailure> {
        let mut value = f64::BinarySearch::new(f64::NAN);

        ensure(value.simplify(), "NaN simplifies once")?;
        ensure_eq(&0.0, &value.current(), "NaN simplifies to zero")?;
        ensure(value.complicate(), "the zero complicates back")?;
        ensure(value.current().is_nan(), "complicating restores NaN")?;
        ensure(
            !value.clone().complicate(),
            "the restored tree cannot complicate",
        )?;
        ensure(
            !value.clone().simplify(),
            "the restored tree cannot simplify",
        )
    }

    #[test]
    fn float_simplifies_to_smallest_normal() -> Result<(), TestFailure> {
        let mut runner = TestRunner::default();
        let mut value = ensure_some(
            (f64::MIN_POSITIVE..2.0).new_tree(&mut runner).ok(),
            "float range generates a value tree",
        )?;

        while value.simplify() {}

        ensure_eq(
            &f64::MIN_POSITIVE,
            &value.current(),
            "the range shrinks to the smallest normal",
        )
    }

    /// The shared body of the per-type `*_any_generates_desired_values`
    /// properties: it evaluates to `Result<(), TestFailure>` so the strict
    /// property closures can return it directly.
    macro_rules! float_generation_test_body {
        ($strategy:ident, $typ:ident) => {{
            use std::num::FpCategory;

            let strategy = $strategy;
            let bits = strategy.normal_bits();

            let mut seen_positive = 0;
            let mut seen_negative = 0;
            let mut seen_normal = 0;
            let mut seen_subnormal = 0;
            let mut seen_zero = 0;
            let mut seen_infinite = 0;
            let mut seen_quiet_nan = 0;
            let mut seen_signaling_nan = 0;
            let mut runner = TestRunner::deterministic();

            // Check whether this version of Rust honours the NaN payload in
            // from_bits
            let fidelity_1 = f32::from_bits(0x7F80_0001).to_bits();
            let fidelity_2 = f32::from_bits(0xFF80_0001).to_bits();
            let nan_fidelity = fidelity_1 != fidelity_2;

            for _ in 0..1024 {
                let mut tree = ensure_some(
                    strategy.new_tree(&mut runner).ok(),
                    "float class strategy generates a value tree",
                )?;
                let mut increment = 1;

                loop {
                    let value = tree.current();

                    let sign = value.signum(); // So we correctly handle -0
                    if sign < 0.0 {
                        ensure(
                            bits.contains(FloatTypes::NEGATIVE),
                            "a negative value implies the NEGATIVE class",
                        )?;
                        seen_negative += increment;
                    } else if sign > 0.0 {
                        // i.e., not NaN
                        ensure(
                            bits.contains(FloatTypes::POSITIVE),
                            "a positive value implies the POSITIVE class",
                        )?;
                        seen_positive += increment;
                    }

                    match value.classify() {
                        FpCategory::Nan if nan_fidelity => {
                            let raw = value.to_bits();
                            let is_negative = raw << 1 >> 1 != raw;
                            if is_negative {
                                ensure(
                                    bits.contains(FloatTypes::NEGATIVE),
                                    "a negative NaN implies the NEGATIVE \
                                     class",
                                )?;
                                seen_negative += increment;
                            } else {
                                ensure(
                                    bits.contains(FloatTypes::POSITIVE),
                                    "a positive NaN implies the POSITIVE \
                                     class",
                                )?;
                                seen_positive += increment;
                            }

                            let is_quiet = raw & ($typ::EXP_MASK >> 1)
                                == $typ::NAN.to_bits() & ($typ::EXP_MASK >> 1);
                            if is_quiet {
                                // x86/AMD64 turn signalling NaNs into quiet
                                // NaNs quite aggressively depending on what
                                // registers LLVM decides to use to pass the
                                // value around, so accept either case here.
                                ensure(
                                    bits.contains(FloatTypes::QUIET_NAN)
                                        || bits.contains(
                                            FloatTypes::SIGNALING_NAN,
                                        ),
                                    "a quiet NaN implies a NaN class",
                                )?;
                                seen_quiet_nan += increment;
                                seen_signaling_nan += increment;
                            } else {
                                ensure(
                                    bits.contains(FloatTypes::SIGNALING_NAN),
                                    "a signaling NaN implies the \
                                     SIGNALING_NAN class",
                                )?;
                                seen_signaling_nan += increment;
                            }
                        }

                        FpCategory::Nan => {
                            // Since safe Rust doesn't currently allow
                            // generating any NaN other than one particular
                            // payload, don't check the sign or signallingness
                            // and consider this to be both signs and
                            // signallingness for counting purposes.
                            seen_positive += increment;
                            seen_negative += increment;
                            seen_quiet_nan += increment;
                            seen_signaling_nan += increment;
                            ensure(
                                bits.contains(FloatTypes::QUIET_NAN)
                                    || bits.contains(FloatTypes::SIGNALING_NAN),
                                "a NaN implies a NaN class",
                            )?;
                        }
                        FpCategory::Infinite => {
                            ensure(
                                bits.contains(FloatTypes::INFINITE),
                                "an infinity implies the INFINITE class",
                            )?;
                            seen_infinite += increment;
                        }
                        FpCategory::Zero => {
                            ensure(
                                bits.contains(FloatTypes::ZERO),
                                "a zero implies the ZERO class",
                            )?;
                            seen_zero += increment;
                        }
                        FpCategory::Subnormal => {
                            ensure(
                                bits.contains(FloatTypes::SUBNORMAL),
                                "a subnormal implies the SUBNORMAL class",
                            )?;
                            seen_subnormal += increment;
                        }
                        FpCategory::Normal => {
                            ensure(
                                bits.contains(FloatTypes::NORMAL),
                                "a normal value implies the NORMAL class",
                            )?;
                            seen_normal += increment;
                        }
                    }

                    // Don't count simplified values towards the counts
                    increment = 0;
                    if !tree.simplify() {
                        break;
                    }
                }
            }

            if bits.contains(FloatTypes::POSITIVE) {
                ensure(
                    seen_positive > 200,
                    "the POSITIVE class is well represented",
                )?;
            }
            if bits.contains(FloatTypes::NEGATIVE) {
                ensure(
                    seen_negative > 200,
                    "the NEGATIVE class is well represented",
                )?;
            }
            if bits.contains(FloatTypes::NORMAL) {
                ensure(
                    seen_normal > 100,
                    "the NORMAL class is well represented",
                )?;
            }
            if bits.contains(FloatTypes::SUBNORMAL) {
                ensure(
                    seen_subnormal > 5,
                    "the SUBNORMAL class is represented",
                )?;
            }
            if bits.contains(FloatTypes::ZERO) {
                ensure(seen_zero > 5, "the ZERO class is represented")?;
            }
            if bits.contains(FloatTypes::INFINITE) {
                ensure(seen_infinite > 0, "the INFINITE class is represented")?;
            }
            if bits.contains(FloatTypes::QUIET_NAN) {
                ensure(
                    seen_quiet_nan > 0,
                    "the QUIET_NAN class is represented",
                )?;
            }
            if bits.contains(FloatTypes::SIGNALING_NAN) {
                ensure(
                    seen_signaling_nan > 0,
                    "the SIGNALING_NAN class is represented",
                )?;
            }
            Ok(())
        }};
    }

    /// Run one strict float-class property with the 1024-case config the
    /// legacy `proptest!` block used.
    fn run_float_class_property<S, F>(
        strategy: &S,
        context: &'static str,
        property: F,
    ) -> Result<(), TestFailure>
    where
        S: Strategy,
        F: Fn(S::Value) -> Result<(), TestFailure>,
    {
        crate::strict::ensure_property_with_config(
            strategy,
            context,
            Config {
                failure_persistence: None,
                ..Config::with_cases(1024)
            },
            property,
        )
    }

    #[cfg(feature = "f16")]
    #[test]
    fn f16_any_generates_desired_values() -> Result<(), TestFailure> {
        run_float_class_property(
            &crate::bits::u32::ANY.prop_map(f16::Any::from_bits),
            "every f16 class combination generates matching values",
            |strategy| float_generation_test_body!(strategy, f16),
        )
    }

    #[cfg(feature = "f16")]
    #[test]
    fn f16_any_sanity() -> Result<(), TestFailure> {
        run_float_class_property(
            &crate::bits::u32::ANY.prop_map(f16::Any::from_bits),
            "every f16 class combination upholds the shrink contract",
            |strategy| {
                check_strategy_sanity(
                    strategy,
                    Some(CheckStrategySanityOptions {
                        strict_complicate_after_simplify: false,
                        ..CheckStrategySanityOptions::default()
                    }),
                );
                Ok(())
            },
        )
    }

    #[test]
    fn f32_any_generates_desired_values() -> Result<(), TestFailure> {
        run_float_class_property(
            &crate::bits::u32::ANY.prop_map(f32::Any::from_bits),
            "every f32 class combination generates matching values",
            |strategy| float_generation_test_body!(strategy, f32),
        )
    }

    #[test]
    fn f32_any_sanity() -> Result<(), TestFailure> {
        run_float_class_property(
            &crate::bits::u32::ANY.prop_map(f32::Any::from_bits),
            "every f32 class combination upholds the shrink contract",
            |strategy| {
                check_strategy_sanity(
                    strategy,
                    Some(CheckStrategySanityOptions {
                        strict_complicate_after_simplify: false,
                        ..CheckStrategySanityOptions::default()
                    }),
                );
                Ok(())
            },
        )
    }

    #[test]
    fn f64_any_generates_desired_values() -> Result<(), TestFailure> {
        run_float_class_property(
            &crate::bits::u32::ANY.prop_map(f64::Any::from_bits),
            "every f64 class combination generates matching values",
            |strategy| float_generation_test_body!(strategy, f64),
        )
    }

    #[test]
    fn f64_any_sanity() -> Result<(), TestFailure> {
        run_float_class_property(
            &crate::bits::u32::ANY.prop_map(f64::Any::from_bits),
            "every f64 class combination upholds the shrink contract",
            |strategy| {
                check_strategy_sanity(
                    strategy,
                    Some(CheckStrategySanityOptions {
                        strict_complicate_after_simplify: false,
                        ..CheckStrategySanityOptions::default()
                    }),
                );
                Ok(())
            },
        )
    }

    mod panic_on_empty {
        // These tests pin the *documented* panic contract of empty numeric
        // ranges, so the panic is the behavior under test: it is observed
        // through `catch_unwind` while the assertions themselves use the
        // strict vocabulary.
        macro_rules! panic_on_empty {
            ($t:tt, $zero:expr, $one:expr) => {
                mod $t {
                    use crate::strategy::Strategy;
                    use crate::test_runner::TestRunner;
                    use std::panic;
                    use std::string::String;
                    use strict_test_support::{TestFailure, ensure};

                    const ZERO: $t = $zero;
                    const ONE: $t = $one;

                    #[test]
                    fn range() -> Result<(), TestFailure> {
                        ensure(
                            panic::catch_unwind(|| {
                                let mut runner = TestRunner::deterministic();
                                drop((ZERO..ZERO).new_tree(&mut runner));
                            })
                            .err()
                            .and_then(|payload| {
                                payload.downcast_ref::<String>().map(|message| {
                                    message == "Invalid use of empty range 0..0."
                                })
                            }) == Some(true),
                            "an empty range panics with the documented \
                             message",
                        )
                    }

                    #[test]
                    fn range_inclusive() -> Result<(), TestFailure> {
                        ensure(
                            panic::catch_unwind(|| {
                                let mut runner = TestRunner::deterministic();
                                drop(
                                    core::ops::RangeInclusive::new(ONE, ZERO)
                                        .new_tree(&mut runner),
                                );
                            })
                            .err()
                            .and_then(|payload| {
                                payload.downcast_ref::<String>().map(|message| {
                                    message == "Invalid use of empty range 1..=0."
                                })
                            }) == Some(true),
                            "an empty inclusive range panics with the \
                             documented message",
                        )
                    }
                }
            };
        }
        panic_on_empty!(u8, 0, 1);
        panic_on_empty!(i8, 0, 1);
        panic_on_empty!(u16, 0, 1);
        panic_on_empty!(i16, 0, 1);
        panic_on_empty!(u32, 0, 1);
        panic_on_empty!(i32, 0, 1);
        panic_on_empty!(u64, 0, 1);
        panic_on_empty!(i64, 0, 1);
        panic_on_empty!(usize, 0, 1);
        panic_on_empty!(isize, 0, 1);
        #[cfg(feature = "f16")]
        panic_on_empty!(f16, 0.0, 1.0);
        panic_on_empty!(f32, 0.0, 1.0);
        panic_on_empty!(f64, 0.0, 1.0);
    }
}
