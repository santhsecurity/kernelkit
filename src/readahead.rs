//! File readahead and page cache control for scan pipelines.
//!
//! These wrappers let scan stages overlap I/O with CPU computation by
//! triggering kernel readahead before the data is needed, and evicting
//! pages after scanning to avoid polluting the page cache.

use crate::{Error, Result};
use std::os::fd::AsRawFd;

/// Advise the kernel to read ahead `count` bytes starting at `offset`.
///
/// This is a hint — the kernel may ignore it if memory pressure is high.
/// On non-Linux platforms this is a no-op.
///
/// # Errors
///
/// Returns an error if the underlying `readahead` syscall fails.
pub fn readahead(file: &impl AsRawFd, offset: u64, count: usize) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let off = libc::off64_t::try_from(offset).map_err(|_| Error::System {
            operation: "readahead",
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "offset too large for off64_t",
            ),
        })?;
        // SAFETY: readahead is safe to call on any valid fd.
        let result = unsafe { libc::readahead(file.as_raw_fd(), off, count) };
        if result != 0 {
            return Err(Error::System {
                operation: "readahead",
                source: std::io::Error::last_os_error(),
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        _ = (file, offset, count);
    }
    Ok(())
}

/// Advise the kernel that the specified byte range is no longer needed.
///
/// Calls `posix_fadvise(FADV_DONTNEED)` to evict the range from the
/// page cache. This prevents scanned files from evicting hot pages
/// used by subsequent scan stages.
///
/// On non-Linux/macOS platforms this is a no-op.
///
/// # Errors
///
/// Returns an error if `posix_fadvise` fails.
pub fn evict_pages(file: &impl AsRawFd, offset: u64, len: u64) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let off = libc::off_t::try_from(offset).map_err(|_| Error::System {
            operation: "posix_fadvise(DONTNEED)",
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "offset too large for off_t",
            ),
        })?;
        let length = libc::off_t::try_from(len).map_err(|_| Error::System {
            operation: "posix_fadvise(DONTNEED)",
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "len too large for off_t",
            ),
        })?;
        // SAFETY: posix_fadvise is safe on valid fds.
        let result = unsafe {
            libc::posix_fadvise(file.as_raw_fd(), off, length, libc::POSIX_FADV_DONTNEED)
        };
        if result != 0 {
            return Err(Error::System {
                operation: "posix_fadvise(DONTNEED)",
                source: std::io::Error::from_raw_os_error(result),
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        _ = (file, offset, len);
    }
    Ok(())
}

/// Advise the kernel that this file will be read sequentially.
///
/// Calls `posix_fadvise(FADV_SEQUENTIAL)` to double the kernel's
/// default readahead window.
///
/// # Errors
///
/// Returns an error if `posix_fadvise` fails.
pub fn advise_sequential(file: &impl AsRawFd) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let result =
            unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL) };
        if result != 0 {
            return Err(Error::System {
                operation: "posix_fadvise(SEQUENTIAL)",
                source: std::io::Error::from_raw_os_error(result),
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        _ = file;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use std::io::Write;

    #[test]
    fn readahead_on_tempfile() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        // Should not fail (kernel may ignore, but no error)
        let _ = readahead(f.as_file(), 0, 4096);
    }

    #[test]
    fn evict_pages_on_tempfile() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        let _ = evict_pages(f.as_file(), 0, 4096);
    }

    #[test]
    fn advise_sequential_on_tempfile() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        assert!(advise_sequential(f.as_file()).is_ok());
    }
}
