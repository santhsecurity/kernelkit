//! Cross-platform kernel optimization primitives behind stable Rust APIs.
//!
//! Linux gets real kernel-backed optimizations where available. Other
//! platforms keep the API available with safe fallbacks so callers can write
//! one code path and let the platform decide how much optimization is possible.
//!
//! # Example
//!
//! ```rust
//! use kernelkit::{HugePageVec, cpu_features, prefetch};
//!
//! let mut values = HugePageVec::<u64>::new(128);
//! values.as_mut_slice()[0] = 7;
//! prefetch::prefetch_read(values.as_slice().as_ptr());
//! let features = cpu_features::detect();
//! assert!(features.cache_line_size > 0);
//! ```

#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::todo,
        clippy::unimplemented,
        clippy::panic
    )
)]
#![allow(
    clippy::doc_markdown,
    clippy::module_name_repetitions,
    clippy::must_use_candidate
)]

use std::num::NonZeroUsize;

#[cfg(not(target_arch = "wasm32"))]
pub mod affinity;
pub mod binformat;
#[cfg(not(target_arch = "wasm32"))]
pub mod corpus;
pub mod cpu_features;
#[cfg(not(target_arch = "wasm32"))]
pub mod hugepages;
#[cfg(not(target_arch = "wasm32"))]
pub mod memory;
#[cfg(not(target_arch = "wasm32"))]
pub mod mlock;
#[cfg(not(target_arch = "wasm32"))]
pub mod mmap;
#[cfg(not(target_arch = "wasm32"))]
pub mod numa;
#[cfg(not(target_arch = "wasm32"))]
pub mod perf;
#[cfg(not(target_arch = "wasm32"))]
pub mod prefetch;
#[cfg(not(target_arch = "wasm32"))]
pub mod readahead;

#[cfg(not(target_arch = "wasm32"))]
pub use affinity::{pin_to_core, pin_to_numa_node, read_irq_affinity};
#[cfg(not(target_arch = "wasm32"))]
pub use binformat::FileHeader;
#[cfg(not(target_arch = "wasm32"))]
pub use corpus::{MmapCorpus, MmapRegion};
pub use cpu_features::CpuFeatures;
#[cfg(not(target_arch = "wasm32"))]
pub use hugepages::HugePageVec;
#[cfg(not(target_arch = "wasm32"))]
pub use memory::{MemoryStatus, memory_pressure};
#[cfg(not(target_arch = "wasm32"))]
pub use mmap::MmapBlock;
#[cfg(not(target_arch = "wasm32"))]
pub use readahead::{advise_sequential, evict_pages};

/// Result type used by `kernelkit`.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by kernel-backed operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The requested allocation length overflowed the address space.
    #[error(
        "allocation for {count} values of `{type_name}` overflowed. Fix: reduce the element count or use a smaller element type."
    )]
    AllocationOverflow {
        /// Number of requested elements.
        count: usize,
        /// Rust type name for the allocation target.
        type_name: &'static str,
    },
    /// The caller supplied a null pointer for a non-zero region length.
    #[error(
        "pointer was null for a non-zero region length. Fix: pass a valid pointer or use length 0."
    )]
    NullPointer,
    /// The caller supplied a node identifier that is not available.
    #[error(
        "NUMA node {node} is not available on this machine. Fix: use a node in the range 0..{available}."
    )]
    InvalidNode {
        /// The requested node.
        node: u32,
        /// Number of available nodes.
        available: usize,
    },
    /// The operating system rejected a kernel operation.
    #[error(
        "{operation} failed: {source}. Fix: verify kernel support, process privileges, and resource limits."
    )]
    System {
        /// Name of the failing operation.
        operation: &'static str,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },
    /// A Linux-only helper could not discover the expected sysfs metadata.
    #[error(
        "could not read CPU cache metadata from `{path}`. Fix: verify `/sys` is mounted and readable, or rely on the fallback defaults."
    )]
    SysfsRead {
        /// Path that failed.
        path: &'static str,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A Linux helper found malformed sysfs metadata.
    #[error(
        "could not parse CPU cache metadata from `{path}`. Fix: verify the kernel exposes numeric cache values in sysfs."
    )]
    SysfsParse {
        /// Path that failed.
        path: &'static str,
    },
    /// A binary format file had the wrong magic bytes.
    #[error(
        "invalid binary format magic bytes. Fix: ensure the file was written by a compatible version."
    )]
    InvalidMagic,
    /// A binary format file has a newer version than supported.
    #[error(
        "unsupported binary format version {version} (max {max_version}). Fix: upgrade the reader or downgrade the file."
    )]
    UnsupportedVersion {
        /// Observed version.
        version: u64,
        /// Maximum supported.
        max_version: u64,
    },
    /// A binary format section declared a length too large for the platform.
    #[error(
        "section length {length} exceeds platform address space. Fix: ensure the file is not corrupted."
    )]
    SectionTooLarge {
        /// Declared section length.
        length: u64,
    },
    /// Unexpected end of data while reading a binary format.
    #[error(
        "unexpected end of data reading {context}: needed {needed} bytes, {remaining} available. Fix: ensure the file is not truncated."
    )]
    UnexpectedEof {
        /// What was being read.
        context: &'static str,
        /// Bytes needed.
        needed: usize,
        /// Bytes remaining.
        remaining: usize,
    },
    /// A dynamic library could not be loaded.
    #[error(
        "could not load `{library}`: {source}. Fix: install the runtime library or rely on the portable fallback path."
    )]
    LibraryLoad {
        /// Shared library name.
        library: &'static str,
        /// Loader error.
        #[source]
        source: libloading::Error,
    },
    /// A required symbol was missing from a dynamic library.
    #[error(
        "could not resolve `{symbol}` from `{library}`: {source}. Fix: install a compatible version of the runtime library."
    )]
    SymbolLoad {
        /// Shared library name.
        library: &'static str,
        /// Missing symbol.
        symbol: &'static str,
        /// Loader error.
        #[source]
        source: libloading::Error,
    },
}

fn checked_len<T>(count: usize) -> Result<usize> {
    count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(Error::AllocationOverflow {
            count,
            type_name: std::any::type_name::<T>(),
        })
}

fn non_zero_len(len: usize) -> Option<NonZeroUsize> {
    NonZeroUsize::new(len)
}

/// Return the system page size in bytes.
///
/// On Linux/macOS this queries `sysconf(_SC_PAGESIZE)` at runtime.
/// Falls back to 4096 if the query fails.
///
/// # Examples
///
/// ```rust
/// let ps = kernelkit::page_size();
/// assert!(ps >= 4096);
/// assert!(ps.is_power_of_two());
/// ```
#[must_use]
pub fn page_size() -> usize {
    #[cfg(unix)]
    {
        // SAFETY: sysconf is safe to call with _SC_PAGESIZE.
        let ps = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if ps > 0 {
            return usize::try_from(ps).unwrap_or(4096);
        }
    }
    4096
}
