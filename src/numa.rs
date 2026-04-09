//! NUMA-aware helpers with Linux fast paths and portable fallbacks.

#[cfg(target_os = "linux")]
use std::sync::OnceLock;

use crate::{Error, Result, checked_len};

/// Return the current NUMA node for the calling thread when the platform can determine it.
///
/// # Example
///
/// ```rust
/// let _node = kernelkit::numa::current_node();
/// ```
#[must_use]
pub fn current_node() -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let mut cpu = 0_u32;
        let mut node = 0_u32;
        let result = unsafe {
            libc::syscall(
                libc::SYS_getcpu,
                &raw mut cpu,
                &raw mut node,
                std::ptr::null_mut::<libc::c_void>(),
            )
        };
        if result == 0 {
            return Some(node);
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Pin the calling thread to a NUMA node when supported.
///
/// # Errors
///
/// Returns an error when the node is out of range or the operating system
/// rejects the affinity request.
pub fn pin_to_node(node: u32) -> Result<()> {
    validate_node(node)?;

    #[cfg(target_os = "linux")]
    {
        if let Some(library) = LinuxNuma::load()? {
            library.run_on_node(node)?;
        }
    }

    Ok(())
}

/// Allocate initialized values and, on Linux, migrate the backing pages toward a NUMA node.
///
/// # Example
///
/// ```rust
/// let values = kernelkit::numa::alloc_on_node::<u64>(8, 0)?;
/// assert_eq!(values.len(), 8);
/// # Ok::<(), kernelkit::Error>(())
/// ```
/// # Errors
/// Returns an error if node is out of bounds or allocation fails.
pub fn alloc_on_node<T: Default>(count: usize, node: u32) -> Result<Vec<T>> {
    validate_node(node)?;
    checked_len::<T>(count)?;

    let mut values: Vec<T> = std::iter::repeat_with(T::default).take(count).collect();

    #[cfg(target_os = "linux")]
    {
        if !values.is_empty()
            && let Some(library) = LinuxNuma::load()?
            && library.has_multiple_nodes()
        {
            let byte_len = checked_len::<T>(count)?;
            library.tonode_memory(values.as_mut_ptr().cast::<libc::c_void>(), byte_len, node)?;
        }
    }

    Ok(values)
}

/// Return the number of NUMA nodes visible to the current process.
///
/// # Example
///
/// ```rust
/// assert!(kernelkit::numa::node_count() >= 1);
/// ```
#[must_use]
pub fn node_count() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(Some(library)) = LinuxNuma::load() {
            let count = library.max_node().saturating_add(1);
            return usize::try_from(count).unwrap_or(1);
        }
    }

    1
}

fn validate_node(node: u32) -> Result<()> {
    let available = node_count();
    if usize::try_from(node)
        .ok()
        .is_some_and(|index| index < available)
    {
        Ok(())
    } else {
        Err(Error::InvalidNode { node, available })
    }
}

#[cfg(target_os = "linux")]
struct LinuxNuma {
    _library: libloading::Library,
    available: unsafe extern "C" fn() -> libc::c_int,
    max_node: unsafe extern "C" fn() -> libc::c_int,
    run_on_node: unsafe extern "C" fn(libc::c_int) -> libc::c_int,
    tonode_memory:
        unsafe extern "C" fn(*mut libc::c_void, libc::size_t, libc::c_int) -> libc::c_long,
}

#[cfg(target_os = "linux")]
impl LinuxNuma {
    fn load() -> Result<Option<&'static Self>> {
        static LIBRARY: OnceLock<Option<LinuxNuma>> = OnceLock::new();
        static INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        if let Some(value) = LIBRARY.get() {
            return Ok(value.as_ref());
        }

        let _guard = INIT_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(value) = LIBRARY.get() {
            return Ok(value.as_ref());
        }

