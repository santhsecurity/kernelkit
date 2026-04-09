//! Adversarial tests for mmap operations on special files and edge cases.
//!
//! These tests verify kernelkit handles dangerous file types gracefully:
//! - Zero-byte files (must not crash or return invalid pointer)
//! - Files larger than available RAM (must work via OS paging)
//! - Permission denied (must return error, not panic)
//! - Special files (/dev/zero, /proc/self/maps, etc.)
//!
//! Every finding is critical at internet scale.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;

use kernelkit::mmap::{self, MmapBlock};

// =============================================================================
// 1. ZERO-BYTE FILE TESTS
// =============================================================================

/// CRITICAL: mmap on 0-byte file must not crash or return invalid pointer.
/// The behavior should be deterministic - either consistently return error
/// or consistently return valid empty mmap.
#[test]
fn mmap_zero_byte_file_is_safe() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty.txt");

    // Create a truly empty file
    fs::File::create(&path).expect("create empty file");

    // Verify file is actually 0 bytes
    let metadata = fs::metadata(&path).expect("metadata");
    assert_eq!(metadata.len(), 0, "test file must be 0 bytes");

    // Attempt to mmap - must NOT crash
    let result = mmap::open_read(&path);

    // Acceptable outcomes:
    // 1. Ok(empty_mmap) - mmap succeeds with length 0
    // 2. Err(...) - mmap fails with descriptive error
    match result {
        Ok(mmap) => {
            // If mmap succeeds, it MUST be empty and valid
            assert_eq!(mmap.len(), 0, "0-byte file mmap must have length 0");
            // Accessing an empty mmap should be safe (no data to access)
            let _empty: &[u8] = &mmap;
        }
        Err(e) => {
            let msg = e.to_string();
            // Error must be descriptive
            assert!(
                msg.contains("mmap") || msg.contains("empty") || msg.contains("length"),
                "Error should indicate mmap/length issue: {msg}"
            );
        }
    }
}

/// CRITICAL: MmapBlock with 0 bytes must fail gracefully, not panic.
#[test]
fn mmap_block_zero_bytes_fails_gracefully() {
    let result = MmapBlock::new(0);

    assert!(
        result.is_err(),
        "MmapBlock::new(0) must return error, not succeed or panic"
    );

    let error = result.unwrap_err();
    let msg = error.to_string();
    assert!(
        msg.contains("null") || msg.contains("zero") || msg.contains("length"),
        "Error should mention null/zero/length: {msg}"
    );
}

/// CRITICAL: open_read_with_size with expected_size=0 must handle gracefully.
#[test]
fn mmap_open_read_with_size_zero_handles_gracefully() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty.txt");

    fs::File::create(&path).expect("create empty file");

    // Request 0 bytes from a 0-byte file
    let result = mmap::open_read_with_size(&path, 0);

    // Should either succeed with empty mmap or fail gracefully
    match result {
        Ok(mmap) => assert_eq!(mmap.len(), 0, "mmap must be empty"),
        Err(e) => {
            let msg = e.to_string();
            assert!(!msg.is_empty(), "Error must be descriptive, not empty");
        }
    }
}

// =============================================================================
// 2. FILES LARGER THAN AVAILABLE RAM
// =============================================================================

/// CRITICAL: mmap on file larger than RAM must work (OS handles paging).
/// We test with a moderately large file that won't actually OOM the test runner.
#[test]
fn mmap_large_file_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("large.bin");

    // Create a 10MB file - large enough to test the path but not OOM tests
    let size = 10 * 1024 * 1024;
    {
        let mut file = fs::File::create(&path).expect("create file");
        let chunk = vec![0xABu8; 65536];
        for _ in 0..(size / chunk.len()) {
            file.write_all(&chunk).expect("write chunk");
        }
        file.flush().expect("flush");
    }

    // Verify file size
    let metadata = fs::metadata(&path).expect("metadata");
    assert_eq!(metadata.len(), size as u64, "file size mismatch");

    // mmap must succeed - OS will handle paging
    let mmap = mmap::open_read(&path).expect("mmap of large file should succeed");
    assert_eq!(mmap.len(), size, "mmap size must match file size");

    // Verify first and last bytes are accessible
    assert_eq!(mmap[0], 0xAB, "first byte must match");
    assert_eq!(mmap[size - 1], 0xAB, "last byte must match");

    // Sample middle to ensure paging works
    assert_eq!(mmap[size / 2], 0xAB, "middle byte must match");
}

