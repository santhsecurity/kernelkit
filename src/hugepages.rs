//! Huge-page-backed vectors.
//!
//! Linux attempts to allocate the backing storage from 2 MiB huge pages using
//! `mmap(MAP_HUGETLB)`. When the kernel rejects that request, the type falls
//! back to a normal `Vec<T>` without changing the API.

use std::marker::PhantomData;
use std::mem;
use std::ptr::{self, NonNull};
use std::slice;

use crate::{Error, Result, checked_len};

const HUGEPAGE_BYTES: usize = 2 * 1024 * 1024;

enum Backing<T> {
    Standard(Vec<T>),
    #[cfg(target_os = "linux")]
    Huge(HugeAllocation<T>),
}

#[cfg(target_os = "linux")]
struct HugeAllocation<T> {
    ptr: NonNull<T>,
    count: usize,
    map_len: usize,
    _marker: PhantomData<T>,
}

/// A vector-like allocation that prefers Linux huge pages.
///
/// `HugePageVec::new` initializes every element using `T::default()` so the
/// returned slices are always valid Rust references.
///
/// # Example
///
/// ```rust
/// use kernelkit::HugePageVec;
///
/// let mut values = HugePageVec::<u32>::new(4);
/// values.as_mut_slice()[2] = 11;
/// assert_eq!(values.as_slice(), &[0, 0, 11, 0]);
/// ```
pub struct HugePageVec<T> {
    backing: Backing<T>,
}

// SAFETY: The backing storage (Vec or mmap) owns exclusive memory.
// Vec<T> is Send+Sync when T is, and HugeAllocation owns a unique mmap region.
unsafe impl<T: Send> Send for HugePageVec<T> {}
// SAFETY: &HugePageVec only provides &[T] access, which is safe to share.
unsafe impl<T: Sync> Sync for HugePageVec<T> {}

impl<T> HugePageVec<T> {
    /// Number of elements in the allocation.
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.backing {
            Backing::Standard(values) => values.len(),
            #[cfg(target_os = "linux")]
            Backing::Huge(allocation) => allocation.count,
        }
    }

    /// Whether the allocation is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the allocation as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        match &self.backing {
            Backing::Standard(values) => values.as_slice(),
            #[cfg(target_os = "linux")]
            Backing::Huge(allocation) => allocation.as_slice(),
        }
    }

    /// Borrow the allocation as a mutable slice.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        match &mut self.backing {
            Backing::Standard(values) => values.as_mut_slice(),
            #[cfg(target_os = "linux")]
            Backing::Huge(allocation) => allocation.as_mut_slice(),
        }
    }
}

impl<T: Default> HugePageVec<T> {
    /// Allocate `count` initialized elements, preferring huge pages on Linux.
    ///
    /// When huge-page allocation is not available, the function falls back to a
    /// normal `Vec<T>`.
    ///
    /// Returns `Err` if the requested size exceeds memory bounds (OOM prevention).
    /// # Errors
    ///
    /// Returns an error if the allocation size overflows or cannot be satisfied.
    pub fn new_fallible(count: usize) -> Result<Self> {
        #[cfg(target_os = "linux")]
        if let Ok(Some(allocation)) = HugeAllocation::new(count) {
            return Ok(Self {
                backing: Backing::Huge(allocation),
            });
        }

        // Try to allocate standard Vec, avoiding panic on OOM by using try_reserve
        let mut values = Vec::new();
        if values.try_reserve(count).is_err() {
            return Err(Error::AllocationOverflow {
                count,
                type_name: std::any::type_name::<T>(),
            });
        }
        values.resize_with(count, T::default);

        // Fallback: hint the kernel to use transparent huge pages (THP).
        // This is advisory — fails silently on kernels without THP support.
        #[cfg(target_os = "linux")]
        if !values.is_empty() {
            let ptr = values.as_ptr().cast::<libc::c_void>().cast_mut();
            let byte_len = values.len() * mem::size_of::<T>();
            let _ = unsafe { libc::madvise(ptr, byte_len, libc::MADV_HUGEPAGE) };
        }
        Ok(Self {
            backing: Backing::Standard(values),
        })
    }

    /// Allocate `count` initialized elements, preferring huge pages on Linux.
    ///
    /// When huge-page allocation is not available, the function falls back to a
    /// normal `Vec<T>`.
    ///
    /// Note: Will panic if the allocation exceeds system bounds. Use `new_fallible`
    /// for strict OOM prevention.
    #[must_use]
    pub fn new(count: usize) -> Self {
        #[cfg(target_os = "linux")]
        if let Ok(Some(allocation)) = HugeAllocation::new(count) {
            return Self {
                backing: Backing::Huge(allocation),
            };
        }

        let values: Vec<T> = std::iter::repeat_with(T::default).take(count).collect();
        // Fallback: hint the kernel to use transparent huge pages (THP).
        // This is advisory — fails silently on kernels without THP support.
        #[cfg(target_os = "linux")]
        if !values.is_empty() {
            let ptr = values.as_ptr().cast::<libc::c_void>().cast_mut();
            let byte_len = values.len() * mem::size_of::<T>();
            let _ = unsafe { libc::madvise(ptr, byte_len, libc::MADV_HUGEPAGE) };
        }
        Self {
            backing: Backing::Standard(values),
        }
    }
}

