# kernelkit

Cross-platform kernel optimization toolkit with Linux fast paths and safe fallbacks.

[![Crates.io](https://img.shields.io/crates/v/kernelkit.svg)](https://crates.io/crates/kernelkit)
[![Documentation](https://docs.rs/kernelkit/badge.svg)](https://docs.rs/kernelkit)
[![Status](https://img.shields.io/badge/status-stable-brightgreen.svg)](#status)

## Features

- **mmap**: Memory mapping with `MADV_HUGEPAGE` + `MADV_SEQUENTIAL` hints and zero-copy `MmapBlock`.
- **NUMA**: Node discovery, thread pinning, memory allocation, and page migration (`SYS_mbind` / `libnuma`).
- **hugepages**: `HugePageVec` backing allocations with 2 MiB / 1 GiB transparent hugepages on Linux.
- **mlock**: RAII page locking handles (`MlockGuard`) preventing swapping to disk.
- **cpu_features**: Dynamic feature detection (AVX2, AVX-512, NEON, cache line size, L1/L2/L3 capacities).
- **prefetch**: Architecture-optimized cache prefetch hints (NTA, T0, T1, T2).
- **readahead**: Asynchronous page cache prefetching and page eviction.
- **binformat**: Zero-copy header validation for binary file formats.
- **corpus**: Thread-safe directory scanner mapping sets of files with symlink security validation.

## Quick Start

```rust
use kernelkit::{cpu_features, mmap};

fn main() -> Result<(), kernelkit::Error> {
    // Detect CPU features and cache sizing
    let features = cpu_features::detect();
    println!("AVX2: {}, L1 Cache: {} KB", features.avx2, features.l1_size / 1024);

    // Open file with memory mapping and advice hints
    let file_map = mmap::open_read("Cargo.toml")?;
    println!("Mapped file size: {} bytes", file_map.len());

    Ok(())
}
```

## NUMA Memory Allocation

```rust
use kernelkit::numa;

fn main() -> Result<(), kernelkit::Error> {
    let count = numa::node_count();
    println!("Available NUMA nodes: {}", count);

    // Allocate vector memory on NUMA node 0
    let buffer = numa::alloc_on_node::<u64>(1024, 0)?;
    assert_eq!(buffer.len(), 1024);

    Ok(())
}
```

## Status

`kernelkit` is marked as **stable**. It includes a comprehensive test suite (unit, integration, property, fault-injection, and adversarial tests) and automated fuzzing targets in `fuzz/`.

## License

MIT OR Apache-2.0
