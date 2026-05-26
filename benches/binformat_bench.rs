use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kernelkit::binformat::FileHeader;

fn bench_binformat(c: &mut Criterion) {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 2,
    }
    .write_to(&mut buf)
    .unwrap();
    c.bench_function("file_header_read", |b| {
        b.iter(|| black_box(FileHeader::read_from(&buf, b"KIT1", 10).unwrap()));
    });
}

criterion_group!(benches, bench_binformat);
criterion_main!(benches);