/// CRITICAL: open_read_with_size must verify size matches for large files.
#[test]
fn mmap_large_file_size_mismatch_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("large.bin");

    let actual_size = 5 * 1024 * 1024u64;
    {
        let mut file = fs::File::create(&path).expect("create file");
        let chunk = vec![0xCDu8; 65536];
        for _ in 0..(actual_size as usize / chunk.len()) {
            file.write_all(&chunk).expect("write chunk");
        }
    }

    // Request wrong size
    let wrong_size = actual_size + 1;
    let result = mmap::open_read_with_size(&path, wrong_size);

    let error = result.expect_err("size mismatch should fail");
    let msg = error.to_string();
    assert!(
        msg.contains("size mismatch") || msg.contains("mismatch"),
        "Error should indicate size mismatch: {msg}"
    );
}

// =============================================================================
// 3. PERMISSION DENIED TESTS
// =============================================================================

/// CRITICAL: mmap on file with no read permission must return error, not panic.
#[test]
#[cfg(unix)]
fn mmap_permission_denied_returns_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secret.txt");

    fs::write(&path, "secret content").expect("write file");

    // Remove all permissions
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&path, perms).expect("set permissions");

    // mmap must fail with error (not panic)
    let result = mmap::open_read(&path);

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o644);
    let _ = fs::set_permissions(&path, perms);

    assert!(result.is_err(), "mmap of unreadable file must fail");

    let error = result.unwrap_err();
    let msg = error.to_string();
    assert!(
        msg.contains("open") || msg.contains("permission") || msg.contains("denied"),
        "Error should indicate open/permission issue: {msg}"
    );
}

/// CRITICAL: mmap on directory must return error, not panic or succeed.
#[test]
fn mmap_directory_fails() {
    let dir = tempfile::tempdir().expect("tempdir");

    let result = mmap::open_read(dir.path());

    assert!(result.is_err(), "mmap of directory must fail");

    let error = result.unwrap_err();
    let msg = error.to_string();
    assert!(
        msg.contains("mmap") || msg.contains("open") || msg.contains("directory"),
        "Error should indicate mmap/open/directory issue: {msg}"
    );
}

/// CRITICAL: mmap on non-existent path must return error.
#[test]
fn mmap_nonexistent_file_fails() {
    let path = "/definitely/not/a/real/path/kernelkit_test_nonexistent";

    let result = mmap::open_read(path);

    assert!(result.is_err(), "mmap of nonexistent file must fail");

    let error = result.unwrap_err();
    let msg = error.to_string();
    assert!(
        msg.contains("open") || msg.contains("not found") || msg.contains("No such"),
        "Error should indicate open/not found: {msg}"
    );
}

// =============================================================================
// 4. SPECIAL FILE TESTS
// =============================================================================

/// CRITICAL: mmap on /dev/null must handle gracefully.
/// /dev/null is a character device that discards all input.
#[test]
#[cfg(unix)]
fn mmap_dev_null_fails_gracefully() {
    // /dev/null is a character device - mmap should fail
    let result = mmap::open_read("/dev/null");

    // Must either fail or return empty mmap
    match result {
        Ok(mmap) => {
            // If it succeeds, should be empty (device has no content)
            assert_eq!(mmap.len(), 0, "/dev/null mmap must be empty");
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("mmap") || msg.contains("invalid") || msg.contains("device"),
                "Error should indicate mmap/device issue: {msg}"
            );
        }
    }
}

