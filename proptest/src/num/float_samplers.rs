//-
// Copyright 2022 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Alternative uniform float samplers.
//! These samplers are used over the ones from `rand` because the ones provided by the
//! rand crate are prone to overflow. In addition, these are 'high precision' samplers
//! that are more appropriate for test data.
//! The samplers work by splitting the range into equally sized intervals and selecting
//! an iterval at random. That interval is then itself split and a new interval is
//! selected at random. The process repeats until the interval only contains two
//! floating point values at the bounds. At that stage, one is selected at random and
//! returned.

#[cfg(feature = "f16")]
pub(crate) use self::f16::F16U;
pub(crate) use self::f32::F32U;
pub(crate) use self::f64::F64U;

/// Defines a high-precision uniform float sampler module for one float type.
///
/// Emits a `pub mod` (named after `$typ`) providing a `FloatUniform`
/// (`impl UniformSampler`) and the `$wrapper` newtype (`F32U`/`F64U`/`F16U`)
/// that `num` samples through. The sampler recursively splits the range into
/// equal-width intervals and descends until a single ULP remains, avoiding the
/// overflow and precision loss of `rand`'s float `Uniform`.
macro_rules! float_sampler {
    (
        $typ: ident,
        $int_typ: ident,
        $wrapper: ident
        $(, no_std_trait = $no_std_trait:path)?
    ) => {
        mod $typ {
            use rand::prelude::*;
            use rand::distr::uniform::{
                SampleBorrow, SampleUniform, UniformSampler,
            };
            $(
                #[cfg(not(feature = "std"))]
                use $no_std_trait;
            )?
            #[must_use]
            // Returns the previous float value. In other words the greatest value representable
            // as a float such that `next_down(float) < float`. `-0.` is treated as `0.`.
            fn next_down(float: $typ) -> $typ {
                debug_assert!(float.is_finite() && float > $typ::MIN, "`next_down` invalid input: {}", float);
                if float == (0.) {
                    -$typ::from_bits(1)
                } else if float < 0. {
                    $typ::from_bits(float.to_bits() + 1)
                } else {
                    $typ::from_bits(float.to_bits() - 1)
                }
            }

            #[must_use]
            // Returns the unit in last place using the definition by John Harrison.
            // This is the distance between `a` and the next closest float. Note that
            // `ulp(1) = $typ::EPSILON/2`.
            #[allow(clippy::single_call_fn, reason = "compute one unit-in-last-place step for the float uniform sampler's interval split")]
            fn ulp(float: $typ) -> $typ {
                debug_assert!(float.is_finite() && float > $typ::MIN, "`ulp` invalid input: {}", float);
                float.abs() - next_down(float.abs())
            }

            #[derive(Copy, Clone, Debug)]
            pub(crate) struct $wrapper($typ);

            impl From<$typ> for $wrapper {
                fn from(x: $typ) -> Self {
                    $wrapper(x)
                }
            }
            impl From<$wrapper> for $typ {
                fn from(x: $wrapper) -> Self {
                    x.0
                }
            }

            #[derive(Clone, Copy, Debug)]
            pub(crate) struct FloatUniform {
                low: $typ,
                high: $typ,
                intervals: Option<IntervalCollection>,
                inclusive: bool,
            }

            impl UniformSampler for FloatUniform {

                type X = $wrapper;

                fn new<B1, B2>(low: B1, high: B2) -> Result<Self, rand::distr::uniform::Error>
                where
                    B1: SampleBorrow<Self::X> + Sized,
                    B2: SampleBorrow<Self::X> + Sized,
                {
                    let low = low.borrow().0;
                    let high = high.borrow().0;
                    if !(low.is_finite() && high.is_finite()) {
                        return Err(rand::distr::uniform::Error::NonFinite);
                    }
                    if !(high - low > 0.) {
                        return Err(rand::distr::uniform::Error::EmptyRange);
                    }
                    Ok(FloatUniform {
                        low,
                        high,
                        intervals: Some(split_interval([low, high])),
                        inclusive: false,
                    })
                }

                fn new_inclusive<B1, B2>(low: B1, high: B2) -> Result<Self, rand::distr::uniform::Error>
                where
                    B1: SampleBorrow<Self::X> + Sized,
                    B2: SampleBorrow<Self::X> + Sized,
                {
                    let low = low.borrow().0;
                    let high = high.borrow().0;
                    if !(low.is_finite() && high.is_finite()) {
                        return Err(rand::distr::uniform::Error::NonFinite);
                    }
                    if low > high {
                        return Err(rand::distr::uniform::Error::EmptyRange);
                    }

                    // A single-point inclusive range is well-defined and yields `low`.
                    let intervals = if low == high {
                        None
                    } else {
                        Some(split_interval([low, high]))
                    };

                    Ok(FloatUniform {
                        low,
                        high,
                        intervals,
                        inclusive: true,
                    })
                }

                fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
                    let initial = match self.intervals {
                        Some(i) => i,
                        None => return $wrapper(self.low),
                    };
                    let mut intervals = initial;
                    while intervals.count > 1 {
                        let new_interval = intervals.get(rng.random_range(0..intervals.count));
                        intervals = split_interval(new_interval);
                    }
                    let last = intervals.get(0);
                    let result = *last.choose(rng).expect("Slice is not empty");

                    // These results could happen because the first split might
                    // overshoot one of the bounds. We could resample in this
                    // case but for testing data this is not a problem.
                    let clamped_result = if result < self.low {
                        debug_assert!(self.low - result < initial.step);
                        self.low
                    } else if result > self.high{
                        debug_assert!(result - self.high < initial.step);
                        self.high
                    } else {
                        result
                    };

                    if !self.inclusive && clamped_result == self.high  {
                        return $wrapper(next_down(self.high));
                    };

                    $wrapper(clamped_result)
                }
            }

            impl SampleUniform for $wrapper {
                type Sampler = FloatUniform;
            }

            // Divides the range [low, high] into intervals of size epsilon * max(abs(low, high));
            // Note that the one interval may extend out of the range.
            #[derive(Clone, Copy, Debug)]
            struct IntervalCollection {
                start: $typ,
                step: $typ,
                count: $int_typ,
            }

            fn split_interval([low, high]: [$typ; 2]) -> IntervalCollection {
                    // The `FloatUniform` constructors validate their bounds
                    // and return typed errors before calling here, so these
                    // are internal invariants rather than input checks.
                    debug_assert!(low.is_finite(), "low finite");
                    debug_assert!(high.is_finite(), "high finite");
                    debug_assert!(high - low > 0., "invalid range");

                    let min_abs = $typ::min(low.abs(), high.abs());
                    let max_abs = $typ::max(low.abs(), high.abs());

                    let gap = ulp(max_abs);

                    let (start, step) = if low.abs() < high.abs() {
                        (high, -gap)
                    } else {
                        (low, gap)
                    };

                    let min_gaps = min_abs / gap;
                    let max_gaps = max_abs / gap;
                    debug_assert!(
                        max_gaps.floor() == max_gaps,
                        "max_gaps is an integer"
                    );

                    let count = if low.signum() == high.signum() {
                        max_gaps as $int_typ - min_gaps.floor() as $int_typ
                    } else {
                        // `step` is a power of two so `min_gaps` won't be rounded
                        // except possibly to 0.
                        if min_gaps == 0. && min_abs > 0. {
                            max_gaps as $int_typ + 1
                        } else {
                            max_gaps as $int_typ + min_gaps.ceil() as $int_typ
                        }
                    };

                    debug_assert!(count - 1 <= 2 * MAX_PRECISE_INT);

                    IntervalCollection {
                        start,
                        step,
                        count,
                    }
            }


            impl IntervalCollection {
                fn get(&self, index: $int_typ) -> [$typ; 2] {
                    assert!(index < self.count, "index out of bounds");

                    // `index` might be greater that `MAX_PERCISE_INT`
                    // which means `MAX_PRECIST_INT as $typ` would round
                    // to a different number. Fortunately, `index` will
                    // never be larger than `2 * MAX_PRECISE_INT` (as
                    // asserted above).
                    let x = ((index / 2) as $typ).mul_add(
                        2. * self.step,
                        (index % 2) as $typ * self.step + self.start,
                    );

                    let y = x + self.step;

                    if self.step > 0. {
                        [x, y]
                    } else {
                        [y, x]
                    }
                }
            }


            // Values greater than MAX_PRECISE_INT may be rounded when converted to float.
            const MAX_PRECISE_INT: $int_typ =
                <$int_typ>::pow(2, $typ::MANTISSA_DIGITS);

            #[cfg(test)]
            mod test {

                use strict_test_support::{
                    TestFailure, ensure, ensure_eq, ensure_ok, ensure_some,
                };

                use super::*;
                use crate::prelude::*;

                #[test]
                fn uniform_constructors_accept_valid_ranges()
                -> Result<(), TestFailure> {
                    ensure(
                        FloatUniform::new($wrapper(1.), $wrapper(2.)).is_ok(),
                        "new accepts a finite non-empty range",
                    )?;
                    ensure(
                        FloatUniform::new_inclusive(
                            $wrapper(1.),
                            $wrapper(1.),
                        )
                        .is_ok(),
                        "new_inclusive accepts a single-point range",
                    )
                }

                #[test]
                fn uniform_constructors_reject_invalid_ranges()
                -> Result<(), TestFailure> {
                    ensure(
                        matches!(
                            FloatUniform::new($wrapper(2.), $wrapper(1.)),
                            Err(rand::distr::uniform::Error::EmptyRange)
                        ),
                        "new rejects a reversed range as empty",
                    )?;
                    ensure(
                        matches!(
                            FloatUniform::new($wrapper(1.), $wrapper(1.)),
                            Err(rand::distr::uniform::Error::EmptyRange)
                        ),
                        "new rejects a single-point exclusive range as empty",
                    )?;
                    ensure(
                        matches!(
                            FloatUniform::new(
                                $wrapper($typ::NAN),
                                $wrapper(1.),
                            ),
                            Err(rand::distr::uniform::Error::NonFinite)
                        ),
                        "new rejects a non-finite bound",
                    )?;
                    ensure(
                        matches!(
                            FloatUniform::new_inclusive(
                                $wrapper(2.),
                                $wrapper(1.),
                            ),
                            Err(rand::distr::uniform::Error::EmptyRange)
                        ),
                        "new_inclusive rejects a reversed range as empty",
                    )
                }

                #[allow(clippy::single_call_fn, reason = "test-only helper ordering a float pair before the uniform sampler round-trip check")]
                fn sort((left, right): ($typ, $typ)) -> ($typ, $typ) {
                    if left < right {
                        (left, right)
                    } else {
                        (right, left)
                    }
                }

                fn finite() -> impl Strategy<Value = $typ> {
                    prop::num::$typ::NEGATIVE
                    | prop::num::$typ::POSITIVE
                    | prop::num::$typ::NORMAL
                    | prop::num::$typ::SUBNORMAL
                    | prop::num::$typ::ZERO
                }

                fn finite_above_min() -> impl Strategy<Value = $typ> {
                    // The legacy tests rejected `MIN` with `prop_assume!`;
                    // the precondition lives in the strategy instead.
                    finite().prop_filter(
                        "value must be above the type minimum",
                        |val| *val > $typ::MIN,
                    )
                }

                fn bounds() -> impl Strategy<Value = ($typ, $typ)> {
                    (finite(), finite())
                        .prop_filter("Bounds can't be equal", |(left, right)| left != right)
                        .prop_map(sort)
                }

                #[test]
                fn range_test() -> Result<(), TestFailure> {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (-1., 10.);
                    let uniform = ensure_ok(
                        FloatUniform::new($wrapper(low), $wrapper(high)),
                        "the bounds form a uniform sampler",
                    )?;

                    let samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    for sample in samples {
                        ensure(
                            low <= sample && sample < high,
                            "every sample stays within the half-open range",
                        )?;
                    }
                    Ok(())
                }

                #[test]
                fn range_end_bound_test() -> Result<(), TestFailure> {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (1., 1. + $typ::EPSILON);
                    let uniform = ensure_ok(
                        FloatUniform::new($wrapper(low), $wrapper(high)),
                        "the bounds form a uniform sampler",
                    )?;

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    ensure(
                        samples.all(|x| x == 1.),
                        "a one-ulp half-open range only yields its base",
                    )
                }

                #[test]
                fn inclusive_range_test() -> Result<(), TestFailure> {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (-1., 10.);
                    let uniform = ensure_ok(
                        FloatUniform::new_inclusive($wrapper(low), $wrapper(high)),
                        "the bounds form a uniform sampler",
                    )?;

                    let samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    for sample in samples {
                        ensure(
                            low <= sample && sample <= high,
                            "every sample stays within the inclusive range",
                        )?;
                    }
                    Ok(())
                }

                #[test]
                fn inclusive_range_end_bound_test() -> Result<(), TestFailure> {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (1., 1. + $typ::EPSILON);
                    let uniform = ensure_ok(
                        FloatUniform::new_inclusive($wrapper(low), $wrapper(high)),
                        "the bounds form a uniform sampler",
                    )?;

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    ensure(
                        samples.any(|x| x == 1. + $typ::EPSILON),
                        "the inclusive end bound is sampled",
                    )
                }

                #[test]
                fn inclusive_range_single_point() -> Result<(), TestFailure> {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let point: $typ = 0.0;
                    let uniform = ensure_ok(
                        FloatUniform::new_inclusive($wrapper(point), $wrapper(point)),
                        "a single-point range forms a uniform sampler",
                    )?;
                    for _ in 0..16 {
                        ensure_eq(
                            &$typ::from(uniform.sample(&mut test_rng)),
                            &point,
                            "a single-point range always yields its point",
                        )?;
                    }
                    Ok(())
                }

                #[test]
                fn inclusive_range_single_point_strategy() -> Result<(), TestFailure> {
                    use crate::test_runner::TestRunner;
                    use crate::strategy::{Strategy, ValueTree};

                    let mut runner = TestRunner::default();
                    let mid: $typ = 1.5;
                    let tree = ensure_some(
                        (mid..=mid).new_tree(&mut runner).ok(),
                        "a single-point inclusive range generates a value tree",
                    )?;
                    ensure_eq(
                        &tree.current(),
                        &mid,
                        "the single-point strategy yields its point",
                    )
                }

                #[test]
                fn all_floats_in_range_are_possible_1() -> Result<(), TestFailure> {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (1. - $typ::EPSILON, 1. + $typ::EPSILON);
                    let uniform = ensure_ok(
                        FloatUniform::new_inclusive($wrapper(low), $wrapper(high)),
                        "the bounds form a uniform sampler",
                    )?;

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    ensure(
                        samples.any(|x| x == 1. - $typ::EPSILON / 2.),
                        "an interior float of the range is sampled",
                    )
                }

                #[test]
                fn all_floats_in_range_are_possible_2() -> Result<(), TestFailure> {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (0., MAX_PRECISE_INT as $typ);
                    let uniform = ensure_ok(
                        FloatUniform::new_inclusive($wrapper(low), $wrapper(high)),
                        "the bounds form a uniform sampler",
                    )?;

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)))
                        .map(|x| x.fract());

                    ensure(
                        samples.any(|x| x != 0.),
                        "fractional values are sampled across the range",
                    )
                }

                #[test]
                fn max_precise_int_plus_one_is_rounded_down() -> Result<(), TestFailure> {
                    ensure_eq(
                        &(((MAX_PRECISE_INT + 1) as $typ) as $int_typ),
                        &MAX_PRECISE_INT,
                        "the first imprecise integer rounds back down",
                    )
                }

                #[test]
                fn next_down_less_than_float() -> Result<(), TestFailure> {
                    crate::strict::ensure_property(
                        &finite_above_min(),
                        "next_down is strictly below its input",
                        |val| {
                            ensure(
                                next_down(val) < val,
                                "next_down yields a smaller float",
                            )
                        },
                    )
                }

                #[test]
                fn no_value_between_float_and_next_down() -> Result<(), TestFailure> {
                    crate::strict::ensure_property(
                        &finite_above_min(),
                        "next_down is the immediate predecessor",
                        |val| {
                            let prev = next_down(val);
                            let avg = prev / 2. + val / 2.;
                            ensure(
                                avg == prev || avg == val,
                                "no float lies between a value and its \
                                 next_down",
                            )
                        },
                    )
                }

                #[test]
                fn values_less_than_or_equal_to_max_precise_int_are_not_rounded() -> Result<(), TestFailure> {
                    crate::strict::ensure_property(
                        &(0..=MAX_PRECISE_INT),
                        "precise integers survive a float round trip",
                        |i| {
                            ensure_eq(
                                &((i as $typ) as $int_typ),
                                &i,
                                "the round-tripped integer is unchanged",
                            )
                        },
                    )
                }

                #[test]
                fn indivisible_intervals_are_split_to_self() -> Result<(), TestFailure> {
                    crate::strict::ensure_property(
                        &finite_above_min(),
                        "a one-ulp interval is indivisible",
                        |val| {
                            let prev = next_down(val);
                            let intervals = split_interval([prev, val]);
                            ensure_eq(
                                &intervals.count,
                                &1,
                                "the indivisible interval splits to itself",
                            )
                        },
                    )
                }

                #[test]
                fn split_intervals_are_the_same_size() -> Result<(), TestFailure> {
                    // The legacy test `prop_assume!`d a non-trivial split;
                    // the precondition lives in the strategy filter instead.
                    let inputs = (bounds(), any::<[prop::sample::Index; 32]>())
                        .prop_filter(
                            "the bounds must split into at least two \
                             intervals",
                            |((low, high), _)| {
                                split_interval([*low, *high]).count > 1
                            },
                        );
                    crate::strict::ensure_property(
                        &inputs,
                        "split intervals share one width",
                        |((low, high), indices)| {
                            let intervals = split_interval([low, high]);
                            let size = (intervals.count - 1) as usize;

                            let mut it = indices.iter()
                                .map(|i| i.index(size) as $int_typ)
                                .map(|i| intervals.get(i))
                                .map(|[low, high]| high - low);

                            let interval_size = ensure_some(
                                it.next(),
                                "at least one interval is sampled",
                            )?;
                            ensure(
                                it.all(|width| width == interval_size),
                                "every sampled interval has the same width",
                            )
                        },
                    )
                }

                #[test]
                fn split_intervals_are_consecutive() -> Result<(), TestFailure> {
                    let inputs = (bounds(), any::<[prop::sample::Index; 32]>())
                        .prop_filter(
                            "the bounds must split into at least three \
                             intervals",
                            |((low, high), _)| {
                                split_interval([*low, *high]).count > 2
                            },
                        );
                    crate::strict::ensure_property(
                        &inputs,
                        "split intervals are consecutive",
                        |((low, high), indices)| {
                            let intervals = split_interval([low, high]);
                            let size = (intervals.count - 1) as usize;

                            let mut it = indices.iter()
                                .map(|i| i.index(size - 1) as $int_typ)
                                .map(|i| (intervals.get(i), intervals.get(i + 1)));

                            let ascending = it.all(|([_, h1], [l2, _])| h1 == l2);
                            let descending = it.all(|([l1, _], [_, h2])| l1 == h2);

                            ensure(
                                ascending || descending,
                                "adjacent intervals share a bound in one \
                                 direction",
                            )
                        },
                    )
                }

                #[test]
                fn first_split_might_slightly_overshoot_one_bound() -> Result<(), TestFailure> {
                    crate::strict::ensure_property(
                        &bounds(),
                        "the first split covers the bounds with at most one \
                         overshoot",
                        |(low, high)| {
                            let intervals = split_interval([low, high]);
                            let start = intervals.get(0);
                            let end = intervals.get(intervals.count - 1);
                            let (low_interval, high_interval) = if start[0] < end[0] {
                                (start, end)
                            } else {
                                (end, start)
                            };

                            ensure(
                                low == low_interval[0] && high_interval[0] < high && high <= high_interval[1] ||
                                low_interval[0] <= low && low < low_interval[1] && high == high_interval[1],
                                "exactly one bound may overshoot",
                            )
                        },
                    )
                }

                #[test]
                fn subsequent_splits_always_match_bounds() -> Result<(), TestFailure> {
                    crate::strict::ensure_property(
                        &(bounds(), any::<prop::sample::Index>()),
                        "recursive splits stay within their interval",
                        |((low, high), index)| {
                            // This property is true because the distances of split intervals of
                            // are powers of two so the smaller one always divides the larger.

                            let intervals = split_interval([low, high]);
                            let size = (intervals.count - 1) as usize;

                            let interval = intervals.get(index.index(size) as $int_typ);
                            let small_intervals = split_interval(interval);

                            let start = small_intervals.get(0);
                            let end = small_intervals.get(small_intervals.count - 1);
                            let (low_interval, high_interval) = if start[0] < end[0] {
                                (start, end)
                            } else {
                                (end, start)
                            };

                            ensure(
                                interval[0] == low_interval[0] &&
                                interval[1] == high_interval[1],
                                "the sub-split spans exactly its parent \
                                 interval",
                            )
                        },
                    )
                }
            }
        }
    };
}

#[cfg(feature = "f16")]
float_sampler!(f16, u16, F16U);
float_sampler!(f32, u32, F32U, no_std_trait = num_traits::float::Float);
float_sampler!(f64, u64, F64U, no_std_trait = num_traits::float::Float);
