//! Fault injection tests for kernelkit using faultkit.
//!
//! These tests verify that kernelkit handles IO failures gracefully
//! when mmap/read operations fail.

#![allow(clippy::single_match)]
#![allow(clippy::unwrap_used)]

use std::fs;
use tempfile::TempDir;

// Note: faultkit provides the injection framework but kernelkit's mmap
// functions use memmap2 directly, not through faultkit checkpoints yet.
// These tests verify error handling through natural failure paths instead.

#[test]
fn mmap_nonexistent_file_returns_error() {
    let result = kernelkit::mmap::open_read(std::path::Path::new("/nonexistent/path/xyz123"));
    assert!(result.is_err(), "mmap of nonexistent file should error");
}

#[test]
fn mmap_empty_file_succeeds() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.txt");
    fs::write(&path, "").unwrap();

    let result = kernelkit::mmap::open_read(&path);
    // Empty file mmap behavior varies by OS — either succeeds with empty mmap or errors
    // Both are acceptable
    match result {
        Ok(mmap) => assert!(mmap.is_empty(), "empty file mmap should be empty"),
        Err(_) => {} // Also acceptable
    }
}

#[test]
fn mmap_valid_file_returns_correct_content() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    let content = b"hello world test content";
    fs::write(&path, content).unwrap();

    let mmap = kernelkit::mmap::open_read(&path).unwrap();
    assert_eq!(&mmap[..], content, "mmap content should match file content");
}

#[test]
fn mmap_large_file_succeeds() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("large.bin");
    let content = vec![0xABu8; 10 * 1024 * 1024]; // 10MB
    fs::write(&path, &content).unwrap();

    let mmap = kernelkit::mmap::open_read(&path).unwrap();
    assert_eq!(
        mmap.len(),
        content.len(),
        "mmap length should match file size"
    );
    assert_eq!(mmap[0], 0xAB, "first byte should match");
    assert_eq!(mmap[mmap.len() - 1], 0xAB, "last byte should match");
}

#[cfg(unix)]
#[test]
fn mmap_permission_denied_returns_error() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("noperm.txt");
    fs::write(&path, "secret").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();

    let result = kernelkit::mmap::open_read(&path);
    assert!(
        result.is_err(),
        "mmap of permission-denied file should error"
    );

    // Restore permissions for cleanup
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn mmap_directory_returns_error() {
    let dir = TempDir::new().unwrap();
    let result = kernelkit::mmap::open_read(dir.path());
    assert!(result.is_err(), "mmap of directory should error");
}

// Faultkit integration: once kernelkit's mmap path is instrumented with
// faultkit::should_fail_mmap() checkpoints, these tests will verify
// graceful handling of injected mmap failures.
#[test]
fn faultkit_mmap_injection_smoke_test() {
    faultkit::clear();
    let _ = faultkit::inject(faultkit::Fault::Mmap { fail_after: 0 });
    assert!(
        faultkit::should_fail_mmap(),
        "injected mmap fault should fire"
    );
    faultkit::clear();
    assert!(
        !faultkit::should_fail_mmap(),
        "cleared fault should not fire"
    );
}
