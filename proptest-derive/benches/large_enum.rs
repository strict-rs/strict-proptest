//! Criterion benchmark for the derive's enum/union code generation.
//!
//! Times building and sampling a derived strategy — `any::<T>()`, then
//! `new_tree`, then reading the value — for `LargeEnum1` (16 `String`
//! variants) and `LargeEnum2` (16 variants each wrapping a `LargeEnum1`),
//! measuring the runtime cost of the nested-tuple union codegen that the
//! `boxed_union` feature toggles.

use std::hint::black_box;

use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use proptest::prelude::*;
use proptest::strategy::ValueTree as _;
use proptest::test_runner::TestRunner;
use proptest_derive::Arbitrary;

/// Define the same sixteen-variant workload at each nesting depth, including
/// the payload reader that keeps every generated variant observable.
macro_rules! benchmark_enums {
  ($(#[doc = $description:literal] $name:ident($payload:ty);)+) => {
    $(
      #[doc = $description]
      #[derive(Arbitrary, Debug)]
      #[proptest(no_params)]
      enum $name {
        /// First generated payload.
        V1($payload),
        /// Second generated payload.
        V2($payload),
        /// Third generated payload.
        V3($payload),
        /// Fourth generated payload.
        V4($payload),
        /// Fifth generated payload.
        V5($payload),
        /// Sixth generated payload.
        V6($payload),
        /// Seventh generated payload.
        V7($payload),
        /// Eighth generated payload.
        V8($payload),
        /// Ninth generated payload.
        V9($payload),
        /// Tenth generated payload.
        V10($payload),
        /// Eleventh generated payload.
        V11($payload),
        /// Twelfth generated payload.
        V12($payload),
        /// Thirteenth generated payload.
        V13($payload),
        /// Fourteenth generated payload.
        V14($payload),
        /// Fifteenth generated payload.
        V15($payload),
        /// Sixteenth generated payload.
        V16($payload),
      }

      impl AsRef<str> for $name {
        /// Borrow the active variant's innermost string.
        fn as_ref(&self) -> &str {
          match self {
            &Self::V1(ref payload)
            | &Self::V2(ref payload)
            | &Self::V3(ref payload)
            | &Self::V4(ref payload)
            | &Self::V5(ref payload)
            | &Self::V6(ref payload)
            | &Self::V7(ref payload)
            | &Self::V8(ref payload)
            | &Self::V9(ref payload)
            | &Self::V10(ref payload)
            | &Self::V11(ref payload)
            | &Self::V12(ref payload)
            | &Self::V13(ref payload)
            | &Self::V14(ref payload)
            | &Self::V15(ref payload)
            | &Self::V16(ref payload) => payload.as_ref(),
          }
        }
      }
    )+
  };
}

benchmark_enums! {
  /// Flat benchmark enum with sixteen generated `String` payload variants.
  LargeEnum1(String);
  /// Nested benchmark enum with sixteen generated `LargeEnum1` payload variants.
  LargeEnum2(LargeEnum1);
}

/// Build and sample one derived enum strategy, observing its payload length.
fn sample_enum<T: Arbitrary + AsRef<str>>(runner: &mut TestRunner) -> Option<usize> {
  let strategy = any::<T>();
  let tree = black_box(strategy.new_tree(runner));
  tree
    .ok()
    .map(|generated_tree| black_box(generated_tree.current().as_ref().len()))
}

/// Register the large-enum derive benchmarks with the `Criterion` harness.
#[allow(
  clippy::single_call_fn,
  reason = "criterion entry point registering the LargeEnum1 and LargeEnum2 benches"
)]
fn enum_benchmark(harness: &mut Criterion) {
  let _harness = harness
    .bench_function("enum 1", |bencher| {
      let mut runner = TestRunner::default();
      bencher.iter(|| black_box(sample_enum::<LargeEnum1>(&mut runner)));
    })
    .bench_function("enum 2", |bencher| {
      let mut runner = TestRunner::default();
      bencher.iter(|| black_box(sample_enum::<LargeEnum2>(&mut runner)));
    });
}

criterion_group!(benches, enum_benchmark);
criterion_main!(benches);
