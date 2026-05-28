# kernelkit

Part of [Santh](https://santh.dev) - open source Rust security and infrastructure tooling. Follow [@SanthProject](https://x.com/SanthProject) on X.

Cross-platform kernel optimization toolkit with Linux fast paths and safe fallbacks.

## Quick Start

```rust
let features = kernelkit::cpu_features::detect();
println!("AVX2: {}, L1: {}KB", features.avx2, features.l1_size / 1024);

let mmap = kernelkit::mmap::open_read("Cargo.toml")?;
println!("File size: {} bytes", mmap.len());
```

## Features

- **mmap** with MADV_HUGEPAGE + MADV_SEQUENTIAL hints
- **CPU affinity** — pin threads to cores or NUMA nodes
- **readahead** — prefetch files, evict pages after scan
- **perf_event** — hardware counter reading (cycles, cache misses)
- **hugepages** — HugePageVec for TLB-friendly allocations
- **mlock** — lock pattern databases in memory
- **binformat** — binary file header parsing
- **NUMA** — allocate on specific NUMA nodes

## License

MIT
