//! Shared mmap-backed allocation primitives.

use std::ptr::{self, NonNull};

use crate::file_identity::{validate_file_identity, FileIdentity};
use crate::{Error, Result};

/// Linux huge page advice used for mmap-backed allocations.
#[cfg(target_os = "linux")]
const MADVISE_HUGEPAGE: libc::c_int = libc::MADV_HUGEPAGE;

/// A raw block of writable virtual memory allocated from the operating system.
#[derive(Debug)]
pub struct MmapBlock {
    ptr: NonNull<u8>,
    len: usize,
    numa_node: Option<u32>,
}

impl MmapBlock {
    /// Allocate a new anonymous writable mapping.
    /// # Errors
    ///
    /// Returns an error if `len` is zero or the underlying `mmap` fails.
    pub fn new(len: usize) -> Result<Self> {
        if len == 0 {
            return Err(Error::NullPointer);
        }

        let ptr = map_region(len)?;
        // HUGEPAGE is an advisory hint and may be rejected for very large or
        // unsupported mappings; such rejections must not fail the allocation.
        let _ = advise_hugepage(ptr, len);

        Ok(Self {
            ptr,
            len,
            numa_node: None,
        })
    }

    /// Allocate a new mapping and bind it to a NUMA node when supported.
    /// # Errors
    ///
    /// Returns an error if `len` is zero, if `mmap` fails, or if the NUMA
    /// node is invalid or binding fails.
    pub fn new_on_node(len: usize, node: u32) -> Result<Self> {
        if len == 0 {
            return Err(Error::NullPointer);
        }

        let ptr = map_region(len)?;
        // HUGEPAGE is an advisory hint and may be rejected for very large or
        // unsupported mappings; such rejections must not fail the allocation.
        let _ = advise_hugepage(ptr, len);

        #[cfg(target_os = "linux")]
        {
            if let Err(error) = bind_to_numa_node(ptr, len, node) {
                unmap_region(ptr, len);
                return Err(error);
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = node;
        }

        Ok(Self {
            ptr,
            len,
            #[cfg(target_os = "linux")]
            numa_node: Some(node),
            #[cfg(not(target_os = "linux"))]
            numa_node: None,
        })
    }

    /// Immutable raw pointer to the start of the mapping.
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr().cast_const()
    }

    /// Mutable raw pointer to the start of the mapping.
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Length of the mapping in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the mapping is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// NUMA node this block is bound to, if any.
    #[must_use]
    pub fn numa_node(&self) -> Option<u32> {
        self.numa_node
    }
}

impl Drop for MmapBlock {
    fn drop(&mut self) {
        unmap_region(self.ptr, self.len);
    }
}

// SAFETY: The mapping owns a unique virtual address range and can be transferred
// across threads without aliasing.
unsafe impl Send for MmapBlock {}

fn map_region(len: usize) -> Result<NonNull<u8>> {
    #[cfg(target_os = "linux")]
    let flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
    #[cfg(not(target_os = "linux"))]
    let flags = libc::MAP_PRIVATE | libc::MAP_ANON;

    // SAFETY: Arguments follow mmap contract. Returned pointer is checked.
    let raw_ptr = unsafe {
        libc::mmap(
            ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            flags,
            -1,
            0,
        )
    };

    if raw_ptr == libc::MAP_FAILED {
        return Err(Error::System {
            operation: "mmap",
            source: std::io::Error::last_os_error(),
        });
    }

    NonNull::new(raw_ptr.cast::<u8>()).ok_or(Error::NullPointer)
}

fn unmap_region(ptr: NonNull<u8>, len: usize) {
    // SAFETY: `ptr,len` were returned by mmap and are still owned by this type.
    let _ = unsafe { libc::munmap(ptr.as_ptr().cast::<libc::c_void>(), len) };
}

