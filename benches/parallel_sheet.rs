//! Parallel scaling on a synthetic 1,000-expression sheet (M8).

use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use kip::{eval, eval_batch, parse, EmptyResolver, RegistryBuilder};

fn build_sheet(n: usize, reg: &kip::Registry) -> Vec<Arc<kip::Expr>> {
    (0..n)
        .map(|i| parse(&format!("{i} in + 1 ft"), reg).expect("parse"))
        .collect()
}

fn bench_parallel_sheet(c: &mut Criterion) {
    let reg = registry();
    let sheet = build_sheet(1_000, &reg);

    let mut group = c.benchmark_group("parallel_sheet_1000");
    group.throughput(Throughput::Elements(1_000));

    group.bench_function("serial", |b| {
        b.iter(|| {
            sheet
                .iter()
                .map(|expr| eval(black_box(expr.as_ref()), &reg, &EmptyResolver))
                .collect::<Vec<_>>()
        });
    });

    #[cfg(feature = "parallel")]
    group.bench_function("eval_batch", |b| {
        b.iter(|| {
            let refs: Vec<_> = sheet.iter().map(|e| e.as_ref()).collect();
            eval_batch(black_box(refs), &reg, &EmptyResolver)
        });
    });

    group.finish();
}

fn registry() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

criterion_group!(benches, bench_parallel_sheet);
criterion_main!(benches);
