//! Cache prefetch helpers.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use core::arch::x86_64::{_MM_HINT_T0, _MM_HINT_T1, _mm_prefetch};

/// Prefetch a pointer for an anticipated read.
///
/// # Example
///
/// ```rust
/// let values = [1_u32, 2, 3];
/// kernelkit::prefetch::prefetch_read(values.as_ptr());
/// ```
#[inline]
pub fn prefetch_read<T>(ptr: *const T) {
    prefetch_impl(ptr.cast::<u8>(), false);
}

/// Prefetch a pointer for an anticipated write.
///
/// # Example
///
/// ```rust
/// let mut values = [1_u32, 2, 3];
/// kernelkit::prefetch::prefetch_write(values.as_mut_ptr());
/// ```
#[inline]
pub fn prefetch_write<T>(ptr: *const T) {
    prefetch_impl(ptr.cast::<u8>(), true);
}

/// Prefetch a pointer with non-temporal hint (NTA).
///
/// Use for streaming data that will be used once and should not pollute
/// the cache (e.g., scanning 10GB of files where each page is read once).
///
/// # Example
///
/// ```rust
/// let data = [0u8; 4096];
/// kernelkit::prefetch::prefetch_nontemporal(data.as_ptr());
/// ```
#[inline]
pub fn prefetch_nontemporal<T>(ptr: *const T) {
    if ptr.is_null() {
        return;
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        _mm_prefetch(ptr.cast::<i8>(), core::arch::x86_64::_MM_HINT_NTA);
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("prfm pldl1strm, [{ptr}]", ptr = in(reg) ptr, options(nostack, readonly));
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64",)))]
    {
        let _ = ptr;
    }
}

/// Maximum bytes to prefetch in a single call to prevent CPU exhaustion.
pub const MAX_PREFETCH_BYTES: usize = 2 * 1024 * 1024; // 2 MB

/// Prefetch all cache lines that overlap a byte range.
///
/// Limits the prefetch to `MAX_PREFETCH_BYTES` (2MB) to prevent unbounded
/// CPU time on adversarial lengths.
///
/// # Example
///
/// ```rust
/// let bytes = [0_u8; 96];
/// // SAFETY: bytes is a valid stack allocation, len matches the array size.
/// unsafe { kernelkit::prefetch::prefetch_range(bytes.as_ptr(), bytes.len()) };
/// ```
/// # Safety
/// `ptr` must be valid for reads of `len` bytes.
pub unsafe fn prefetch_range(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    let len = len.min(MAX_PREFETCH_BYTES);
    let cache_line = crate::cpu_features::detect().cache_line_size.max(1);
    let mut offset = 0usize;
    while offset < len {
        let line_ptr = unsafe { ptr.add(offset) };
        prefetch_impl(line_ptr, false);
        offset = offset.saturating_add(cache_line);
    }
}

#[inline]
fn prefetch_impl(ptr: *const u8, write: bool) {
    if ptr.is_null() {
        return;
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        if write {
            _mm_prefetch(ptr.cast::<i8>(), _MM_HINT_T1);
        } else {
            _mm_prefetch(ptr.cast::<i8>(), _MM_HINT_T0);
        }
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        if write {
            core::arch::asm!("prfm pstl1keep, [{ptr}]", ptr = in(reg) ptr, options(nostack, readonly));
        } else {
            core::arch::asm!("prfm pldl1keep, [{ptr}]", ptr = in(reg) ptr, options(nostack, readonly));
        }
    }

    #[cfg(target_arch = "arm")]
    unsafe {
        core::arch::asm!("pld [{ptr}]", ptr = in(reg) ptr, options(nostack, readonly));
    }

    #[cfg(not(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm"
    )))]
    {
        let _ = (ptr, write);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::{prefetch_range, prefetch_read, prefetch_write};

    #[test]
    fn prefetch_helpers_accept_valid_pointers() {
        let mut values = [0_u64; 16];
        prefetch_read(values.as_ptr());
        prefetch_write(values.as_mut_ptr());
        unsafe {
            prefetch_range(values.as_ptr().cast::<u8>(), std::mem::size_of_val(&values));
        }
    }

    #[test]
    fn prefetch_range_ignores_empty_inputs() {
        unsafe {
            prefetch_range(std::ptr::null(), 0);
        }
    }
}
