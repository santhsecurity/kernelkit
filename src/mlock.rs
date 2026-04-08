//! Memory locking helpers.
//!
//! These functions wrap `mlock` and `munlock` where the platform supports
//! them. Zero-length requests are treated as success.

use crate::{Error, Result, non_zero_len};

/// Lock a memory region to reduce the chance of page-out.
///
/// # Example
///
/// ```rust
/// let bytes = [1_u8, 2, 3, 4];
/// kernelkit::mlock::lock_region(bytes.as_ptr(), bytes.len())?;
/// kernelkit::mlock::unlock_region(bytes.as_ptr(), bytes.len())?;
/// # Ok::<(), kernelkit::Error>(())
/// ```
/// # Errors
/// Returns an error if the kernel rejects the mlock request.
pub fn lock_region(ptr: *const u8, len: usize) -> Result<()> {
    region_call(ptr, len, libc::mlock, "mlock")
}

/// Unlock a memory region previously locked with [`lock_region`].
///
/// # Example
///
/// ```rust
/// let bytes = [0_u8; 16];
/// kernelkit::mlock::unlock_region(bytes.as_ptr(), bytes.len())?;
/// # Ok::<(), kernelkit::Error>(())
/// ```
/// # Errors
/// Returns an error if the kernel rejects the munlock request.
pub fn unlock_region(ptr: *const u8, len: usize) -> Result<()> {
    region_call(ptr, len, libc::munlock, "munlock")
}

fn region_call(
    ptr: *const u8,
    len: usize,
    operation: unsafe extern "C" fn(*const libc::c_void, libc::size_t) -> libc::c_int,
    operation_name: &'static str,
) -> Result<()> {
    if non_zero_len(len).is_none() {
        return Ok(());
    }
    if ptr.is_null() {
        return Err(Error::NullPointer);
    }

    let result = unsafe { operation(ptr.cast::<libc::c_void>(), len) };
    if result == 0 {
        Ok(())
    } else {
        Err(Error::System {
            operation: operation_name,
            source: std::io::Error::last_os_error(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{lock_region, unlock_region};

    #[test]
    fn zero_length_lock_is_a_noop() {
        assert!(lock_region(std::ptr::null(), 0).is_ok());
        assert!(unlock_region(std::ptr::null(), 0).is_ok());
    }

    #[test]
    fn null_pointer_with_non_zero_length_is_rejected() {
        let error = lock_region(std::ptr::null(), 4).expect_err("null pointer must fail");
        assert!(matches!(error, crate::Error::NullPointer));
    }
}
