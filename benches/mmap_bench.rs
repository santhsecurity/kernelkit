use criterion::{Criterion, criterion_group, criterion_main};

fn bench_page_size(c: &mut Criterion) {
    c.bench_function("page_size", |b| {
        b.iter(|| kernelkit::page_size());
    });
}

fn bench_cpu_features(c: &mut Criterion) {
    c.bench_function("cpu_features_detect", |b| {
        b.iter(|| kernelkit::cpu_features::detect());
    });
}

criterion_group!(benches, bench_page_size, bench_cpu_features);
criterion_main!(benches);
