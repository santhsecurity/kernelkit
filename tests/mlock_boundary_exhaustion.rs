//! Boundary and exhaustion tests for the mlock/munlock path.
//!
//! `lock_region` must report kernel refusals honestly: locking more than
//! `RLIMIT_MEMLOCK` allows, or locking an unmapped address, returns a typed
//! `Err` — never a panic, never silent success. These tests read the real
//! rlimit so they scale to whatever the host actually enforces.

#![cfg(unix)]

use kernelkit::mlock::{lock_region, unlock_region};

/// Reads `RLIMIT_MEMLOCK`; returns `None` when the limit is infinity (the
/// exhaustion scenario is then unreachable by construction).
fn memlock_limit() -> Option<u64> {
    let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut limit) };
    assert_eq!(rc, 0, "getrlimit(RLIMIT_MEMLOCK) must succeed");
    if limit.rlim_cur == libc::RLIM_INFINITY {
        None
    } else {
        Some(limit.rlim_cur)
    }
}

/// Exhaustion: locking `RLIMIT_MEMLOCK + 2 pages` of freshly faulted memory
/// must fail with an honest error instead of pretending the region is pinned.
///
/// Why: a scanner that believes its corpus is pinned when the kernel refused
/// makes latency promises it cannot keep; the error path is the only truthful
/// answer. The region is touched page-by-page first so the kernel accounts it
/// against the limit (locking untouched anonymous memory can succeed lazily
/// on some kernels and would make this test vacuous).
#[test]
fn exhaustion_beyond_rlimit_memlock_is_an_error() {
    let Some(limit) = memlock_limit() else {
        eprintln!("RLIMIT_MEMLOCK is infinity; exhaustion scenario unreachable, skipping");
        return;
    };
    if limit > 1 << 40 {
        eprintln!("RLIMIT_MEMLOCK unreasonably large ({limit}); skipping to avoid OOM");
        return;
    }

    let page = 4096usize;
    let len = (limit as usize).saturating_add(2 * page);
    let region = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    assert_ne!(region, libc::MAP_FAILED, "anonymous mmap of {len} bytes");
    // Fault every page so the lock attempt is accounted honestly.
    for offset in (0..len).step_by(page) {
        unsafe { (region as *mut u8).add(offset).write_volatile(1) };
    }

    let result = lock_region(region as *const u8, len);
    unsafe { libc::munmap(region, len) };
    assert!(
        result.is_err(),
        "locking RLIMIT_MEMLOCK + 2 pages ({len} bytes, limit {limit}) must fail honestly"
    );
}

/// Boundary: locking an address the process never mapped must return an
/// `Err` (ENOMEM from the kernel), not a panic or a silent `Ok`.
///
/// Why: the wrapper's contract is "kernel refusal becomes `Error::System`";
/// an unmapped address is the simplest refusal the kernel offers and must
/// round-trip through the error path unchanged.
#[test]
fn boundary_locking_unmapped_address_errors() {
    // MAP_NORESERVE|MAP_ANON guard page we immediately unmap to get a
    // guaranteed-unmapped, page-aligned address.
    let page = 4096usize;
    let probe = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            page,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    assert_ne!(probe, libc::MAP_FAILED);
    unsafe { libc::munmap(probe, page) };

    let result = lock_region(probe as *const u8, page);
    assert!(result.is_err(), "locking an unmapped page must fail honestly");
}

/// Boundary: lock then unlock of a small real region round-trips, and a
/// double unlock is still a defined result (the kernel permits unlocking
/// unlocked memory), proving the wrapper does not invent state the kernel
/// does not have.
///
/// Why: pinning bookkeeping lives in the kernel; the wrapper must stay a
/// faithful pass-through so callers can reason about it from man pages alone.
#[test]
fn boundary_lock_unlock_roundtrip_is_faithful() {
    let mut bytes = vec![0xABu8; 3 * 4096];
    lock_region(bytes.as_ptr(), bytes.len()).expect("locking 3 resident pages");
    bytes[0] = 1; // still writable while locked
    unlock_region(bytes.as_ptr(), bytes.len()).expect("unlock after lock");
    unlock_region(bytes.as_ptr(), bytes.len())
        .expect("double unlock is a defined kernel result, must not panic");
}
