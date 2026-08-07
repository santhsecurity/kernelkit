# KernelKit Specification

`kernelkit` is a cross-platform kernel optimization toolkit providing Linux fast paths and portable safe fallbacks for high-performance memory management, NUMA topology control, CPU feature detection, hardware prefetching, and direct file I/O operations.

---

## 1. Subsystems & Architecture

### 1.1 Memory Mapping (`mmap`)
- **`open_read(path)`**: Maps a file read-only with `MADV_SEQUENTIAL` and `MADV_HUGEPAGE` hints. Lazy mapping avoids unnecessary page table initialization.
- **`open_with_advice(path, advice)`**: Maps a file read-only and applies a designated `MmapAdvice` (`Sequential`, `Random`, `WillNeed`, `DontNeed`).
- **`MmapBlock`**: RAII wrapper around anonymous mmap allocations supporting custom alignment, NUMA binding, page locking (`mlock`), and transparent huge page hints.

### 1.2 NUMA Topology & Placement (`numa`)
- **`current_node()`**: Returns the 0-indexed NUMA node ID of the executing CPU core using `SYS_getcpu` on Linux; `None` on other OSes.
- **`node_count()`**: Queries visible NUMA nodes via `libnuma`. Guaranteed to return $\ge 1$.
- **`pin_to_node(node)`**: Sets thread affinity to all CPU cores belonging to `node`.
- **`bind_memory_to_node(ptr, len, node)`**: Binds mapped memory ranges via `SYS_mbind` (`MPOL_BIND | MPOL_MF_MOVE`).
- **`alloc_on_node<T>(count, node)`**: Allocates initialized vector memory and migrates physical pages to `node`.
- **`migrate_to_node(ptr, byte_len, node)`**: Best-effort page migration using `libnuma::numa_tonode_memory`.

### 1.3 Huge Pages (`hugepages`)
- **`HugePageVec<T>`**: High-performance contiguous memory allocation backed by 2 MiB or 1 GiB transparent hugepages on Linux (`MAP_HUGETLB`), falling back to standard heap memory when unavailable.
- **`is_hugepage_available()`**: Probes system support for hugepage allocations.

### 1.4 Memory Locking (`mlock`)
- **`MlockGuard`**: RAII handle holding memory ranges locked in physical RAM (`libc::mlock`), preventing swapping to disk.
- **`lock_all()` / `unlock_all()`**: Configures `libc::mlockall(MCL_CURRENT | MCL_FUTURE)`.

### 1.5 CPU Hardware Detection & Prefetching (`cpu_features`, `prefetch`)
- **`detect()`**: Dynamic CPU feature detection returning `CpuFeatures` struct (AVX2, AVX-512, ARM NEON, cache line sizes, L1/L2/L3 cache capacities).
- **`prefetch_read_nta(ptr)` / `prefetch_read_t0(ptr)`**: Low-level hardware cache prefetch hints using architecture-specific SIMD intrinsics with generic no-op fallbacks.

### 1.6 Binary Format & File Corpus Parsing (`binformat`, `corpus`, `readahead`, `affinity`)
- **`FileHeader`**: Zero-copy header validation for binary file formats.
- **`MmapCorpus`**: Thread-safe directory scanner mapping sets of files with configurable size caps and symlink rejection to prevent path traversal vulnerabilities.
- **`readahead(fd, offset, count)`**: Page cache prefetching via `posix_fadvise` or `libc::readahead`.
- **`set_thread_affinity(cores)`**: CPU core pin list control.

---

## 2. Invariants & Safety Guarantees

1. **No Panics**: All operations return `Result<T, Error>` without panicking. `unwrap_used`, `expect_used`, `todo`, and `unimplemented` are denied at compile time.
2. **Bounds Safety**: `node_count()` always returns $\ge 1$. Node parameters are strictly validated prior to kernel syscalls (`SYS_mbind`, `SYS_getcpu`).
3. **Symlink Safety**: Corpus scanning explicitly rejects symlinks to prevent path traversal outside designated roots.
4. **Fallback Safety**: Systems missing `libnuma` or non-Linux targets gracefully fallback to safe standard library mechanisms without failing operations unless strong NUMA binding is requested.

---

## 3. Error Model

All operations report errors via `kernelkit::Error`:

```rust
pub enum Error {
    System { operation: &'static str, source: std::io::Error },
    InvalidNode { node: u32, available: usize },
    NullPointer,
    LibraryLoad { library: &'static str, source: libloading::Error },
    SymbolLoad { library: &'static str, symbol: &'static str, source: libloading::Error },
    InvalidHeader { message: String },
    PathTraversal { path: std::path::PathBuf },
}
```
