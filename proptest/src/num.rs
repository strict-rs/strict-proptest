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

use core::error::Error;
use core::fmt;

use bitflags::bitflags;
use rand::distr::Distribution;
use rand::distr::StandardUniform;
use rand::distr::uniform::SampleUniform;
use rand::distr::uniform::Uniform;

use crate::test_runner::Reason;
use crate::test_runner::TestRunner;

/// Convert an exclusive range end into the inclusive bound used by the
/// shrinker for this numeric type.
trait NumericRangeEndpoint: Copy {
  /// Return the inclusive upper bound represented by `end` and `epsilon`.
  fn inclusive_end_from_exclusive(end: Self, epsilon: Self) -> Self;
}

/// Implements [`NumericRangeEndpoint`] for integer types whose exclusive upper
/// bound maps to the previous representable value.
macro_rules! numeric_range_endpoint {
    ($($typ:ty),* $(,)?) => {
        $(
            impl NumericRangeEndpoint for $typ {
                fn inclusive_end_from_exclusive(
                    end: Self,
                    epsilon: Self,
                ) -> Self {
                    end.saturating_sub(epsilon)
                }
            }
        )*
    };
}

numeric_range_endpoint!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize,);

/// Which uniform range constructor rejected its bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniformRangeKind {
  /// Half-open uniform range `[start, end)`.
  HalfOpen,
  /// Inclusive uniform range `[start, end]`.
  Inclusive,
}

/// Error returned when a uniform range cannot be sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformRangeError {
  /// The constructor shape that rejected its bounds.
  kind: UniformRangeKind,
}

impl UniformRangeError {
  /// Error for a rejected half-open range.
  const fn half_open() -> Self {
    Self {
      kind: UniformRangeKind::HalfOpen,
    }
  }

  /// Error for a rejected inclusive range.
  const fn inclusive() -> Self {
    Self {
      kind: UniformRangeKind::Inclusive,
    }
  }
}

impl fmt::Display for UniformRangeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.kind {
      UniformRangeKind::HalfOpen => f.write_str("invalid half-open uniform range"),
      UniformRangeKind::Inclusive => f.write_str("invalid inclusive uniform range"),
    }
  }
}

impl Error for UniformRangeError {}

impl From<UniformRangeError> for Reason {
  fn from(error: UniformRangeError) -> Self {
    match error.kind {
      UniformRangeKind::HalfOpen => "invalid half-open uniform range".into(),
      UniformRangeKind::Inclusive => "invalid inclusive uniform range".into(),
    }
  }
}

#[cfg(all(feature = "f16", not(feature = "alt-stable")))]
impl NumericRangeEndpoint for f16 {
  fn inclusive_end_from_exclusive(end: Self, _epsilon: Self) -> Self {
    end
  }
}

#[cfg(feature = "alt-stable")]
impl NumericRangeEndpoint for half::f16 {
  fn inclusive_end_from_exclusive(end: Self, _epsilon: Self) -> Self {
    end
  }
}

impl NumericRangeEndpoint for f32 {
  fn inclusive_end_from_exclusive(end: Self, _epsilon: Self) -> Self {
    end
  }
}

impl NumericRangeEndpoint for f64 {
  fn inclusive_end_from_exclusive(end: Self, _epsilon: Self) -> Self {
    end
  }
}

/// Generate a random value of `X`, sampled uniformly from the half
/// open range `[low, high)` (excluding `high`).
pub(crate) fn sample_uniform<X: SampleUniform>(run: &mut TestRunner, start: X, end: X) -> Result<X, UniformRangeError> {
  Uniform::new(start, end)
    .map_err(|_error| UniformRangeError::half_open())
    .map(|uniform| uniform.sample(run.rng()))
}

/// Generate a random value of `X`, sampled uniformly from the closed
/// range `[low, high]` (inclusive).
///
/// # Errors
///
/// Returns [`UniformRangeError`] if the range is empty and has no value to
/// draw.
pub fn sample_uniform_incl<X: SampleUniform>(run: &mut TestRunner, start: X, end: X) -> Result<X, UniformRangeError> {
  Uniform::new_inclusive(start, end)
    .map_err(|_error| UniformRangeError::inclusive())
    .map(|uniform| uniform.sample(run.rng()))
}

/// Defines a pair of uniform samplers that go through a wider integer type.
///
/// For a target type `$to` sampled via `$from` (e.g. `usize` via `u64`), emits
/// `$name` (half-open `[start, end)`) and `$incl` (closed `[start, end]`),
/// which cast through `$from` before sampling to reach types `rand` cannot
/// sample directly.
macro_rules! sample_uniform {
  ($name:ident, $incl:ident, $from:ty, $to:ty) => {
    sample_uniform!(@functions $from, $to;
      $name, new, half_open;
      $incl, new_inclusive, inclusive;
    );
  };
  (@functions $from:ty, $to:ty; $($name:ident, $constructor:ident, $error:ident;)+) => {
    $(fn $name(run: &mut TestRunner, start: $to, end: $to) -> Result<$to, UniformRangeError> {
      let start = match <$from>::try_from(start) {
        Ok(value) => value,
        Err(_error) => <$from>::MAX,
      };
      let end = match <$from>::try_from(end) {
        Ok(value) => value,
        Err(_error) => <$from>::MAX,
      };
      let sample = Uniform::<$from>::$constructor(start, end)
        .map_err(|_error| UniformRangeError::$error())?
        .sample(run.rng());
      Ok(match <$to>::try_from(sample) {
        Ok(value) => value,
        Err(_error) => <$to>::MAX,
      })
    })+
  };
}