#[cfg(target_os = "linux")]
struct InitGuard<T> {
    ptr: NonNull<T>,
    map_len: usize,
    initialized: usize,
}

#[cfg(target_os = "linux")]
impl<T> Drop for InitGuard<T> {
    fn drop(&mut self) {
        if self.initialized > 0 {
            unsafe {
                ptr::drop_in_place(ptr::slice_from_raw_parts_mut(
                    self.ptr.as_ptr(),
                    self.initialized,
                ));
            }
        }
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast::<libc::c_void>(), self.map_len);
        }
    }
}

#[cfg(target_os = "linux")]
impl<T> HugeAllocation<T> {
    fn new(count: usize) -> Result<Option<Self>>
    where
        T: Default,
    {
        if count == 0 || mem::size_of::<T>() == 0 {
            return Ok(None);
        }

        let byte_len = checked_len::<T>(count)?;
        let map_len = align_up(byte_len, HUGEPAGE_BYTES).ok_or(Error::AllocationOverflow {
            count,
            type_name: std::any::type_name::<T>(),
        })?;

        let raw_ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_HUGETLB,
                -1,
                0,
            )
        };

        if raw_ptr == libc::MAP_FAILED {
            return Ok(None);
        }

        let Some(typed_ptr) = NonNull::new(raw_ptr.cast::<T>()) else {
            let source = std::io::Error::last_os_error();
            unsafe {
                libc::munmap(raw_ptr, map_len);
            }
            return Err(Error::System {
                operation: "mmap",
                source,
            });
        };

        let mut guard = InitGuard {
            ptr: typed_ptr,
            map_len,
            initialized: 0,
        };

        for index in 0..count {
            unsafe {
                typed_ptr.as_ptr().add(index).write(T::default());
            }
            guard.initialized += 1;
        }

        // Successfully initialized everything; ownership transfers to HugeAllocation.
        mem::forget(guard);

        Ok(Some(Self {
            ptr: typed_ptr,
            count,
            map_len,
            _marker: PhantomData,
        }))
    }

    fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.count) }
    }

    fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.count) }
    }
}

#[cfg(target_os = "linux")]
impl<T> Drop for HugeAllocation<T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(self.ptr.as_ptr(), self.count));
            libc::munmap(self.ptr.as_ptr().cast::<libc::c_void>(), self.map_len);
        }
    }
}

const fn align_up(value: usize, alignment: usize) -> Option<usize> {
    let remainder = value % alignment;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(alignment - remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::HugePageVec;

    #[test]
    fn huge_page_vec_exposes_initialized_storage() {
        let mut values = HugePageVec::<u64>::new(8);
        assert_eq!(values.as_slice(), &[0; 8]);
        values.as_mut_slice()[3] = 19;
        assert_eq!(values.as_slice()[3], 19);
    }

    #[test]
    fn huge_page_vec_handles_zero_length() {
        let values = HugePageVec::<u8>::new(0);
        assert!(values.as_slice().is_empty());
        assert!(values.is_empty());
        assert_eq!(values.len(), 0);
    }

    #[test]
    fn huge_page_vec_len_matches_requested() {
        let values = HugePageVec::<u32>::new(1024);
        assert_eq!(values.len(), 1024);
        assert!(!values.is_empty());
    }

    #[test]
    fn huge_page_vec_large_allocation() {
        // Request enough to possibly trigger huge page path (>2MB of u64)
        let count = 512 * 1024; // 4MB worth of u64s
        let values = HugePageVec::<u64>::new(count);
        assert_eq!(values.len(), count);
        assert_eq!(values.as_slice()[0], 0);
        assert_eq!(values.as_slice()[count - 1], 0);
    }

    #[test]
    fn huge_page_vec_write_read_roundtrip() {
        let mut values = HugePageVec::<u32>::new(256);
        for i in 0..256 {
            values.as_mut_slice()[i] = i as u32 * 7;
        }
        for i in 0..256 {
            assert_eq!(values.as_slice()[i], i as u32 * 7);
        }
    }

    #[test]
    fn huge_page_vec_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<HugePageVec<u32>>();
        assert_send_sync::<HugePageVec<u8>>();
    }

    #[test]
    fn huge_page_vec_zst_works() {
        // Zero-sized types should use standard Vec fallback
        let values = HugePageVec::<()>::new(100);
        assert_eq!(values.len(), 100);
    }
}
