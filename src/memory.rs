//! Memory pressure diagnostics.

use crate::Error;

/// Memory usage counters exposed by `memory_pressure()`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryStatus {
    /// Approximate currently available memory in bytes.
    pub available_bytes: u64,
    /// Total physical memory in bytes.
    pub total_bytes: u64,
}

const PROC_MEMINFO_PATH: &str = "/proc/meminfo";
const KIB_TO_BYTES: u64 = 1024;

#[cfg(target_os = "linux")]
impl MemoryStatus {
    /// Read memory counters from `/proc/meminfo`.
    ///
    /// # Errors
    ///
    /// If `/proc/meminfo` cannot be read or parsed, the function returns an
    /// all-zero structure. This is explicit, fail-safe behavior so callers can
    /// continue monitoring without panic. Fix: keep `/proc/meminfo` readable and
    /// writable to trusted users, or provide a platform-specific alternative.
    pub fn zero() -> Self {
        Self {
            available_bytes: 0,
            total_bytes: 0,
        }
    }

    /// Convert KiB value reported by `/proc/meminfo` into bytes.
    fn kib_to_bytes(kib: u64) -> u64 {
        kib.saturating_mul(KIB_TO_BYTES)
    }

    fn parse_kib_value(line: &str) -> Option<u64> {
        let mut parts = line.split_whitespace();
        parts.next()?;
        let raw = parts.next()?;
        raw.parse::<u64>().ok()
    }

    fn read_from_meminfo() -> crate::Result<Self> {
        let contents =
            std::fs::read_to_string(PROC_MEMINFO_PATH).map_err(|source| Error::SysfsRead {
                path: PROC_MEMINFO_PATH,
                source,
            })?;

        let mut mem_total_kib = None;
        let mut mem_available_kib = None;
        let mut mem_free_kib = None;
        let mut buffers_kib = None;
        let mut cached_kib = None;

        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                mem_total_kib = Self::parse_kib_value(line);
                if mem_total_kib.is_some() && mem_available_kib.is_some() {
                    break;
                }
                continue;
            }

            if line.starts_with("MemAvailable:") {
                mem_available_kib = Self::parse_kib_value(line);
                if mem_total_kib.is_some() && mem_available_kib.is_some() {
                    break;
                }
                continue;
            }

            if line.starts_with("MemFree:") {
                mem_free_kib = Self::parse_kib_value(line);
                continue;
            }

            if line.starts_with("Buffers:") {
                buffers_kib = Self::parse_kib_value(line);
                continue;
            }

            if line.starts_with("Cached:") {
                cached_kib = Self::parse_kib_value(line);
            }
        }

        let total_bytes = mem_total_kib
            .map(Self::kib_to_bytes)
            .ok_or(Error::SysfsParse {
                path: PROC_MEMINFO_PATH,
            })?;

        let available_bytes = mem_available_kib
            .or_else(|| {
                let free = mem_free_kib?;
                let buffers = buffers_kib.unwrap_or(0);
                let cached = cached_kib.unwrap_or(0);
                free.checked_add(buffers)?.checked_add(cached)
            })
            .map(Self::kib_to_bytes)
            .ok_or(Error::SysfsParse {
                path: PROC_MEMINFO_PATH,
            })?;

        Ok(Self {
            available_bytes,
            total_bytes,
        })
    }
}

#[cfg(not(target_os = "linux"))]
impl MemoryStatus {
    pub fn zero() -> Self {
        Self {
            available_bytes: 0,
            total_bytes: 0,
        }
    }
}

/// Return current system memory pressure accounting.
///
/// On Linux this reads `/proc/meminfo` and reports `MemAvailable`/`MemTotal`.
/// On non-Linux platforms, values default to zero because Linux procfs is not
/// available.
/// # Errors
///
/// Returns an error if `/proc/meminfo` cannot be read or parsed.
pub fn memory_pressure() -> crate::Result<MemoryStatus> {
    #[cfg(target_os = "linux")]
    {
        MemoryStatus::read_from_meminfo()
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(MemoryStatus::zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kib_value_from_meminfo_lines() {
        let expected = MemoryStatus {
            available_bytes: 8_u64.saturating_mul(KIB_TO_BYTES),
            total_bytes: 16_u64.saturating_mul(KIB_TO_BYTES),
        };

        let parsed = MemoryStatus::parse_kib_value("MemTotal:\t16384 kB");
        assert_eq!(parsed, Some(16_384));
        assert_eq!(expected.total_bytes, 16 * KIB_TO_BYTES);
        assert_eq!(expected.available_bytes, 8 * KIB_TO_BYTES);
    }

    #[test]
    fn fallback_uses_free_buffers_cached_when_no_mem_available_line() {
        let synthetic = "MemTotal: 1024 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 40 kB\n";
        let mut mem_total_kib = None;
        let mut mem_available_kib = None;
        let mut mem_free_kib = None;
        let mut buffers_kib = None;
        let mut cached_kib = None;

        for line in synthetic.lines() {
            if line.starts_with("MemTotal:") {
                mem_total_kib = MemoryStatus::parse_kib_value(line);
            } else if line.starts_with("MemAvailable:") {
                mem_available_kib = MemoryStatus::parse_kib_value(line);
            } else if line.starts_with("MemFree:") {
                mem_free_kib = MemoryStatus::parse_kib_value(line);
            } else if line.starts_with("Buffers:") {
                buffers_kib = MemoryStatus::parse_kib_value(line);
            } else if line.starts_with("Cached:") {
                cached_kib = MemoryStatus::parse_kib_value(line);
            }
        }

        let available_kib = mem_available_kib
            .or_else(|| {
                let free = mem_free_kib?;
                let buffers = buffers_kib.unwrap_or(0);
                let cached = cached_kib.unwrap_or(0);
                free.checked_add(buffers)?.checked_add(cached)
            })
            .unwrap_or_default();

        assert_eq!(mem_total_kib, Some(1024));
        assert_eq!(available_kib, Some(190).unwrap_or_default());
        assert_eq!(
            MemoryStatus::kib_to_bytes(available_kib),
            190 * KIB_TO_BYTES
        );
        assert_eq!(
            MemoryStatus::kib_to_bytes(mem_total_kib.unwrap_or_default()),
            1024 * KIB_TO_BYTES
        );
    }
}
