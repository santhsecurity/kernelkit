//! CPU affinity helpers with Linux fast paths and safe fallbacks elsewhere.

use crate::{Error, Result};

/// Pin the calling thread to a specific CPU core.
/// # Errors
/// Returns an error if pinning fails.
pub fn pin_to_core(core_id: usize) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let cpu_set_words = usize::try_from(libc::CPU_SETSIZE)
            .unwrap_or(1024)
            .div_ceil(usize::BITS as usize);
        let max_core = cpu_set_words * usize::BITS as usize;
        if core_id >= max_core {
            return Err(Error::System {
                operation: "sched_setaffinity",
                source: std::io::Error::from(std::io::ErrorKind::InvalidInput),
            });
        }

        let mut mask = vec![0usize; cpu_set_words];
        mask[core_id / usize::BITS as usize] |= 1usize << (core_id % usize::BITS as usize);

        // SAFETY: affinity mask lives for the duration of the syscall.
        let result = unsafe {
            libc::sched_setaffinity(
                0,
                std::mem::size_of_val(mask.as_slice()),
                mask.as_ptr().cast::<libc::cpu_set_t>(),
            )
        };
        if result != 0 {
            return Err(Error::System {
                operation: "sched_setaffinity",
                source: std::io::Error::last_os_error(),
            });
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = core_id;
    }

    Ok(())
}

/// Parse an `/proc/irq/<irq>/smp_affinity` string into a list of core ids.
///
/// Linux prints the mask as comma-separated 32-bit (8 hex digit) groups,
/// least-significant group last, so each group covers 32 cores.
fn parse_irq_affinity(content: &str) -> Result<Vec<u32>> {
    let mut cores = Vec::new();
    for (group_idx, chunk) in content.trim().split(',').rev().enumerate() {
        let mask = u64::from_str_radix(chunk.trim(), 16).map_err(|_| Error::System {
            operation: "read_irq_affinity",
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("malformed affinity mask chunk: {chunk}"),
            ),
        })?;
        for bit in 0..32 {
            if mask & (1 << bit) != 0 {
                let core = u32::try_from(group_idx * 32 + bit).map_err(|_| Error::System {
                    operation: "read_irq_affinity",
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "affinity mask core index overflow",
                    ),
                })?;
                cores.push(core);
            }
        }
    }
    Ok(cores)
}

/// Read the CPU affinity mask for a specific IRQ number.
///
/// Reads `/proc/irq/<irq>/smp_affinity` to determine which cores
/// handle a given interrupt. Useful for co-locating scan threads
/// with NVMe IRQ handlers on the same NUMA socket.
///
/// # Errors
///
/// Returns an error if the sysfs file cannot be read or parsed.
pub fn read_irq_affinity(irq: u32) -> Result<Vec<u32>> {
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/irq/{irq}/smp_affinity");
        let content = std::fs::read_to_string(&path).map_err(|source| Error::System {
            operation: "read_irq_affinity",
            source,
        })?;
        parse_irq_affinity(&content)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = irq;
        Ok(Vec::new())
    }
}

/// Pin the calling thread to a specific NUMA node's cores.
/// # Errors
/// Returns an error if pinning fails.
pub fn pin_to_numa_node(node: u32) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        crate::numa::pin_to_node(node)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = node;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_irq_affinity, pin_to_core, pin_to_numa_node};

    #[test]
    fn affinity_helpers_are_non_fatal() {
        let _ = pin_to_core(0);
        let _ = pin_to_numa_node(0);
    }

    #[test]
    fn parse_irq_affinity_handles_single_32bit_group() {
        // Cores 0, 1 and 31 set in the least-significant 32-bit group.
        let cores = parse_irq_affinity("80000003").unwrap();
        assert_eq!(cores, vec![0, 1, 31]);
    }

    #[test]
    fn parse_irq_affinity_handles_multiple_32bit_groups() {
        // 64-core mask: core 32 set in the high group, cores 0 and 1 in the low group.
        let cores = parse_irq_affinity("00000001,00000003").unwrap();
        assert_eq!(cores, vec![0, 1, 32]);
    }

    #[test]
    fn parse_irq_affinity_handles_96_cores() {
        // Core 64 set in the highest group, cores 0 and 31 in lower groups.
        let cores = parse_irq_affinity("00000001,00000000,80000001").unwrap();
        assert_eq!(cores, vec![0, 31, 64]);
    }
}