        let value = Self::try_load()?;
        if LIBRARY.set(value).is_err() {
            // Another thread initialized the cell while we held the lock.
            // This is harmless; drop our loaded value and use the cached one.
        }
        if let Some(l) = LIBRARY.get() {
            return Ok(l.as_ref());
        }
        Err(Error::System {
            operation: "numa library load",
            source: std::io::Error::other("OnceLock initialization failed unexpectedly"),
        })
    }

    fn try_load() -> Result<Option<Self>> {
        let candidates = ["libnuma.so.1", "libnuma.so"];
        for library_name in candidates {
            match Self::load_from_name(library_name) {
                Ok(Some(library)) => return Ok(Some(library)),
                Ok(None) | Err(crate::Error::LibraryLoad { .. }) => {}

                Err(other) => return Err(other),
            }
        }
        Ok(None)
    }

    fn load_from_name(library_name: &'static str) -> Result<Option<Self>> {
        let library = unsafe { libloading::Library::new(library_name) }.map_err(|source| {
            Error::LibraryLoad {
                library: library_name,
                source,
            }
        })?;

        let available = unsafe {
            *library
                .get::<unsafe extern "C" fn() -> libc::c_int>(b"numa_available\0")
                .map_err(|source| Error::SymbolLoad {
                    library: library_name,
                    symbol: "numa_available",
                    source,
                })?
        };
        let max_node = unsafe {
            *library
                .get::<unsafe extern "C" fn() -> libc::c_int>(b"numa_max_node\0")
                .map_err(|source| Error::SymbolLoad {
                    library: library_name,
                    symbol: "numa_max_node",
                    source,
                })?
        };
        let run_on_node = unsafe {
            *library
                .get::<unsafe extern "C" fn(libc::c_int) -> libc::c_int>(b"numa_run_on_node\0")
                .map_err(|source| Error::SymbolLoad {
                    library: library_name,
                    symbol: "numa_run_on_node",
                    source,
                })?
        };
        let tonode_memory = unsafe {
            *library
                .get::<unsafe extern "C" fn(*mut libc::c_void, libc::size_t, libc::c_int) -> libc::c_long>(
                    b"numa_tonode_memory\0",
                )
                .map_err(|source| Error::SymbolLoad {
                    library: library_name,
                    symbol: "numa_tonode_memory",
                    source,
                })?
        };

        let state = unsafe { available() };
        if state < 0 {
            return Ok(None);
        }

        Ok(Some(Self {
            _library: library,
            available,
            max_node,
            run_on_node,
            tonode_memory,
        }))
    }

    fn max_node(&self) -> libc::c_int {
        unsafe {
            let _ = (self.available)();
            (self.max_node)()
        }
    }

    fn has_multiple_nodes(&self) -> bool {
        self.max_node() > 0
    }

    fn run_on_node(&self, node: u32) -> Result<()> {
        let node = libc::c_int::try_from(node).map_err(|_| Error::InvalidNode {
            node,
            available: node_count(),
        })?;
        let result = unsafe { (self.run_on_node)(node) };
        if result == 0 {
            Ok(())
        } else {
            Err(Error::System {
                operation: "numa_run_on_node",
                source: std::io::Error::last_os_error(),
            })
        }
    }

    fn tonode_memory(&self, ptr: *mut libc::c_void, len: usize, node: u32) -> Result<()> {
        let node = libc::c_int::try_from(node).map_err(|_| Error::InvalidNode {
            node,
            available: node_count(),
        })?;
        let result = unsafe { (self.tonode_memory)(ptr, len, node) };
        if result == 0 {
            Ok(())
        } else {
            Err(Error::System {
                operation: "numa_tonode_memory",
                source: std::io::Error::last_os_error(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::{alloc_on_node, current_node, node_count, pin_to_node};

    #[test]
    fn node_count_is_at_least_one() {
        assert!(node_count() >= 1);
    }

    #[test]
    fn alloc_on_node_returns_initialized_values() {
        let values = alloc_on_node::<u32>(4, 0).expect("allocation on node 0 must succeed");
        assert_eq!(values, vec![0; 4]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_current_node_query_is_non_fatal() {
        let _ = current_node();
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn non_linux_current_node_is_none() {
        assert_eq!(current_node(), None);
    }

    #[test]
    fn invalid_node_is_rejected() {
        let invalid = u32::try_from(node_count()).unwrap_or(u32::MAX);
        let error = pin_to_node(invalid).expect_err("invalid node must fail");
        assert!(matches!(error, crate::Error::InvalidNode { .. }));
    }
}
