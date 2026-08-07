# Changelog - kernelkit

All notable changes to `kernelkit` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.5] - 2026-08-07

### Fixed
- Fixed silent-fallback in `cpu_features::parse_cache_value` for unit suffixes (`KB`, `KiB`, `MB`, `MiB`, `GB`, `GiB`, `B`, `BYTES`), preventing silent fallback to L3 size `0` on high-end server processors with gigabyte-scale L3 caches.
- Added `probe_l1_cache_size` to inspect cache `type` (`Data` / `Unified`) so architectures with `index0` as `Instruction` cache accurately resolve L1 Data cache size.
- Fixed fatal error propagation in `corpus::advise_sequential` when `libc::madvise(..., MADV_HUGEPAGE)` returns non-zero, making `MADV_HUGEPAGE` non-fatal advisory to align with `mmap::open_with_advice` and avoid failing corpus loading on systems with disabled or unsupported THP.
- Fixed `affinity::parse_irq_affinity` `ParseIntError` when parsing `/proc/irq/<irq>/smp_affinity` masks containing `0x`/`0X` hex prefixes.
- Fixed `memory::MemoryStatus::parse_kib_value` line parsing when `/proc/meminfo` lines lack whitespace around colons.
- Fixed `numa::node_count()` on Linux falling back to `1` when `libnuma.so` is missing by parsing sysfs `/sys/devices/system/node/online` and `/sys/devices/system/node/node*` directories.

## [0.1.4] - 2026-08-07

### Changed
- Crate `authors` set to `Santh <64453045+santhreal@users.noreply.github.com>`.

### Fixed
- Fixed `node_count()` returning 0 when `libnuma`'s `numa_max_node()` returns the `-1` error sentinel, which previously caused valid node 0 operations to be erroneously rejected with `InvalidNode { node: 0, available: 0 }`.
- Validated `node_count()` invariant ensuring $\ge 1$ active NUMA node across all target platforms and fallback conditions.

### Added
- Added `package.metadata.santh` crate status tag (`stable`).
- Added formal `SPEC.md` specification document and updated documentation.

## [0.1.3] - 2026-07-14

### Fixed
- Fixed IRQ affinity core index calculations on systems with $> 32$ CPU cores (`affinity.rs`).
- Fixed property-based test memory leak in `binformat` identity test.
- Gated target-specific `readahead` tests for Linux targets in adversarial test suite.
- Replaced `Box::leak` calls with direct reference passing in test fixtures.

## [0.1.2] - 2026-04-06

### Added
- Comprehensive security audit test suite with 34 adversarial mmap and special file test cases (`/dev/null`, `/dev/zero`, `/proc` interfaces).
- Fuzzing suite covering `fuzz_binformat`, `fuzz_file_header`, and `fuzz_hugepage`.

## [0.1.0] - 2026-01-15

### Added
- Initial release of `kernelkit` cross-platform kernel optimization toolkit.
- `mmap`, `numa`, `hugepages`, `mlock`, `cpu_features`, `prefetch`, `binformat`, and `readahead` fast paths.
