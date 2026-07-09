//! Parse and eval throughput (M8 criterion suite).

use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use kip::{eval, parse, EmptyResolver, RegistryBuilder};

fn registry() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn bench_parse(c: &mut Criterion) {
    let reg = registry();
    let mut group = c.benchmark_group("parse");
    for size in [100usize, 1_000, 10_000] {
        let mut src = String::from("1 ft");
        for i in 0..size.saturating_sub(1) {
            src.push_str(" + 1 in");
            let _ = i;
        }
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &src, |b, src| {
            b.iter(|| parse(black_box(src), &reg));
        });
    }
    group.finish();
}

fn bench_eval(c: &mut Criterion) {
    let reg = registry();
    let mut src = String::from("1 ft");
    for _ in 0..999 {
        src.push_str(" + 1 in");
    }
    let expr = parse(&src, &reg).expect("parse");
    let mut group = c.benchmark_group("eval");
    group.throughput(Throughput::Elements(1_000));
    group.bench_function("sum_1000_terms", |b| {
        b.iter(|| eval(black_box(expr.as_ref()), &reg, &EmptyResolver));
    });
    group.finish();
}

criterion_group!(benches, bench_parse, bench_eval);
criterion_main!(benches);