/// Dispatches to a uniform sampler in either `generic` or `plain` mode.
///
/// In `generic` mode the bounds are `.into()`-converted to the sample type
/// (used by the primitive ranges); in `plain` mode they are forwarded as-is
/// (used where the value type already is the sample type).
macro_rules! sample_uniform_value {
  (generic, $uniform:ident, $sample_typ:ty, $runner:expr, $start:expr, $end:expr $(,)?) => {
    $crate::num::$uniform::<$sample_typ>($runner, $start.into(), $end.into())
  };
  (plain, $uniform:ident, $sample_typ:ty, $runner:expr, $start:expr, $end:expr $(,)?) => {
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
    <$typ>::from_ne_bytes($runner.rng().next_u64().to_ne_bytes())
  };
}

/// Draws a fully arbitrary integer for a type the RNG cannot sample directly.
///
/// Falls back to a raw `next_u32` word cast to the target type, used for
/// `usize`/`isize` on 32-bit targets.
#[cfg(target_pointer_width = "32")]
macro_rules! unsupported_int_any {
  ($runner:ident, $typ:ty) => {
    <$typ>::from_ne_bytes($runner.rng().next_u32().to_ne_bytes())
  };
}

/// Draws a fully arbitrary integer for a type the RNG cannot sample directly.
///
/// Falls back to the low two bytes of a raw `next_u32` word, used for
/// `usize`/`isize` on 16-bit targets.
#[cfg(target_pointer_width = "16")]
macro_rules! unsupported_int_any {
  ($runner:ident, $typ:ty) => {
    <$typ>::from_ne_bytes({
      let bytes = $runner.rng().next_u32().to_ne_bytes();
      [bytes[0], bytes[1]]
    })
  };
}

