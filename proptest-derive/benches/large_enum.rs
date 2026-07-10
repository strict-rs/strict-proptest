//! Criterion benchmark for the derive's enum/union code generation.
//!
//! Times building and sampling a derived strategy — `any::<T>()`, then
//! `new_tree`, then reading the value — for `LargeEnum1` (16 `String`
//! variants) and `LargeEnum2` (16 variants each wrapping a `LargeEnum1`),
//! measuring the runtime cost of the nested-tuple union codegen that the
//! `boxed_union` feature toggles.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use proptest::{prelude::*, strategy::ValueTree, test_runner::TestRunner};
use proptest_derive::Arbitrary;

#[derive(Arbitrary, Debug)]
#[proptest(no_params)]
enum LargeEnum1 {
    /// Variant 1 carrying a generated `String` payload.
    V1(String),
    /// Variant 2 carrying a generated `String` payload.
    V2(String),
    /// Variant 3 carrying a generated `String` payload.
    V3(String),
    /// Variant 4 carrying a generated `String` payload.
    V4(String),
    /// Variant 5 carrying a generated `String` payload.
    V5(String),
    /// Variant 6 carrying a generated `String` payload.
    V6(String),
    /// Variant 7 carrying a generated `String` payload.
    V7(String),
    /// Variant 8 carrying a generated `String` payload.
    V8(String),
    /// Variant 9 carrying a generated `String` payload.
    V9(String),
    /// Variant 10 carrying a generated `String` payload.
    V10(String),
    /// Variant 11 carrying a generated `String` payload.
    V11(String),
    /// Variant 12 carrying a generated `String` payload.
    V12(String),
    /// Variant 13 carrying a generated `String` payload.
    V13(String),
    /// Variant 14 carrying a generated `String` payload.
    V14(String),
    /// Variant 15 carrying a generated `String` payload.
    V15(String),
    /// Variant 16 carrying a generated `String` payload.
    V16(String),
}

impl LargeEnum1 {
    /// Return the length of the active variant's `String` payload.
    fn payload_len(&self) -> usize {
        match self {
            Self::V1(payload)
            | Self::V2(payload)
            | Self::V3(payload)
            | Self::V4(payload)
            | Self::V5(payload)
            | Self::V6(payload)
            | Self::V7(payload)
            | Self::V8(payload)
            | Self::V9(payload)
            | Self::V10(payload)
            | Self::V11(payload)
            | Self::V12(payload)
            | Self::V13(payload)
            | Self::V14(payload)
            | Self::V15(payload)
            | Self::V16(payload) => payload.len(),
        }
    }
}

#[derive(Arbitrary, Debug)]
#[proptest(no_params)]
enum LargeEnum2 {
    /// Variant 1 carrying a nested `LargeEnum1` payload.
    V1(LargeEnum1),
    /// Variant 2 carrying a nested `LargeEnum1` payload.
    V2(LargeEnum1),
    /// Variant 3 carrying a nested `LargeEnum1` payload.
    V3(LargeEnum1),
    /// Variant 4 carrying a nested `LargeEnum1` payload.
    V4(LargeEnum1),
    /// Variant 5 carrying a nested `LargeEnum1` payload.
    V5(LargeEnum1),
    /// Variant 6 carrying a nested `LargeEnum1` payload.
    V6(LargeEnum1),
    /// Variant 7 carrying a nested `LargeEnum1` payload.
    V7(LargeEnum1),
    /// Variant 8 carrying a nested `LargeEnum1` payload.
    V8(LargeEnum1),
    /// Variant 9 carrying a nested `LargeEnum1` payload.
    V9(LargeEnum1),
    /// Variant 10 carrying a nested `LargeEnum1` payload.
    V10(LargeEnum1),
    /// Variant 11 carrying a nested `LargeEnum1` payload.
    V11(LargeEnum1),
    /// Variant 12 carrying a nested `LargeEnum1` payload.
    V12(LargeEnum1),
    /// Variant 13 carrying a nested `LargeEnum1` payload.
    V13(LargeEnum1),
    /// Variant 14 carrying a nested `LargeEnum1` payload.
    V14(LargeEnum1),
    /// Variant 15 carrying a nested `LargeEnum1` payload.
    V15(LargeEnum1),
    /// Variant 16 carrying a nested `LargeEnum1` payload.
    V16(LargeEnum1),
}

impl LargeEnum2 {
    /// Return the length of the nested payload selected by the active variant.
    fn payload_len(&self) -> usize {
        match self {
            Self::V1(payload)
            | Self::V2(payload)
            | Self::V3(payload)
            | Self::V4(payload)
            | Self::V5(payload)
            | Self::V6(payload)
            | Self::V7(payload)
            | Self::V8(payload)
            | Self::V9(payload)
            | Self::V10(payload)
            | Self::V11(payload)
            | Self::V12(payload)
            | Self::V13(payload)
            | Self::V14(payload)
            | Self::V15(payload)
            | Self::V16(payload) => payload.payload_len(),
        }
    }
}

/// Sample one `LargeEnum1` value through its derived strategy.
#[allow(
    clippy::single_call_fn,
    reason = "criterion iteration sampling a derived LargeEnum1 strategy tree"
)]
fn enum1_bench(runner: &mut TestRunner) -> Option<usize> {
    let strategy = any::<LargeEnum1>();
    let tree = black_box(strategy.new_tree(runner));
    tree.ok()
        .map(|tree| black_box(tree.current().payload_len()))
}

/// Sample one `LargeEnum2` value through its derived strategy.
#[allow(
    clippy::single_call_fn,
    reason = "criterion iteration sampling a derived LargeEnum2 strategy tree"
)]
fn enum2_bench(runner: &mut TestRunner) -> Option<usize> {
    let strategy = any::<LargeEnum2>();
    let tree = black_box(strategy.new_tree(runner));
    tree.ok()
        .map(|tree| black_box(tree.current().payload_len()))
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
            bencher.iter(|| black_box(enum1_bench(&mut runner)))
        })
        .bench_function("enum 2", |bencher| {
            let mut runner = TestRunner::default();
            bencher.iter(|| black_box(enum2_bench(&mut runner)))
        });
}

criterion_group!(benches, enum_benchmark);
criterion_main!(benches);
