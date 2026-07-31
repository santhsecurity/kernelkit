//! Lightweight `perf_event_open` wrapper for hardware counter reading.
//!
//! Enables zero-overhead profiling of cache misses, branch mispredicts,
//! and instruction counts within scan hot paths.

use crate::{Error, Result};
use std::os::unix::io::RawFd;

#[repr(C)]
struct PerfEventAttr {
    type_: u32,
    size: u32,
    config: u64,
    // Remaining fields are zero-initialized.
    // 120 bytes of padding gives a total size of 128, matching the largest
    // known Linux kernel perf_event_attr (as of 6.x). Older kernels only
    // copy what they understand, so a larger size is always safe.
    _rest: [u8; 120],
}

/// Hardware performance counter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwCounter {
    /// Total CPU cycles.
    Cycles,
    /// Total retired instructions.
    Instructions,
    /// L1 data cache misses.
    CacheMisses,
    /// Branch mispredictions.
    BranchMisses,
}

/// An open hardware performance counter.
#[derive(Debug)]
pub struct PerfCounter {
    fd: RawFd,
    counter_type: HwCounter,
}

impl PerfCounter {
    /// Open a hardware performance counter for the calling thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the kernel rejects the `perf_event_open` syscall
    /// (e.g., insufficient permissions or unsupported counter).
    #[cfg(target_os = "linux")]
    pub fn open(counter: HwCounter) -> Result<Self> {
        let config = match counter {
            HwCounter::Cycles => 0,       // PERF_COUNT_HW_CPU_CYCLES
            HwCounter::Instructions => 1, // PERF_COUNT_HW_INSTRUCTIONS
            HwCounter::CacheMisses => 3,  // PERF_COUNT_HW_CACHE_MISSES
            HwCounter::BranchMisses => 5, // PERF_COUNT_HW_BRANCH_MISSES
        };

        // SAFETY: All-zeros is valid for perf_event_attr.
        let mut attr: PerfEventAttr = unsafe { std::mem::zeroed() };
        attr.type_ = 0; // PERF_TYPE_HARDWARE
        attr.size = u32::try_from(std::mem::size_of::<PerfEventAttr>()).unwrap_or(128);
        attr.config = config;

        // SAFETY: perf_event_open is a safe syscall when called correctly.
        let fd = i32::try_from(unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &raw const attr,
                0i32,  // pid = 0 (self)
                -1i32, // cpu = -1 (any)
                -1i32, // group_fd = -1 (no group)
                0u64,  // flags
            )
        })
        .unwrap_or(-1) as RawFd;

        if fd < 0 {
            return Err(Error::System {
                operation: "perf_event_open",
                source: std::io::Error::last_os_error(),
            });
        }

        Ok(Self {
            fd,
            counter_type: counter,
        })
    }

    /// Non-Linux stub.
    #[cfg(not(target_os = "linux"))]
    pub fn open(counter: HwCounter) -> Result<Self> {
        Err(Error::System {
            operation: "perf_event_open",
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "perf_event_open is Linux-only",
            ),
        })
    }

    /// Read the current counter value.
    ///
    /// # Errors
    /// Returns an error if reading from the counter fails.
    pub fn read(&self) -> Result<u64> {
        let mut value = 0u64;
        let n = unsafe {
            libc::read(
                self.fd,
                (&raw mut value).cast::<libc::c_void>(),
                std::mem::size_of::<u64>(),
            )
        };
        if n == 0 {
            return Err(Error::System {
                operation: "perf counter read",
                source: std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "perf counter returned EOF (counter may have been closed)",
                ),
            });
        }
        if n != std::mem::size_of::<u64>().try_into().unwrap_or(0) {
            return Err(Error::System {
                operation: "perf counter read",
                source: std::io::Error::last_os_error(),
            });
        }
        Ok(value)
    }

    /// Reset the counter to zero.
    ///
    /// # Errors
    /// Returns an error if resetting the counter fails.
    pub fn reset(&self) -> Result<()> {
        const PERF_EVENT_IOC_RESET: libc::c_ulong = 0x2403;
        let result = unsafe { libc::ioctl(self.fd, PERF_EVENT_IOC_RESET, 0) };
        if result < 0 {
            return Err(Error::System {
                operation: "perf counter reset",
                source: std::io::Error::last_os_error(),
            });
        }
        Ok(())
    }

    /// The counter type.
    #[must_use]
    pub fn counter_type(&self) -> HwCounter {
        self.counter_type
    }
}

impl Drop for PerfCounter {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(test)]
impl PerfCounter {
    /// Construct a counter around an existing file descriptor.
    /// The fd will be closed when the counter is dropped.
    pub(crate) fn from_raw_fd(fd: RawFd, counter_type: HwCounter) -> Self {
        Self { fd, counter_type }
    }
}

#[cfg(test)]
mod tests {
    use super::{HwCounter, PerfCounter};

    #[test]
    fn read_returns_unexpected_eof_on_zero_byte_read() {
        // A pipe whose write end is closed returns 0 on read, which must be
        // reported as UnexpectedEof rather than a stale last_os_error.
        let mut pipe = [0; 2];
        let result = unsafe { libc::pipe(pipe.as_mut_ptr()) };
        assert_eq!(result, 0, "pipe creation failed");
        unsafe { libc::close(pipe[1]) };

        let counter = PerfCounter::from_raw_fd(pipe[0], HwCounter::Cycles);
        let err = counter.read().unwrap_err();
        let source = err.to_string();
        assert!(
            source.contains("EOF") || source.contains("unexpected end"),
            "expected UnexpectedEof-like error, got: {source}"
        );
        assert_eq!(counter.counter_type(), HwCounter::Cycles);
    }
}