/// Defines the `Any` strategy type and its `ANY` constant for one integer
/// type.
///
/// The generated `Any` produces completely arbitrary values (via `$int_any`)
/// and shrinks them through the module's `BinarySearch` toward `0`.
macro_rules! int_any {
  ($typ:ident, $int_any:ident) => {
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

/// Imports the RNG trait required by one generated integer module.
macro_rules! integer_rng_import {
  (generic) => {
    use rand::RngExt;
  };
  (plain) => {
    use rand::Rng;
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
  (@with_mode $sample_mode:ident, $typ:ident, $sample_typ:ty, $epsilon:expr, $uniform:ident, $incl:ident) => {
    impl Strategy for ::core::ops::Range<$typ> {
      type Tree = BinarySearch;
      type Value = $typ;

      fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        if self.is_empty() {
          return Err("Invalid use of empty range.".into());
        }

        Ok(BinarySearch::new_clamped(
          self.start,
          sample_uniform_value!($sample_mode, $uniform, $sample_typ, runner, self.start, self.end,)?.into(),
          <$typ as super::NumericRangeEndpoint>::inclusive_end_from_exclusive(self.end, $epsilon),
        ))
      }
    }

    impl Strategy for ::core::ops::RangeInclusive<$typ> {
      type Tree = BinarySearch;
      type Value = $typ;

      fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        if self.is_empty() {
          return Err("Invalid use of empty inclusive range.".into());
        }

        Ok(BinarySearch::new_clamped(
          *self.start(),
          sample_uniform_value!($sample_mode, $incl, $sample_typ, runner, *self.start(), *self.end(),)?.into(),
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
          sample_uniform_value!($sample_mode, $incl, $sample_typ, runner, self.start, <$typ>::MAX,)?.into(),
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
          sample_uniform_value!($sample_mode, $uniform, $sample_typ, runner, <$typ>::MIN, self.end,)?.into(),
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
          sample_uniform_value!($sample_mode, $incl, $sample_typ, runner, <$typ>::MIN, self.end,)?.into(),
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
        #[doc = concat!(
            "Strategies and shrinkers for `",
            stringify!($typ),
            "` values."
        )]
        pub mod $typ {
            integer_rng_import!($sample_mode);

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
                pub const fn new(start: $typ) -> Self {
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
                            min(0, hi.saturating_sub(1))
                        } else {
                            max(0, lo)
                        },
                        hi: start,
                        curr: start,
                    }
                }

                const fn reposition(&mut self) -> bool {
                    // Won't ever overflow since lo starts at 0 and advances
                    // towards hi.
                    let interval = self.hi.wrapping_sub(self.lo);
                    let new_mid =
                        self.lo.wrapping_add(interval.wrapping_div(2));

                    if new_mid == self.curr {
                        false
                    } else {
                        self.curr = new_mid;
                        true
                    }
                }

                const fn magnitude_greater(lhs: $typ, rhs: $typ) -> bool {
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

                    self.lo = if self.hi < 0 {
                        self.curr.saturating_sub(1)
                    } else {
                        self.curr.saturating_add(1)
                    };

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
        #[doc = concat!(
            "Strategies and shrinkers for `",
            stringify!($typ),
            "` values."
        )]
        pub mod $typ {
            integer_rng_import!($sample_mode);

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
                pub const fn new(start: $typ) -> Self {
                    BinarySearch {
                        lo: 0,
                        curr: start,
                        hi: start,
                    }
                }

                /// Creates a new binary searcher which will not search below
                /// the given `lo` value.
                const fn new_clamped(lo: $typ, start: $typ, _hi: $typ) -> Self {
                    BinarySearch {
                        lo,
                        curr: start,
                        hi: start,
                    }
                }

                /// Creates a new binary searcher which will not search below
                /// the given `lo` value.
                #[allow(clippy::single_call_fn, reason = "clamp the unsigned binary-search shrinker so it never searches below a floor value")]
                pub const fn new_above(lo: $typ, start: $typ) -> Self {
                    BinarySearch::new_clamped(lo, start, start)
                }

                const fn reposition(&mut self) -> bool {
                    let interval = self.hi.saturating_sub(self.lo);
                    let new_mid =
                        self.lo.saturating_add(interval.div_euclid(2));

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

                    self.lo = self.curr.saturating_add(1);
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
signed_integer_bin_search!(isize, unsupported_int_any, isize_sample_uniform, isize_sample_uniform_incl);
unsigned_integer_bin_search!(u8);
unsigned_integer_bin_search!(u16);
unsigned_integer_bin_search!(u32);
unsigned_integer_bin_search!(u64);
unsigned_integer_bin_search!(u128);
unsigned_integer_bin_search!(usize, unsupported_int_any, usize_sample_uniform, usize_sample_uniform_incl);

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
    if !self.intersects(Self::POSITIVE | Self::NEGATIVE) {
      self |= Self::POSITIVE;
    }

    if !self.intersects(Self::NORMAL | Self::SUBNORMAL | Self::ZERO | Self::INFINITE | Self::QUIET_NAN | Self::SIGNALING_NAN) {
      self |= Self::NORMAL;
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

#[cfg(all(feature = "f16", not(feature = "alt-stable")))]
impl FloatLayout for f16 {
  type Bits = u16;

  const SIGN_MASK: u16 = 0x8000;
  const EXP_MASK: u16 = 0x7c00;
  const EXP_ZERO: u16 = Self::to_bits(1.0);
  const MANTISSA_MASK: u16 = !(<Self as FloatLayout>::SIGN_MASK | Self::EXP_MASK);
}

#[cfg(feature = "alt-stable")]
impl FloatLayout for half::f16 {
  type Bits = u16;

  const SIGN_MASK: u16 = 0x8000;
  const EXP_MASK: u16 = 0x7c00;
  const EXP_ZERO: u16 = Self::ONE.to_bits();
  const MANTISSA_MASK: u16 = !(<Self as FloatLayout>::SIGN_MASK | Self::EXP_MASK);
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
                    (<$typ as FloatLayout>::EXP_MASK | (<$typ as FloatLayout>::EXP_MASK >> 1));
                let signaling_or = (quiet_or ^ (<$typ as FloatLayout>::EXP_MASK >> 1)) |
                    <$typ as FloatLayout>::EXP_MASK;

                let (class_mask, class_or, allow_edge_exp, allow_zero_mant) =
                    prop_oneof![
                        weight!(NORMAL, 20) => Just(
                            (<$typ as FloatLayout>::EXP_MASK | <$typ as FloatLayout>::MANTISSA_MASK, 0,
                             false, true)),
                        weight!(SUBNORMAL, 3) => Just(
                            (<$typ as FloatLayout>::MANTISSA_MASK, 0, true, false)),
                        weight!(ZERO, 4) => Just(
                            (0, 0, true, true)),
                        weight!(INFINITE, 2) => Just(
                            (0, <$typ as FloatLayout>::EXP_MASK, true, true)),
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
                let exp = generated_value & <$typ as FloatLayout>::EXP_MASK;
                if !allow_edge_exp && (0 == exp || <$typ as FloatLayout>::EXP_MASK == exp) {
                    generated_value &= !<$typ as FloatLayout>::EXP_MASK;
                    generated_value |= <$typ as FloatLayout>::EXP_ZERO;
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
    (
        module = $module: ident,
        type = $typ: ident
        $(, type_path = $type_path:path)?,
        sample = $sample_typ: ident,
        zero = $zero: expr,
        two = $two: expr
        $(, always_trait = $always_trait:path)?
        $(, clamped_fn = $clamped_fn:tt)?
    ) => {
        #[doc = concat!(
            "Strategies and shrinkers for `",
            stringify!($module),
            "` values."
        )]
        pub mod $module {
            use super::float_samplers::$sample_typ;

            use core::ops;
            use rand::RngExt;

            use super::{FloatLayout, FloatTypes};
            use crate::strategy::*;
            use crate::test_runner::TestRunner;
            $(
                use $type_path as $typ;
            )?
            $(
                use $always_trait as _;
            )?

            const FLOAT_ZERO: $typ = $zero;
            const FLOAT_TWO: $typ = $two;

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

            fn float_equal(left: $typ, right: $typ) -> bool {
                matches!(
                    left.partial_cmp(&right),
                    Some(core::cmp::Ordering::Equal)
                )
            }

            impl BinarySearch {
                /// Creates a new binary searcher starting at the given value.
                pub const fn new(start: $typ) -> Self {
                    BinarySearch {
                        lo: FLOAT_ZERO,
                        curr: start,
                        hi: start,
                        allowed: FloatTypes::all(),
                    }
                }

                #[allow(clippy::single_call_fn, reason = "restrict a float BinarySearch shrinker to a caller-chosen subset of FloatTypes")]
                const fn new_with_types(start: $typ, allowed: FloatTypes) -> Self {
                    BinarySearch {
                        lo: FLOAT_ZERO,
                        curr: start,
                        hi: start,
                        allowed,
                    }
                }

                /// Creates a new binary searcher which will not produce values
                /// on the other side of `lo` or `hi` from `start`. `lo` is
                /// inclusive, `hi` is exclusive.
                $($clamped_fn)? fn new_clamped(lo: $typ, start: $typ, hi: $typ) -> Self {
                    BinarySearch {
                        lo: if start.is_sign_negative() {
                            hi.min(FLOAT_ZERO)
                        } else {
                            lo.max(FLOAT_ZERO)
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
                    let sign_allowed = if signum > FLOAT_ZERO {
                        self.allowed.contains(FloatTypes::POSITIVE)
                    } else if signum < FLOAT_ZERO {
                        self.allowed.contains(FloatTypes::NEGATIVE)
                    } else {
                        true
                    };

                    class_allowed && sign_allowed
                }

                fn ensure_acceptable(&mut self) -> bool {
                    while !self.current_allowed() {
                        if !self.complicate_once() {
                            return false;
                        }
                    }
                    true
                }

                fn reposition(&mut self) -> bool {
                    let interval = core::ops::Sub::sub(self.hi, self.lo);
                    let interval =
                        if interval.is_finite() { interval } else { FLOAT_ZERO };
                    let new_mid = core::ops::Add::add(
                        self.lo,
                        core::ops::Div::div(interval, FLOAT_TWO),
                    );

                    let midpoint_converged = float_equal(new_mid, self.curr)
                        || matches!(
                            interval.classify(),
                            core::num::FpCategory::Zero
                        );
                    let new_mid = if midpoint_converged {
                        new_mid
                    } else {
                        self.lo
                    };

                    if float_equal(new_mid, self.curr) {
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

                    self.lo = if float_equal(self.curr, self.lo) {
                        self.hi
                    } else {
                        self.curr
                    };

                    self.reposition()
                }

                /// Keep an allowed step or restore the preceding candidate
                /// when no permitted value remains reachable from it.
                fn accept_step(&mut self, previous: Self, changed: bool) -> bool {
                    if !changed {
                        return false;
                    }
                    if self.ensure_acceptable() {
                        true
                    } else {
                        *self = previous;
                        false
                    }
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

                    let previous = *self;
                    self.hi = self.curr;
                    let changed = self.reposition();
                    self.accept_step(previous, changed)
                }

                fn complicate(&mut self) -> bool {
                    let previous = *self;
                    let changed = self.complicate_once();
                    self.accept_step(previous, changed)
                }
            }

            numeric_api!(
                @with_mode
                generic,
                $typ,
                $sample_typ,
                FLOAT_ZERO,
                sample_uniform,
                sample_uniform_incl
            );
        }
    };
}

#[cfg(all(feature = "f16", not(feature = "alt-stable")))]
float_bin_search!(
    module = f16,
    type = f16,
    sample = F16U,
    zero = 0.0,
    two = 2.0
);
#[cfg(feature = "alt-stable")]
float_bin_search!(
    module = half_f16,
    type = HalfF16,
    type_path = half::f16,
    sample = HalfF16U,
    zero = HalfF16::ZERO,
    two = HalfF16::from_f32_const(2.0),
    always_trait = num_traits::float::Float
);
float_bin_search!(
    module = f32,
    type = f32,
    sample = F32U,
    zero = 0.0,
    two = 2.0,
    clamped_fn = const
);
float_bin_search!(
    module = f64,
    type = f64,
    sample = F64U,
    zero = 0.0,
    two = 2.0,
    clamped_fn = const
);

#[cfg(test)]
mod test {
  use core::cmp::Ordering;
  use core::ops::Range;
  use core::ops::RangeBounds;
  use core::ops::RangeFrom;
  use core::ops::RangeTo;

  #[cfg(feature = "strict-test")]
  use crate::strict::ensure_property_with_config;
  #[cfg(feature = "strict-test")]
  use crate::strict::strict_default_config;
  /// Assertions retain their native subject without a deeply nested return signature.
  type Check<S> = Result<(), PredicateFailure<S>>;
  /// Helpers preserve the complete subject on both success and failure.
  type Checked<S> = Result<S, PredicateFailure<S>>;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use super::*;
  use crate::bits::u32 as bits_u32;
  use crate::std_facade::Vec;
  use crate::strategy::*;
  use crate::test_runner::*;

  /// A numeric value tree and every value visited while simplifying it.
  #[derive(Debug)]
  struct ShrinkWalk<T: ValueTree> {
    /// Tree at the observed stopping point.
    tree:   T,
    /// Initial and subsequent native candidates.
    values: Vec<T::Value>,
  }
  /// Generated trees and errors remain paired with their original range strategy.
  type RangeObservation<S> = (S, Vec<Result<ShrinkWalk<<S as Strategy>::Tree>, Reason>>);

  /// Draw and simplify the same hundred cases used by the numeric range contracts.
  fn range_walks<S: Strategy>(strategy: S) -> RangeObservation<S> {
    let mut runner = test_runner_without_persistence();
    let observations = (0..100)
      .map(|_| strategy.new_tree(&mut runner))
      .map(|generated| {
        generated.map(|initial_tree| {
          let (tree, values) = trace_shrink_steps(initial_tree);
          ShrinkWalk {
            tree,
            values,
          }
        })
      })
      .collect();
    (strategy, observations)
  }

  /// Native candidate and legacy one-case outcome for inclusion tests.
  type Inclusion<T> = Vec<Result<(T, Result<bool, TestError<T>>), Reason>>;
  /// Check endpoint inclusion through the public one-case runner while retaining every candidate
  /// and run.
  fn check_inclusive_end<S: Strategy>(strategy: S, end: &S::Value, context: &'static str) -> Checked<Inclusion<S::Value>>
  where
    S::Value: PartialEq,
  {
    let mut runner = TestRunner::deterministic();
    let property = |candidate: S::Value| {
      if candidate == *end {
        Ok(())
      } else {
        Err(TestCaseError::fail("not the inclusive end"))
      }
    };
    let samples = (0..20)
      .map(|_| {
        strategy.new_tree(&mut runner).map(|tree| {
          let initial = tree.current();
          let result = runner.run_one(tree, property);
          (initial, result)
        })
      })
      .collect();
    ensure_that(samples, context, |observed: &Inclusion<S::Value>| {
      observed.iter().all(Result::is_ok) && observed.iter().filter(|result| matches!(result, Ok((_, Ok(_))))).count() > 1
    })
  }

  #[test]
  fn u8_inclusive_end_included() -> Check<Inclusion<i32>> {
    check_inclusive_end(0..=1_i32, &1, "the inclusive endpoint is generated more than once").map(drop)
  }
  #[test]
  fn u8_inclusive_to_end_included() -> Check<Inclusion<u8>> {
    check_inclusive_end(..=1_u8, &1, "the inclusive-to endpoint is generated more than once").map(drop)
  }

  /// Complete convergence trace and observed adjacent-point classifications.
  #[derive(Debug)]
  struct Convergence<T> {
    /// Initial candidate.
    start:      T,
    /// Boundary the test expects the search to find.
    target:     i32,
    /// Native candidates and the property's decision at each one.
    visited:    Vec<(T, bool)>,
    /// Neighbour decisions, made in i32 to cover the integer type's endpoints.
    neighbours: (bool, bool),
  }
  /// Drive a real numeric value tree with a boundary predicate.
  fn convergence<V: ValueTree>(mut tree: V, target: i32, pass: impl Fn(i32) -> bool) -> Convergence<V::Value>
  where
    V::Value: Copy + Into<i32>,
  {
    let start = tree.current();
    let mut visited = Vec::new();
    loop {
      let value = tree.current();
      let passed = pass(value.into());
      visited.push((value, passed));
      if !(if passed { tree.complicate() } else { tree.simplify() }) {
        break;
      }
    }
    let current: i32 = tree.current().into();
    Convergence {
      start,
      target,
      visited,
      neighbours: (current.checked_sub(1).is_some_and(&pass), current.checked_add(1).is_some_and(pass)),
    }
  }

  #[test]
  fn i8_binary_search_always_converges() -> Check<Vec<Convergence<i8>>> {
    let mut observations = Vec::new();
    for start in <i8>::MIN..0 {
      for target in i32::from(start).saturating_add(1)..1 {
        observations.push(convergence(i8::BinarySearch::new(start), target, |probe| probe > target));
      }
    }
    for start in 0..=<i8>::MAX {
      for target in 0..i32::from(start) {
        observations.push(convergence(i8::BinarySearch::new(start), target, |probe| probe < target));
      }
    }
    ensure_that(
      observations,
      "every signed search ends at a failing point adjacent to a passing point",
      |runs| {
        runs.iter().all(|run| {
          run.visited.first().is_some_and(|visited| visited.0 == run.start)
            && run
              .visited
              .last()
              .is_some_and(|visited| !visited.1 && i32::from(visited.0) == run.target)
            && (run.neighbours.0 || run.neighbours.1)
        })
      },
    )
    .map(drop)
  }
  #[test]
  fn u8_binary_search_always_converges() -> Check<Vec<Convergence<u8>>> {
    let mut observations = Vec::new();
    for start in 0..<u8>::MAX {
      for target in 0..i32::from(start) {
        observations.push(convergence(u8::BinarySearch::new(start), target, |probe| probe <= target));
      }
    }
    ensure_that(
      observations,
      "every unsigned search ends immediately above the passing boundary",
      |runs| {
        runs.iter().all(|run| {
          run.visited.first().is_some_and(|visited| visited.0 == run.start)
            && run
              .visited
              .last()
              .is_some_and(|visited| !visited.1 && i32::from(visited.0) == run.target.saturating_add(1))
            && run.neighbours.0
        })
      },
    )
    .map(drop)
  }

  /// Check every integer shrink candidate against its source range and target.
  fn check_integer_range<S>(strategy: S, target: &S::Value, context: &'static str) -> Checked<RangeObservation<S>>
  where
    S: Strategy + RangeBounds<S::Value>,
    S::Value: Ord,
  {
    ensure_that(range_walks(strategy), context, |observed| {
      observed.1.iter().all(|result| {
        result
          .as_ref()
          .is_ok_and(|walk| walk.values.iter().all(|value| observed.0.contains(value)) && walk.tree.current() == *target)
      })
    })
  }

  #[test]
  fn signed_integer_range_including_zero_converges_to_zero() -> Check<RangeObservation<Range<i32>>> {
    check_integer_range(-42_i32..64, &0, "every candidate stays in range and every tree converges to zero").map(drop)
  }
  #[test]
  fn negative_integer_range_stays_in_bounds() -> Check<RangeObservation<RangeTo<i32>>> {
    check_integer_range(
      ..-42_i32,
      &-43,
      "every negative candidate stays in range and converges to the upper bound",
    )
    .map(drop)
  }
  #[test]
  fn positive_signed_integer_range_stays_in_bounds() -> Check<RangeObservation<RangeFrom<i32>>> {
    check_integer_range(42_i32.., &42, "every positive candidate stays in range and converges to its base").map(drop)
  }
  #[test]
  fn unsigned_integer_range_stays_in_bounds() -> Check<RangeObservation<Range<u32>>> {
    check_integer_range(42_u32..56, &42, "every unsigned candidate stays in range and converges to its base").map(drop)
  }

  mod contract_sanity {
    macro_rules! contract_sanity {
      ($t:ident, $value:ty, $forty_two:expr, $fifty_six:expr) => {
        mod $t {
          use crate::strategy::check_strategy_sanity;
          use crate::test_runner::Reason;

          const FORTY_TWO: $value = $forty_two;
          const FIFTY_SIX: $value = $fifty_six;

          #[test]
          fn range() -> Result<(), Reason> {
            check_strategy_sanity(FORTY_TWO..FIFTY_SIX, None)
          }

          #[test]
          fn range_inclusive() -> Result<(), Reason> {
            check_strategy_sanity(FORTY_TWO..=FIFTY_SIX, None)
          }

          #[test]
          fn range_to() -> Result<(), Reason> {
            check_strategy_sanity(..FIFTY_SIX, None)
          }

          #[test]
          fn range_to_inclusive() -> Result<(), Reason> {
            check_strategy_sanity(..=FIFTY_SIX, None)
          }

          #[test]
          fn range_from() -> Result<(), Reason> {
            check_strategy_sanity(FORTY_TWO.., None)
          }
        }
      };
    }
    contract_sanity!(u8, u8, 42, 56);
    contract_sanity!(i8, i8, 42, 56);
    contract_sanity!(u16, u16, 42, 56);
    contract_sanity!(i16, i16, 42, 56);
    contract_sanity!(u32, u32, 42, 56);
    contract_sanity!(i32, i32, 42, 56);
    contract_sanity!(u64, u64, 42, 56);
    contract_sanity!(i64, i64, 42, 56);
    contract_sanity!(usize, usize, 42, 56);
    contract_sanity!(isize, isize, 42, 56);
    #[cfg(all(feature = "f16", not(feature = "alt-stable")))]
    contract_sanity!(f16, f16, 42.0, 56.0);
    contract_sanity!(f32, f32, 42.0, 56.0);
    contract_sanity!(f64, f64, 42.0, 56.0);

    #[cfg(feature = "alt-stable")]
    contract_sanity!(
      half_f16,
      half::f16,
      half::f16::from_f32_const(42.0),
      half::f16::from_f32_const(56.0)
    );
  }

  #[test]
  fn unsigned_integer_binsearch_simplify_complicate_contract_upheld() -> Result<(), Reason> {
    check_strategy_sanity(0_u32..1000_u32, None)?;
    check_strategy_sanity(0_u32..1_u32, None)
  }

  #[test]
  fn signed_integer_binsearch_simplify_complicate_contract_upheld() -> Result<(), Reason> {
    check_strategy_sanity(0_i32..1000_i32, None)?;
    check_strategy_sanity(0_i32..1_i32, None)
  }

  /// Native float range, reached tree, and visited values.
  type FloatRangeObservation = RangeObservation<Range<f64>>;

  macro_rules! float_range_test {
    ($name:ident, $start:expr, $end:expr, $expected:expr) => {
      #[test]
      fn $name() -> Check<FloatRangeObservation> {
        ensure_that(
          range_walks($start..$end),
          "float ranges generate in bounds and converge to their base",
          |observed| {
            observed.1.iter().all(|result| {
              result.as_ref().is_ok_and(|walk| {
                walk.values.first().is_some_and(|value| observed.0.contains(value))
                  && matches!(walk.tree.current().partial_cmp(&$expected), Some(Ordering::Equal))
              })
            })
          },
        )
        .map(drop)
      }
    };
  }
  float_range_test!(positive_float_simplifies_to_zero, 0.0_f64, 2.0, 0.0);
  float_range_test!(positive_float_simplifies_to_base, 1.0_f64, 2.0, 1.0);
  float_range_test!(negative_float_simplifies_to_zero, -2.0_f64, 0.0, 0.0);
  float_range_test!(float_simplifies_to_smallest_normal, <f64>::MIN_POSITIVE, 2.0, <f64>::MIN_POSITIVE);

  /// Float candidates and every simplify/complicate return value.
  #[derive(Debug)]
  struct FloatBacktrack {
    /// Original native value.
    original:      f64,
    /// Whether the first simplification changed the tree.
    simplified:    bool,
    /// Value after that simplification.
    simpler:       f64,
    /// Values produced by complication.
    complications: Vec<f64>,
    /// Whether a copy of the converged tree can still complicate or simplify.
    further:       (bool, bool),
  }
  /// Observe the complete numeric transition contract without boolean projection.
  fn backtrack_float(mut tree: f64::BinarySearch) -> FloatBacktrack {
    let original = tree.current();
    let simplified = tree.simplify();
    let simpler = tree.current();
    let mut complications = Vec::new();
    while tree.complicate() {
      complications.push(tree.current());
    }
    let mut complicating = tree;
    let further = (complicating.complicate(), tree.simplify());
    FloatBacktrack {
      original,
      simplified,
      simpler,
      complications,
      further,
    }
  }

  #[test]
  fn positive_float_complicates_to_original() -> Check<Result<FloatBacktrack, Reason>> {
    let mut runner = test_runner_without_persistence();
    ensure_that(
      (1.0_f64..2.0).new_tree(&mut runner).map(backtrack_float),
      "complicating restores the original float",
      |result| {
        result.as_ref().is_ok_and(|run| {
          run.simplified
            && run
              .complications
              .last()
              .is_some_and(|value| matches!(value.partial_cmp(&run.original), Some(Ordering::Equal)))
        })
      },
    )
    .map(drop)
  }
  macro_rules! nonfinite_backtrack_test {
    ($name:ident, $original:expr) => {
      #[test]
      fn $name() -> Check<FloatBacktrack> {
        ensure_that(
          backtrack_float(f64::BinarySearch::new($original)),
          "nonfinite values simplify to zero and complicate once to the original",
          |run| {
            run.simplified
              && matches!(run.simpler.classify(), core::num::FpCategory::Zero)
              && run.complications.len() == 1
              && run.complications.first().is_some_and(|value| {
                (run.original.is_nan() && value.is_nan()) || matches!(value.partial_cmp(&run.original), Some(Ordering::Equal))
              })
              && run.further == (false, false)
          },
        )
        .map(drop)
      }
    };
  }
  nonfinite_backtrack_test!(positive_infinity_simplifies_directly_to_zero, <f64>::INFINITY);
  nonfinite_backtrack_test!(negative_infinity_simplifies_directly_to_zero, <f64>::NEG_INFINITY);
  nonfinite_backtrack_test!(nan_simplifies_directly_to_zero, <f64>::NAN);

  /// Native samples, requested classes, and observed generation frequencies.
  #[derive(Debug)]
  struct FloatSamples<V> {
    /// Normalized strategy class flags.
    classes: FloatTypes,
    /// Generated and simplified native values in visitation order.
    values:  Vec<V>,
    /// Initial-generation frequencies for sign and IEEE value classes.
    counts:  [u32; 8],
    /// Native generation errors, rather than erased missing value trees.
    errors:  Vec<Reason>,
  }

  /// Collect and check each float-class combination without discarding its subjects.
  macro_rules! float_generation_test_body {
    ($strategy:ident, $value_typ:ty, $zero:expr) => {{
      use core::num::FpCategory;
      let classes = $strategy.normal_bits();
      let mut observations = FloatSamples {
        classes,
        values: Vec::new(),
        counts: [0_u32; 8],
        errors: Vec::new(),
      };
      let mut runner = TestRunner::deterministic();
      let nan_fidelity = <f32>::from_bits(0x7F80_0001).to_bits() != <f32>::from_bits(0xFF80_0001).to_bits();
      let required_classes = |value: $value_typ| {
        let zero: $value_typ = $zero;
        let sign = if value.signum() < zero {
          FloatTypes::NEGATIVE
        } else if value.signum() > zero {
          FloatTypes::POSITIVE
        } else {
          FloatTypes::empty()
        };
        match value.classify() {
          FpCategory::Nan if nan_fidelity => {
            let raw = value.to_bits();
            let sign = if raw << 1 >> 1 != raw {
              FloatTypes::NEGATIVE
            } else {
              FloatTypes::POSITIVE
            };
            let quiet = raw & (<$value_typ as FloatLayout>::EXP_MASK >> 1)
              == <$value_typ>::NAN.to_bits() & (<$value_typ as FloatLayout>::EXP_MASK >> 1);
            (
              sign,
              if quiet {
                FloatTypes::QUIET_NAN | FloatTypes::SIGNALING_NAN
              } else {
                FloatTypes::SIGNALING_NAN
              },
            )
          }
          FpCategory::Nan => (
            FloatTypes::POSITIVE | FloatTypes::NEGATIVE,
            FloatTypes::QUIET_NAN | FloatTypes::SIGNALING_NAN,
          ),
          FpCategory::Infinite => (sign, FloatTypes::INFINITE),
          FpCategory::Zero => (sign, FloatTypes::ZERO),
          FpCategory::Subnormal => (sign, FloatTypes::SUBNORMAL),
          FpCategory::Normal => (sign, FloatTypes::NORMAL),
        }
      };
      let flags = [
        FloatTypes::POSITIVE,
        FloatTypes::NEGATIVE,
        FloatTypes::NORMAL,
        FloatTypes::SUBNORMAL,
        FloatTypes::ZERO,
        FloatTypes::INFINITE,
        FloatTypes::QUIET_NAN,
        FloatTypes::SIGNALING_NAN,
      ];
      for _ in 0..1024 {
        let mut tree = match $strategy.new_tree(&mut runner) {
          Ok(tree) => tree,
          Err(error) => {
            observations.errors.push(error);
            break;
          }
        };
        let initial = tree.current();
        let (sign, category) = required_classes(initial);
        for (count, flag) in observations.counts.iter_mut().zip(flags) {
          if (sign | category).contains(flag) {
            *count = count.saturating_add(1);
          }
        }
        observations.values.push(initial);
        while tree.simplify() {
          observations.values.push(tree.current());
        }
      }
      ensure_that(
        observations,
        "generated and shrunk floats obey requested classes and generation frequencies",
        |observations| {
          observations.errors.is_empty()
            && observations.values.iter().all(|value| {
              let (sign, category) = required_classes(*value);
              observations.classes.intersects(sign) && observations.classes.intersects(category)
            })
            && observations
              .counts
              .iter()
              .zip(flags)
              .zip([200, 200, 100, 5, 5, 0, 0, 0])
              .all(|((count, flag), minimum)| !observations.classes.contains(flag) || *count > minimum)
        },
      )
    }};
  }

  /// Drive float-class properties with their native success and failure types.
  fn run_float_class_property<S: Strategy, A, E>(
    strategy: &S,
    context: &'static str,
    property: impl Fn(S::Value) -> Result<A, E>,
  ) -> PropertyResult<S::Value, A, E> {
    #[cfg(feature = "strict-test")]
    {
      ensure_property_with_config(
        strategy,
        context,
        Config {
          cases: 1024,
          ..strict_default_config()
        },
        property,
      )
    }
    #[cfg(not(feature = "strict-test"))]
    {
      let mut result = TestRunner::new(Config {
        cases: 1024,
        failure_persistence: None,
        ..Config::default()
      })
      .run_typed(strategy, property);
      match result {
        Ok(ref mut run) => run.context = context,
        Err(ref mut failure) => {
          failure.context = context;
          failure.run.context = context;
        }
      }
      result
    }
  }

  macro_rules! float_class_tests {
    ($generation:ident, $sanity:ident, $module:ident, $value:ty, $zero:expr) => {
      #[test]
      fn $generation() -> PropertyResult<$module::Any, FloatSamples<$value>, PredicateFailure<FloatSamples<$value>>> {
        run_float_class_property(
          &bits_u32::ANY.prop_map($module::Any::from_bits),
          "every float class combination generates matching values",
          |strategy| float_generation_test_body!(strategy, $value, $zero),
        )
      }
      #[test]
      fn $sanity() -> PropertyResult<$module::Any, Result<(), Reason>, PredicateFailure<Result<(), Reason>>> {
        run_float_class_property(
          &bits_u32::ANY.prop_map($module::Any::from_bits),
          "every float class combination upholds the shrink contract",
          |strategy| {
            ensure_that(
              check_strategy_sanity(
                strategy,
                Some(CheckStrategySanityOptions {
                  strict_complicate_after_simplify: false,
                  ..CheckStrategySanityOptions::default()
                }),
              ),
              "float class strategy upholds the shrink contract",
              Result::is_ok,
            )
          },
        )
      }
    };
  }
  #[cfg(all(feature = "f16", not(feature = "alt-stable")))]
  float_class_tests!(f16_any_generates_desired_values, f16_any_sanity, f16, f16, 0.0);
  #[cfg(feature = "alt-stable")]
  float_class_tests!(
    half_f16_any_generates_desired_values,
    half_f16_any_sanity,
    half_f16,
    half::f16,
    half::f16::ZERO
  );
  float_class_tests!(f32_any_generates_desired_values, f32_any_sanity, f32, f32, 0.0);
  float_class_tests!(f64_any_generates_desired_values, f64_any_sanity, f64, f64, 0.0);

  mod error_on_empty {
    use crate::test_runner::Reason;

    /// The half-open empty range has a stable generation diagnostic.
    fn empty_range_error<T>(observed: &Result<T, Reason>) -> bool {
      observed
        .as_ref()
        .is_err_and(|reason| reason.message() == "Invalid use of empty range.")
    }

    /// The inclusive empty range has its own stable generation diagnostic.
    fn empty_inclusive_range_error<T>(observed: &Result<T, Reason>) -> bool {
      observed
        .as_ref()
        .is_err_and(|reason| reason.message() == "Invalid use of empty inclusive range.")
    }

    // These tests pin the strict generation-error contract of empty
    // numeric ranges.
    macro_rules! error_on_empty {
      ($t:ident, $value:ty, $zero:expr, $one:expr) => {
        mod $t {
          use strict_test_support::PredicateFailure;
          use strict_test_support::ensure_that;

          use super::empty_inclusive_range_error;
          use super::empty_range_error;
          type Outcome = Result<crate::num::$t::BinarySearch, crate::test_runner::Reason>;

          use crate::strategy::Strategy;
          use crate::test_runner::TestRunner;

          const ZERO: $value = $zero;
          const ONE: $value = $one;

          #[test]
          fn range() -> Result<(), PredicateFailure<Outcome>> {
            let mut runner = TestRunner::deterministic();
            let result = (ZERO..ZERO).new_tree(&mut runner);
            ensure_that(result, "an empty range returns a generation error", empty_range_error).map(drop)
          }

          #[test]
          fn range_inclusive() -> Result<(), PredicateFailure<Outcome>> {
            let mut runner = TestRunner::deterministic();
            let result = core::ops::RangeInclusive::new(ONE, ZERO).new_tree(&mut runner);
            ensure_that(
              result,
              "an empty inclusive range returns a generation error",
              empty_inclusive_range_error,
            )
            .map(drop)
          }
        }
      };
    }
    error_on_empty!(u8, u8, 0, 1);
    error_on_empty!(i8, i8, 0, 1);
    error_on_empty!(u16, u16, 0, 1);
    error_on_empty!(i16, i16, 0, 1);
    error_on_empty!(u32, u32, 0, 1);
    error_on_empty!(i32, i32, 0, 1);
    error_on_empty!(u64, u64, 0, 1);
    error_on_empty!(i64, i64, 0, 1);
    error_on_empty!(usize, usize, 0, 1);
    error_on_empty!(isize, isize, 0, 1);
    #[cfg(all(feature = "f16", not(feature = "alt-stable")))]
    error_on_empty!(f16, f16, 0.0, 1.0);
    error_on_empty!(f32, f32, 0.0, 1.0);
    error_on_empty!(f64, f64, 0.0, 1.0);

    #[cfg(feature = "alt-stable")]
    error_on_empty!(half_f16, half::f16, half::f16::ZERO, half::f16::ONE);
  }
}