/// CRITICAL: mmap on /dev/zero must handle gracefully.
/// /dev/zero provides infinite zeros but reports size 0 - mmap behavior is platform-specific.
#[test]
#[cfg(unix)]
fn mmap_dev_zero_handles_gracefully() {
    // First verify /dev/zero exists and is a character device
    let metadata = match fs::metadata("/dev/zero") {
        Ok(m) => m,
        Err(_) => {
            // Skip test if /dev/zero doesn't exist
            return;
        }
    };

    assert!(
        metadata.file_type().is_char_device(),
        "/dev/zero must be a character device"
    );

    // /dev/zero reports size 0 (infinite content)
    assert_eq!(metadata.len(), 0, "/dev/zero metadata should report size 0");

    // Attempt to mmap /dev/zero
    let result = mmap::open_read("/dev/zero");

    // /dev/zero behavior varies by platform:
    // - Linux: Usually fails with EINVAL (no size) or returns empty mmap
    // - Some BSDs: May succeed with some default size
    // We accept either outcome as long as it's safe
    match result {
        Ok(mmap) => {
            // If mmap succeeds with size 0, that's acceptable for a device with no size
            // The important thing is that it doesn't crash or return invalid pointer
            if mmap.is_empty() {
                // Empty mmap for 0-size device is acceptable
                return;
            }
            // If mmap has content, verify it's accessible and contains zeros
            assert!(
                !mmap.is_empty(),
                "/dev/zero mmap should have content if it succeeds"
            );
            // First byte should be zero
            assert_eq!(mmap[0], 0, "/dev/zero content must be zeros");
        }
        Err(e) => {
            let msg = e.to_string();
            // Should fail with descriptive error
            assert!(!msg.is_empty(), "Error must not be empty: {msg}");
        }
    }
}

/// CRITICAL: mmap on /proc/self/maps must handle gracefully.
/// This is a pseudo-file with changing content - should not crash.
#[test]
#[cfg(target_os = "linux")]
fn mmap_proc_self_maps_handles_gracefully() {
    let path = "/proc/self/maps";

    // Verify file exists
    if !Path::new(path).exists() {
        // Skip on systems without /proc
        return;
    }

    // Attempt to mmap
    let result = mmap::open_read(path);

    // /proc files may succeed or fail depending on kernel version
    match result {
        Ok(mmap) => {
            // If mmap succeeds, content should be readable
            // Content changes as memory map changes, so we just verify access
            let _len = mmap.len();
            if !mmap.is_empty() {
                let _first_byte = mmap[0]; // Should not panic
            }
        }
        Err(e) => {
            let msg = e.to_string();
            // Failure is acceptable with descriptive error
            assert!(!msg.is_empty(), "Error must be descriptive: {msg}");
        }
    }
}

/// CRITICAL: mmap on /proc/meminfo must handle gracefully.
#[test]
#[cfg(target_os = "linux")]
fn mmap_proc_meminfo_handles_gracefully() {
    let path = "/proc/meminfo";

    if !Path::new(path).exists() {
        return;
    }

    let result = mmap::open_read(path);

    match result {
        Ok(mmap) => {
            // Should be non-empty (contains memory info)
            assert!(!mmap.is_empty(), "/proc/meminfo should have content");
            // Should contain "MemTotal"
            let content = String::from_utf8_lossy(&mmap);
            assert!(
                content.contains("MemTotal"),
                "/proc/meminfo should contain MemTotal"
            );
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(!msg.is_empty(), "Error must be descriptive: {msg}");
        }
    }
}

/// CRITICAL: Attempting to mmap a block device should fail gracefully.
/// Testing with a loop device if available, otherwise skip.
#[test]
#[cfg(unix)]
fn mmap_block_device_fails_gracefully() {
    // Common block devices that may exist
    let block_devices = ["/dev/loop0", "/dev/loop1", "/dev/sda", "/dev/nvme0n1"];

    for dev in &block_devices {
        if !Path::new(dev).exists() {
            continue;
        }

        let metadata = fs::metadata(dev).expect("metadata");
        if !metadata.file_type().is_block_device() {
            continue;
        }

        // Found a block device - attempt mmap
        let result = mmap::open_read(dev);

        // Block device mmap should either:
        // 1. Fail (most common - no defined size)
        // 2. Succeed with the device's content
        match result {
            Ok(mmap) => {
                // If succeeds, should have some size
                let _ = mmap.len(); // Just verify access doesn't panic
            }
            Err(e) => {
                let msg = e.to_string();
                // Should fail with descriptive error
                assert!(!msg.is_empty(), "Error must be descriptive: {msg}");
            }
        }

        // Only test one device
        break;
    }
}