#[cfg(target_os = "linux")]
fn advise_hugepage(ptr: NonNull<u8>, len: usize) -> Result<()> {
    // SAFETY: pointer and length represent a currently-valid mapping.
    let result = unsafe { libc::madvise(ptr.as_ptr().cast::<libc::c_void>(), len, MADVISE_HUGEPAGE) };
    if result != 0 {
        return Err(Error::System {
            operation: "madvise(HUGEPAGE)",
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn advise_hugepage(_ptr: NonNull<u8>, _len: usize) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn bind_to_numa_node(ptr: NonNull<u8>, len: usize, node: u32) -> Result<()> {
    crate::numa::bind_memory_to_node(ptr, len, node)
}

#[cfg(not(target_os = "linux"))]
fn bind_to_numa_node(_ptr: NonNull<u8>, _len: usize, _node: u32) -> Result<()> {
    Ok(())
}

/// Kernel advice hints for memory-mapped files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmapAdvice {
    /// Sequential access (kernel prefetches aggressively).
    Sequential,
    /// Random access (disable readahead).
    Random,
    /// Pages will be needed soon (prefault).
    WillNeed,
}

/// Open a file as a read-only memory map with optimal kernel hints.
///
/// Applies `MADV_SEQUENTIAL` + `MADV_HUGEPAGE` on Linux and maps the file
/// lazily. The caller is responsible for ensuring the file is not truncated
/// while mapped.
///
/// **SIGBUS risk:** If the backing file is truncated after mapping,
/// accessing the corresponding pages can still raise SIGBUS.
/// Fix: ensure files are immutable or locked while mapped.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or mapped.
///
/// # Examples
///
/// ```no_run
/// let mmap = kernelkit::mmap::open_read("/etc/hosts").unwrap();
/// assert!(!mmap.is_empty());
/// ```
pub fn open_read(path: impl AsRef<std::path::Path>) -> Result<memmap2::Mmap> {
    #[cfg(test)]
    if faultkit::should_fail_mmap() {
        return Err(crate::Error::System {
            operation: "mmap",
            source: std::io::Error::other("faultkit: injected mmap failure"),
        });
    }

    open_with_advice(path, MmapAdvice::Sequential)
}

/// Open a file as a read-only map after validating its on-disk size.
///
/// Maps the file lazily and applies `MADV_SEQUENTIAL` and `MADV_HUGEPAGE` hints
/// so the kernel can optimize sequential access. The caller is responsible
/// for ensuring the file is not truncated while mapped.
///
/// **SIGBUS risk:** If the backing file is truncated after mapping,
/// accessing the corresponding pages can still raise SIGBUS.
/// Fix: ensure files are immutable or locked while mapped.
///
/// # Errors
///
/// Returns an error if the file size differs from `expected_size` or if the
/// file cannot be opened or mapped.
pub fn open_read_with_size(
    path: impl AsRef<std::path::Path>,
    expected_size: u64,
) -> Result<memmap2::Mmap> {
    #[cfg(test)]
    if faultkit::should_fail_mmap() {
        return Err(crate::Error::System {
            operation: "mmap",
            source: std::io::Error::other("faultkit: injected mmap failure"),
        });
    }

    let file = std::fs::File::open(path.as_ref()).map_err(|source| Error::System {
        operation: "open",
        source,
    })?;
    let metadata = file.metadata().map_err(|source| Error::System {
        operation: "metadata",
        source,
    })?;
    let expected_identity = FileIdentity::from_metadata(&metadata);
    if metadata.len() != expected_size {
        return Err(Error::System {
            operation: "open_read_with_size",
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "file size mismatch: expected {expected_size} bytes, found {} bytes",
                    metadata.len()
                ),
            ),
        });
    }

    let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(|source| Error::System {
        operation: "mmap",
        source,
    })?;
    validate_file_identity(&file, expected_identity, expected_size, "mapped file changed during open")?;

    #[cfg(target_os = "linux")]
    if !mmap.is_empty() {
        let ptr = mmap.as_ptr().cast::<libc::c_void>().cast_mut();
        let len = mmap.len();
        // SAFETY: pointer and length come from a valid mmap region.
        let sequential_result = unsafe { libc::madvise(ptr, len, libc::MADV_SEQUENTIAL) };
        if sequential_result != 0 {
            return Err(Error::System {
                operation: "madvise(SEQUENTIAL)",
                source: std::io::Error::last_os_error(),
            });
        }
        // SAFETY: pointer and length come from a valid mmap region.
        let _hugepage_result = unsafe { libc::madvise(ptr, len, libc::MADV_HUGEPAGE) };
        // HUGEPAGE is an advisory hint and may be rejected by the kernel for
        // very large or unsupported mappings; do not fail the map because of it.
    }

    Ok(mmap)
}

/// Open a file as a read-only memory map with explicit advice.
///
/// Maps the file lazily and applies the requested `MADV_*` hint followed by
/// `MADV_HUGEPAGE` so the kernel can optimize access. `MADV_HUGEPAGE` is
/// advisory and may be rejected for very large mappings; such rejections are
/// ignored. The caller is responsible for ensuring the file is not truncated
/// while mapped.
///
/// **SIGBUS risk:** If the backing file is truncated after mapping,
/// accessing the corresponding pages can still raise SIGBUS.
/// Fix: ensure files are immutable or locked while mapped.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or mapped.
pub fn open_with_advice(
    path: impl AsRef<std::path::Path>,
    advice: MmapAdvice,
) -> Result<memmap2::Mmap> {
    let file = std::fs::File::open(path.as_ref()).map_err(|source| Error::System {
        operation: "open",
        source,
    })?;
    let expected_identity = file.metadata().map_err(|source| Error::System {
        operation: "metadata",
        source,
    })?;
    let expected_identity = FileIdentity::from_metadata(&expected_identity);
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(|source| Error::System {
        operation: "mmap",
        source,
    })?;
    let mmap_len = u64::try_from(mmap.len()).map_err(|_| Error::System {
        operation: "open_with_advice",
        source: std::io::Error::other("mapped region length does not fit in u64"),
    })?;
    validate_file_identity(&file, expected_identity, mmap_len, "mapped file changed during open")?;

    #[cfg(target_os = "linux")]
    if !mmap.is_empty() {
        let ptr = mmap.as_ptr().cast::<libc::c_void>().cast_mut();
        let len = mmap.len();
        let madvise_flag = match advice {
            MmapAdvice::Sequential => libc::MADV_SEQUENTIAL,
            MmapAdvice::Random => libc::MADV_RANDOM,
            MmapAdvice::WillNeed => libc::MADV_WILLNEED,
        };
        let operation = match advice {
            MmapAdvice::Sequential => "madvise(SEQUENTIAL)",
            MmapAdvice::Random => "madvise(RANDOM)",
            MmapAdvice::WillNeed => "madvise(WILLNEED)",
        };
        // SAFETY: pointer and length come from a valid mmap region.
        let advice_result = unsafe { libc::madvise(ptr, len, madvise_flag) };
        if advice_result != 0 {
            return Err(Error::System {
                operation,
                source: std::io::Error::last_os_error(),
            });
        }
        // SAFETY: pointer and length come from a valid mmap region.
        let _hugepage_result = unsafe { libc::madvise(ptr, len, libc::MADV_HUGEPAGE) };
        // HUGEPAGE is advisory and may be rejected; do not fail the map.
    }

    Ok(mmap)
}

/// Release pages backing this mmap region back to the kernel.
///
/// Dropping the mmap is the safe equivalent of `MADV_DONTNEED` for read-only
/// mappings; the kernel reclaims all pages immediately. This prevents page
/// cache pollution when scanning millions of files at internet scale.
///
/// Call this after scanning is complete and you no longer need the mapping.
pub fn release(mmap: memmap2::Mmap) {
    drop(mmap);
}
#[cfg(test)]
mod tests {

    use super::{MmapBlock, open_read, open_read_with_size};
    use std::io::Write;

    #[test]
    fn allocates_and_exposes_len() {
        let mut block = MmapBlock::new(4096).expect("mmap block");
        assert_eq!(block.len(), 4096);
        assert!(!block.is_empty());
        assert!(!block.as_mut_ptr().is_null());
    }

    #[test]
    fn zero_length_fails() {
        let result = MmapBlock::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn write_and_read_back() {
        let mut block = MmapBlock::new(4096).expect("mmap block");
        // Write bytes through the raw pointer
        let ptr = block.as_mut_ptr();
        unsafe {
            ptr.write(0xAB);
            ptr.add(1).write(0xCD);
            assert_eq!(*ptr, 0xAB);
            assert_eq!(*ptr.add(1), 0xCD);
        }
    }

    #[test]
    fn large_allocation() {
        // 16MB, tests real mmap code path
        let block = MmapBlock::new(16 * 1024 * 1024).expect("large mmap block");
        assert_eq!(block.len(), 16 * 1024 * 1024);
    }

    #[test]
    fn numa_node_is_none_by_default() {
        let block = MmapBlock::new(4096).expect("mmap block");
        assert!(block.numa_node().is_none());
    }


    #[test]
    fn open_read_with_matching_size_succeeds() {
        let mut file = tempfile::NamedTempFile::new().expect("tempfile");
        file.write_all(b"kernelkit").expect("write data");

        let mmap = open_read_with_size(file.path(), 9).expect("mmap succeeds");
        assert_eq!(&mmap[..], b"kernelkit");
    }

    #[test]
    fn open_read_with_wrong_size_fails() {
        let mut file = tempfile::NamedTempFile::new().expect("tempfile");
        file.write_all(b"kernelkit").expect("write data");

        let error = open_read_with_size(file.path(), 8).expect_err("size mismatch");
        assert!(error.to_string().contains("file size mismatch"));
    }

    #[test]
    fn open_read_nonexistent_path_fails() {
        let error = open_read("/definitely/not/a/real/kernelkit/path").expect_err("missing file");
        assert!(error.to_string().contains("open failed"));
    }

    #[test]
    fn open_read_empty_file_fails() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        match open_read(file.path()) {
            Ok(mmap) => assert!(mmap.is_empty()),
            Err(error) => assert!(error.to_string().contains("mmap failed")),
        }
    }

    #[test]
    fn open_read_directory_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = open_read(dir.path()).expect_err("directory should fail");
        assert!(
            error.to_string().contains("mmap failed") || error.to_string().contains("open failed")
        );
    }

    #[test]
    fn open_read_dev_null_fails() {
        let error = open_read("/dev/null").expect_err("dev null should not map");
        assert!(error.to_string().contains("mmap failed"));
    }

    #[test]
    fn open_read_with_size_nonexistent_path_fails() {
        let error = open_read_with_size("/definitely/not/a/real/kernelkit/path", 1)
            .expect_err("missing file");
        assert!(error.to_string().contains("open failed"));
    }

    #[test]
    fn open_read_with_size_rejects_empty_file() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        match open_read_with_size(file.path(), 0) {
            Ok(mmap) => assert!(mmap.is_empty()),
            Err(error) => assert!(error.to_string().contains("mmap failed")),
        }
    }

    #[test]
    fn open_read_faultkit_injection_returns_contextual_error() {
        let mut file = tempfile::NamedTempFile::new().expect("tempfile");
        file.write_all(b"kernelkit").expect("write data");

        faultkit::clear();
        let _ = faultkit::inject(faultkit::Fault::Mmap { fail_after: 0 });

        let error = open_read(file.path()).expect_err("fault injection should fail mmap");
        assert!(error.to_string().contains("mmap failed"));
        assert!(
            error
                .to_string()
                .contains("faultkit: injected mmap failure")
        );

        faultkit::clear();
        let mmap = open_read(file.path()).expect("fault cleared");
        assert_eq!(&mmap[..], b"kernelkit");
    }

    #[test]
    fn open_read_with_size_faultkit_injection_returns_contextual_error() {
        let mut file = tempfile::NamedTempFile::new().expect("tempfile");
        file.write_all(b"kernelkit").expect("write data");

        faultkit::clear();
        let _ = faultkit::inject(faultkit::Fault::Mmap { fail_after: 0 });

        let error =
            open_read_with_size(file.path(), 9).expect_err("fault injection should fail mmap");
        assert!(error.to_string().contains("mmap failed"));
        assert!(
            error
                .to_string()
                .contains("faultkit: injected mmap failure")
        );

        faultkit::clear();
        let mmap = open_read_with_size(file.path(), 9).expect("fault cleared");
        assert_eq!(&mmap[..], b"kernelkit");
    }
}
