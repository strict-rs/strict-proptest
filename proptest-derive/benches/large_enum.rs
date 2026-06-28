use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use proptest::{prelude::*, strategy::ValueTree, test_runner::TestRunner};
use proptest_derive::Arbitrary;

#[derive(Arbitrary, Debug)]
#[proptest(no_params)]
enum LargeEnum1 {
    V1(String),
    V2(String),
    V3(String),
    V4(String),
    V5(String),
    V6(String),
    V7(String),
    V8(String),
    V9(String),
    V10(String),
    V11(String),
    V12(String),
    V13(String),
    V14(String),
    V15(String),
    V16(String),
}

impl LargeEnum1 {
    fn payload_len(&self) -> usize {
        match self {
            Self::V1(value)
            | Self::V2(value)
            | Self::V3(value)
            | Self::V4(value)
            | Self::V5(value)
            | Self::V6(value)
            | Self::V7(value)
            | Self::V8(value)
            | Self::V9(value)
            | Self::V10(value)
            | Self::V11(value)
            | Self::V12(value)
            | Self::V13(value)
            | Self::V14(value)
            | Self::V15(value)
            | Self::V16(value) => value.len(),
        }
    }
}

#[derive(Arbitrary, Debug)]
#[proptest(no_params)]
enum LargeEnum2 {
    V1(LargeEnum1),
    V2(LargeEnum1),
    V3(LargeEnum1),
    V4(LargeEnum1),
    V5(LargeEnum1),
    V6(LargeEnum1),
    V7(LargeEnum1),
    V8(LargeEnum1),
    V9(LargeEnum1),
    V10(LargeEnum1),
    V11(LargeEnum1),
    V12(LargeEnum1),
    V13(LargeEnum1),
    V14(LargeEnum1),
    V15(LargeEnum1),
    V16(LargeEnum1),
}

impl LargeEnum2 {
    fn payload_len(&self) -> usize {
        match self {
            Self::V1(value)
            | Self::V2(value)
            | Self::V3(value)
            | Self::V4(value)
            | Self::V5(value)
            | Self::V6(value)
            | Self::V7(value)
            | Self::V8(value)
            | Self::V9(value)
            | Self::V10(value)
            | Self::V11(value)
            | Self::V12(value)
            | Self::V13(value)
            | Self::V14(value)
            | Self::V15(value)
            | Self::V16(value) => value.payload_len(),
        }
    }
}

fn enum1_bench(runner: &mut TestRunner) {
    let strategy = any::<LargeEnum1>();
    let tree = strategy.new_tree(runner);
    if let Ok(tree) = &tree {
        black_box(tree.current().payload_len());
    }
    let _ = black_box(tree);
}

fn enum2_bench(runner: &mut TestRunner) {
    let strategy = any::<LargeEnum2>();
    let tree = strategy.new_tree(runner);
    if let Ok(tree) = &tree {
        black_box(tree.current().payload_len());
    }
    let _ = black_box(tree);
}

fn enum_benchmark(c: &mut Criterion) {
    c.bench_function("enum 1", |b| {
        let mut runner = TestRunner::default();
        b.iter(|| enum1_bench(&mut runner))
    });
    c.bench_function("enum 2", |b| {
        let mut runner = TestRunner::default();
        b.iter(|| enum2_bench(&mut runner))
    });
}

criterion_group!(benches, enum_benchmark);
criterion_main!(benches);