// =============================================================================
// 5. CONCURRENT ACCESS TESTS
// =============================================================================

/// CRITICAL: Concurrent mmap operations must be safe.
#[test]
fn mmap_concurrent_access_is_safe() {
    use std::sync::Arc;
    use std::thread;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("concurrent.bin");

    // Create test file
    fs::write(&path, b"concurrent test data").expect("write file");

    let path = Arc::new(path);
    let mut handles = vec![];

    // Spawn multiple threads that mmap the same file
    for i in 0..10 {
        let path = Arc::clone(&path);
        handles.push(thread::spawn(move || {
            let mmap = mmap::open_read(&*path).expect(&format!("thread {} mmap", i));
            assert_eq!(&mmap[..], b"concurrent test data");
        }));
    }

    for handle in handles {
        handle.join().expect("thread should complete");
    }
}

// =============================================================================
// 6. FILE TYPE VALIDATION TESTS
// =============================================================================

/// CRITICAL: MmapCorpus must reject symlinks to prevent directory traversal.
#[test]
fn mmap_corpus_rejects_symlinks() {
    use kernelkit::MmapCorpus;

    let dir = tempfile::tempdir().expect("tempdir");
    let real_file = dir.path().join("real.txt");
    let symlink = dir.path().join("link.txt");

    fs::write(&real_file, "real content").expect("write file");

    // Create symlink (may fail on Windows without permissions)
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_file, &symlink).expect("create symlink");

        // MmapCorpus should reject the directory with symlink
        let result = MmapCorpus::open(dir.path());

        // Should fail due to symlink
        assert!(result.is_err(), "MmapCorpus must reject symlinks");
    }
}

/// CRITICAL: MmapCorpus must handle files of various sizes correctly.
#[test]
fn mmap_corpus_mixed_file_sizes() {
    use kernelkit::MmapCorpus;

    let dir = tempfile::tempdir().expect("tempdir");

    // Create files of various sizes
    fs::write(dir.path().join("empty.txt"), b"").expect("write empty");
    fs::write(dir.path().join("tiny.txt"), b"x").expect("write tiny");
    fs::write(dir.path().join("small.txt"), vec![0u8; 1024]).expect("write small");
    fs::write(dir.path().join("medium.txt"), vec![0u8; 65536]).expect("write medium");

    let corpus = MmapCorpus::open(dir.path()).expect("open corpus");
    let files: Vec<_> = corpus.iter().collect();

    assert_eq!(files.len(), 4, "should have 4 files");

    // Verify sizes
    let mut found_sizes = std::collections::HashSet::new();
    for (_, content) in files {
        found_sizes.insert(content.len());
    }

    assert!(found_sizes.contains(&0), "should have empty file");
    assert!(found_sizes.contains(&1), "should have 1-byte file");
    assert!(found_sizes.contains(&1024), "should have 1KB file");
    assert!(found_sizes.contains(&65536), "should have 64KB file");
}

/// CRITICAL: MmapCorpus must enforce file size limits.
#[test]
fn mmap_corpus_enforces_size_limits() {
    use kernelkit::MmapCorpus;

    let dir = tempfile::tempdir().expect("tempdir");

    // Create a file slightly over 1MB limit
    let oversized = vec![0u8; 1024 * 1024 + 1];
    fs::write(dir.path().join("oversized.bin"), oversized).expect("write file");

    // Default limit is 1GB per file, 10GB total - this should succeed
    // But let's test with custom limits
    let result = MmapCorpus::open_with_limits(dir.path(), 1024 * 1024, 10 * 1024 * 1024 * 1024);

    assert!(result.is_err(), "should fail due to file size limit");
}
