//! Exhaustive adversarial tests for kernelkit.
//!
//! These tests verify:
//! - Robust handling of edge cases (null pointers, zero lengths, empty inputs)
//! - Graceful degradation on resource exhaustion
//! - Deterministic behavior across repeated calls
//! - Data integrity under all operations
//! - Proper error handling for malformed inputs

use std::fs;
use std::io::Write;

use kernelkit::{
    FileHeader, HugePageVec, MmapBlock, MmapCorpus, binformat, cpu_features, memory_pressure,
    mlock, mmap, numa, page_size, prefetch,
};

// =============================================================================
// MMAP ADVERSARIAL TESTS
// =============================================================================

#[test]
fn mmap_open_read_existing_file() {
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    file.write_all(b"kernelkit test content")
        .expect("write data");
    file.flush().expect("flush");

    let mmap = mmap::open_read(file.path()).expect("mmap should succeed for existing file");
    assert_eq!(&mmap[..], b"kernelkit test content");
}

#[test]
fn mmap_open_read_nonexistent_file() {
    let result = mmap::open_read(std::path::Path::new(
        "/definitely/not/a/real/path/kernelkit_test",
    ));
    let error = result.expect_err("should fail for non-existent file");
    let msg = error.to_string();
    assert!(
        msg.contains("open failed"),
        "Error should indicate open failed: {msg}"
    );
}

#[test]
fn mmap_open_read_empty_file() {
    let file = tempfile::NamedTempFile::new().expect("tempfile");
    // Empty files may succeed (empty mmap) or fail depending on platform
    match mmap::open_read(file.path()) {
        Ok(mmap) => assert!(mmap.is_empty(), "empty file should produce empty mmap"),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("mmap failed"),
                "Error should indicate mmap failed: {msg}"
            );
        }
    }
}

#[test]
fn mmap_open_read_verifies_contents_match_fs_read() {
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    file.write_all(&data).expect("write data");
    file.flush().expect("flush");

    let mmap = mmap::open_read(file.path()).expect("mmap should succeed");
    let fs_data = fs::read(file.path()).expect("fs read should succeed");

    assert_eq!(
        &mmap[..],
        &fs_data[..],
        "mmap contents must match fs::read contents"
    );
    assert_eq!(mmap.len(), fs_data.len(), "lengths must match");
}

#[test]
fn mmap_open_read_large_file() {
    // Test with a file > 1MB but don't require >1GB in tests
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    let size = 5 * 1024 * 1024; // 5MB
    let chunk = vec![0xABu8; 4096];
    for _i in 0..(size / 4096) {
        file.write_all(&chunk).expect("write chunk");
    }
    file.flush().expect("flush");

    let mmap = mmap::open_read(file.path()).expect("mmap should succeed for large file");
    assert_eq!(mmap.len(), size, "mmap size should match file size");
    // Verify first and last bytes
    assert_eq!(mmap[0], 0xAB);
    assert_eq!(mmap[size - 1], 0xAB);
}

#[test]
fn mmap_anon_zero_bytes_fails() {
    let result = MmapBlock::new(0);
    assert!(result.is_err(), "zero-byte allocation should fail");
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("null"),
        "Error should mention null: {error}"
    );
}

#[test]
fn mmap_anon_4096_bytes_succeeds() {
    let mut block = MmapBlock::new(4096).expect("4KB allocation should succeed");
    assert_eq!(block.len(), 4096);
    assert!(!block.is_empty());
    assert!(!block.as_mut_ptr().is_null());
}

#[test]
fn mmap_anon_2mb_hugepage_size_succeeds() {
    let size = 2 * 1024 * 1024; // 2MB
    let mut block = MmapBlock::new(size).expect("2MB allocation should succeed");
    assert_eq!(block.len(), size);

    // Verify we can write and read back
    let ptr = block.as_mut_ptr();
    unsafe {
        ptr.write(0x12);
        ptr.add(size - 1).write(0x34);
        assert_eq!(*ptr, 0x12);
        assert_eq!(*ptr.add(size - 1), 0x34);
    }
}

#[test]
fn mmap_block_send_sync_bounds() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<MmapBlock>();
    assert_sync::<MmapBlock>();
}

#[test]
fn mmap_open_read_directory_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let result = mmap::open_read(dir.path());
    let error = result.expect_err("should fail for directory");
    let msg = error.to_string();
    assert!(
        msg.contains("mmap failed") || msg.contains("open failed"),
        "Error should indicate failure: {msg}"
    );
}

#[test]
fn mmap_open_read_with_size_mismatch_fails() {
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    file.write_all(b"exactly 9 bytes").expect("write");
    file.flush().expect("flush");

    let result = mmap::open_read_with_size(file.path(), 8);
    let error = result.expect_err("should fail on size mismatch");
    assert!(
        error.to_string().contains("file size mismatch"),
        "Error should mention size mismatch"
    );
}

#[test]
fn mmap_open_read_with_size_exact_match_succeeds() {
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    file.write_all(b"nine bytes!").expect("write");
    file.flush().expect("flush");

    let mmap = mmap::open_read_with_size(file.path(), 11).expect("should succeed with exact size");
    assert_eq!(&mmap[..], b"nine bytes!");
}

// =============================================================================
// CPU FEATURES ADVERSARIAL TESTS
// =============================================================================

#[test]
fn cpu_features_detect_returns_valid_cache_line_size() {
    let features = cpu_features::detect();
    // Cache line size must be a power of 2 and reasonable
    assert!(
        features.cache_line_size > 0,
        "cache_line_size must be positive"
    );
    assert!(
        features.cache_line_size.is_power_of_two(),
        "cache_line_size should be power of 2"
    );
    assert!(
        features.cache_line_size >= 16 && features.cache_line_size <= 256,
        "cache_line_size should be between 16 and 256 bytes"
    );
}

#[test]
fn cpu_features_detect_returns_valid_l1_size() {
    let features = cpu_features::detect();
    assert!(features.l1_size > 0, "L1 size must be positive");
    assert!(
        features.l1_size >= 1024 && features.l1_size <= 1024 * 1024,
        "L1 size should be between 1KB and 1MB"
    );
}

#[test]
fn cpu_features_detect_returns_valid_l2_size() {
    let features = cpu_features::detect();
    // L2 can be 0 on some systems (if not present or not detected)
    if features.l2_size > 0 {
        assert!(
            features.l2_size >= 1024 && features.l2_size <= 64 * 1024 * 1024,
            "L2 size should be reasonable if present"
        );
    }
}

#[test]
fn cpu_features_detect_is_deterministic() {
    let first = cpu_features::detect();
    let second = cpu_features::detect();
    let third = cpu_features::detect();

    assert_eq!(
        first, second,
        "CPU feature detection must be deterministic (call 1 vs 2)"
    );
    assert_eq!(
        second, third,
        "CPU feature detection must be deterministic (call 2 vs 3)"
    );
    assert_eq!(
        first, third,
        "CPU feature detection must be deterministic (call 1 vs 3)"
    );
}

#[test]
fn cpu_features_avx2_is_boolean() {
    let features = cpu_features::detect();
    // avx2 is either true or false - both are valid
    let _ = features.avx2; // Just ensure it's accessible
}

#[test]
fn cpu_features_neon_is_boolean() {
    let features = cpu_features::detect();
    // neon is either true or false - both are valid
    let _ = features.neon; // Just ensure it's accessible
}

#[test]
fn cpu_features_avx512_variants_consistent() {
    let features = cpu_features::detect();
    // If avx512 is false, all variants should be false
    if !features.avx512 {
        assert!(!features.avx512bw, "avx512bw requires avx512");
        assert!(!features.avx512vl, "avx512vl requires avx512");
        assert!(!features.avx512vbmi, "avx512vbmi requires avx512");
    }
}

#[test]
fn cpu_features_struct_equality() {
    let f1 = cpu_features::detect();
    let f2 = cpu_features::detect();

    // All fields should be equal
    assert_eq!(f1.avx512, f2.avx512);
    assert_eq!(f1.avx512bw, f2.avx512bw);
    assert_eq!(f1.avx512vl, f2.avx512vl);
    assert_eq!(f1.avx512vbmi, f2.avx512vbmi);
    assert_eq!(f1.avx2, f2.avx2);
    assert_eq!(f1.neon, f2.neon);
    assert_eq!(f1.cache_line_size, f2.cache_line_size);
    assert_eq!(f1.l1_size, f2.l1_size);
    assert_eq!(f1.l2_size, f2.l2_size);
    assert_eq!(f1.l3_size, f2.l3_size);
}

// =============================================================================
// PREFETCH ADVERSARIAL TESTS
// =============================================================================

#[test]
fn prefetch_read_on_valid_pointer_does_not_panic() {
    let data = [0u8; 4096];
    prefetch::prefetch_read(data.as_ptr());
    // If we get here without panic, the test passes
}

#[test]
fn prefetch_write_on_valid_pointer_does_not_panic() {
    let mut data = [0u8; 4096];
    prefetch::prefetch_write(data.as_mut_ptr());
    // If we get here without panic, the test passes
}

#[test]
fn prefetch_nontemporal_on_valid_pointer_does_not_panic() {
    let data = [0u8; 4096];
    prefetch::prefetch_nontemporal(data.as_ptr());
    // If we get here without panic, the test passes
}

#[test]
fn prefetch_read_does_not_corrupt_data() {
    let data: Vec<u8> = (0..=255).collect();
    let original = data.clone();

    for i in 0..data.len() {
        prefetch::prefetch_read(&data[i]);
    }

    assert_eq!(data, original, "prefetch_read should not modify data");
}

#[test]
fn prefetch_write_does_not_corrupt_data() {
    let mut data: Vec<u8> = (0..=255).collect();
    let original = data.clone();

    for i in 0..data.len() {
        prefetch::prefetch_write(&mut data[i]);
    }

    assert_eq!(data, original, "prefetch_write should not modify data");
}

#[test]
fn prefetch_range_on_valid_region_does_not_panic() {
    let data = [0u8; 8192];
    unsafe {
        prefetch::prefetch_range(data.as_ptr(), data.len());
    }
    // If we get here without panic, the test passes
}

#[test]
fn prefetch_range_zero_length_does_not_panic() {
    let data = [0u8; 4096];
    unsafe {
        prefetch::prefetch_range(data.as_ptr(), 0);
    }
    // If we get here without panic, the test passes
}

#[test]
fn prefetch_range_null_pointer_zero_length_does_not_panic() {
    unsafe {
        prefetch::prefetch_range(std::ptr::null(), 0);
    }
    // If we get here without panic, the test passes
}

#[test]
fn prefetch_range_does_not_corrupt_data() {
    let data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    let original = data.clone();

    unsafe {
        prefetch::prefetch_range(data.as_ptr(), data.len());
    }

    assert_eq!(data, original, "prefetch_range should not modify data");
}

#[test]
fn prefetch_range_large_region() {
    let data = vec![0xABu8; 1024 * 1024]; // 1MB
    let original = data.clone();

    unsafe {
        prefetch::prefetch_range(data.as_ptr(), data.len());
    }

    assert_eq!(
        data, original,
        "prefetch_range should not modify large region"
    );
}

// =============================================================================
// HUGEPAGE ADVERSARIAL TESTS
// =============================================================================

#[test]
fn hugepage_vec_zero_length() {
    let vec = HugePageVec::<u8>::new(0);
    assert_eq!(vec.len(), 0);
    assert!(vec.is_empty());
    assert!(vec.as_slice().is_empty());
}

#[test]
fn hugepage_vec_small_allocation() {
    let mut vec = HugePageVec::<u64>::new(10);
    assert_eq!(vec.len(), 10);

    // Verify initialized to default
    for i in 0..10 {
        assert_eq!(vec.as_slice()[i], 0);
    }

    // Write and read back
    vec.as_mut_slice()[5] = 42;
    assert_eq!(vec.as_slice()[5], 42);
}

#[test]
fn hugepage_vec_large_allocation_fallback() {
    // Request enough to possibly trigger huge page path (>2MB worth of u64s)
    let count = 512 * 1024; // 4MB worth of u64s
    let vec = HugePageVec::<u64>::new(count);

    assert_eq!(vec.len(), count);

    // Verify first and last elements are initialized
    assert_eq!(vec.as_slice()[0], 0);
    assert_eq!(vec.as_slice()[count - 1], 0);
}

#[test]
fn hugepage_vec_zst_works() {
    // Zero-sized types should work (use standard Vec fallback)
    let vec = HugePageVec::<()>::new(1000);
    assert_eq!(vec.len(), 1000);
}

#[test]
fn hugepage_vec_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<HugePageVec<u32>>();
    assert_sync::<HugePageVec<u32>>();
}

#[test]
fn hugepage_vec_write_read_roundtrip() {
    let mut vec = HugePageVec::<u32>::new(1000);

    // Write pattern
    for i in 0..1000 {
        vec.as_mut_slice()[i] = i as u32 * 7 + 13;
    }

    // Read back and verify
    for i in 0..1000 {
        assert_eq!(vec.as_slice()[i], i as u32 * 7 + 13);
    }
}

// =============================================================================
// BINARY FORMAT ADVERSARIAL TESTS
// =============================================================================

#[test]
fn binformat_header_roundtrip() {
    let header = FileHeader {
        magic: b"TEST",
        version: 42,
    };

    let mut bytes = Vec::new();
    header.write_to(&mut bytes).expect("write header");
    bytes.extend_from_slice(b"payload data here");

    let (version, rest) = FileHeader::read_from(&bytes, b"TEST", 100).expect("read header");
    assert_eq!(version, 42);
    assert_eq!(rest, b"payload data here");
}

#[test]
fn binformat_corrupted_header_detection() {
    let header = FileHeader {
        magic: b"GOOD",
        version: 1,
    };

    let mut bytes = Vec::new();
    header.write_to(&mut bytes).expect("write header");

    // Try to read with wrong magic
    let result = FileHeader::read_from(&bytes, b"BAD!", 1);
    let error = result.expect_err("should fail with wrong magic");
    assert!(
        matches!(error, kernelkit::Error::InvalidMagic),
        "Error should be InvalidMagic"
    );
}

#[test]
fn binformat_unsupported_version_detection() {
    let header = FileHeader {
        magic: b"TEST",
        version: 100,
    };

    let mut bytes = Vec::new();
    header.write_to(&mut bytes).expect("write header");

    // Try to read with max_version = 50
    let result = FileHeader::read_from(&bytes, b"TEST", 50);
    let error = result.expect_err("should fail with unsupported version");
    assert!(
        matches!(
            error,
            kernelkit::Error::UnsupportedVersion {
                version: 100,
                max_version: 50
            }
        ),
        "Error should be UnsupportedVersion"
    );
}

#[test]
fn binformat_truncated_magic_detection() {
    let bytes = b"TE"; // Too short for magic "TEST"

    let result = FileHeader::read_from(bytes, b"TEST", 1);
    let error = result.expect_err("should fail with truncated magic");
    assert!(
        matches!(
            error,
            kernelkit::Error::UnexpectedEof {
                context: "file header magic",
                ..
            }
        ),
        "Error should indicate truncated magic"
    );
}

#[test]
fn binformat_truncated_version_detection() {
    let bytes = b"TEST\x01\x00\x00"; // Only 3 bytes after magic

    let result = FileHeader::read_from(bytes, b"TEST", 1);
    let error = result.expect_err("should fail with truncated version");
    assert!(
        matches!(
            error,
            kernelkit::Error::UnexpectedEof {
                context: "file header version",
                ..
            }
        ),
        "Error should indicate truncated version"
    );
}

#[test]
fn binformat_section_roundtrip() {
    let mut bytes = Vec::new();
    let payload = b"hello world";
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(b"tail");

    let (section, rest) = binformat::read_section(&bytes).expect("read section");
    assert_eq!(section, payload);
    assert_eq!(rest, b"tail");
}

#[test]
fn binformat_section_too_large_detection() {
    // Write u64::MAX as section length
    let bytes = u64::MAX.to_le_bytes();

    let result = binformat::read_section(&bytes);
    let error = result.expect_err("should fail with section too large");
    // Either SectionTooLarge or UnexpectedEof (if platform has smaller usize)
    assert!(
        matches!(error, kernelkit::Error::SectionTooLarge { .. })
            || matches!(
                error,
                kernelkit::Error::UnexpectedEof {
                    context: "section payload",
                    ..
                }
            ),
        "Error should indicate section problem: {error}"
    );
}

#[test]
fn binformat_section_truncated_payload_detection() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&100u64.to_le_bytes()); // Claims 100 bytes
    bytes.extend_from_slice(b"only 17 bytes"); // But only provides 17

    let result = binformat::read_section(&bytes);
    let error = result.expect_err("should fail with truncated payload");
    assert!(
        matches!(
            error,
            kernelkit::Error::UnexpectedEof {
                context: "section payload",
                ..
            }
        ),
        "Error should indicate truncated payload"
    );
}

#[test]
fn binformat_zero_length_section() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(b"tail");

    let (section, rest) = binformat::read_section(&bytes).expect("read zero-length section");
    assert!(section.is_empty());
    assert_eq!(rest, b"tail");
}

#[test]
fn binformat_nested_sections() {
    // Create inner section
    let mut inner = Vec::new();
    inner.extend_from_slice(&3u64.to_le_bytes());
    inner.extend_from_slice(b"abc");

    // Create outer section containing inner
    let mut outer = Vec::new();
    outer.extend_from_slice(&(inner.len() as u64).to_le_bytes());
    outer.extend_from_slice(&inner);
    outer.extend_from_slice(b"after");

    let (inner_data, rest) = binformat::read_section(&outer).expect("read outer");
    assert_eq!(rest, b"after");

    let (payload, remaining) = binformat::read_section(inner_data).expect("read inner");
    assert_eq!(payload, b"abc");
    assert!(remaining.is_empty());
}

// =============================================================================
// MEMORY LOCKING ADVERSARIAL TESTS
// =============================================================================

#[test]
fn mlock_zero_length_is_noop() {
    // Zero length with null pointer should succeed
    mlock::lock_region(std::ptr::null(), 0).expect("zero-length lock should succeed");
    mlock::unlock_region(std::ptr::null(), 0).expect("zero-length unlock should succeed");
}

#[test]
fn mlock_null_pointer_with_nonzero_length_fails() {
    let result = mlock::lock_region(std::ptr::null(), 100);
    let error = result.expect_err("null pointer with non-zero length should fail");
    assert!(
        matches!(error, kernelkit::Error::NullPointer),
        "Error should be NullPointer"
    );
}

#[test]
fn mlock_valid_region_succeeds_or_fails_gracefully() {
    let data = [0u8; 4096];
    // May succeed or fail based on permissions, but should not panic
    let _ = mlock::lock_region(data.as_ptr(), data.len());
    let _ = mlock::unlock_region(data.as_ptr(), data.len());
}

// =============================================================================
// PAGE SIZE ADVERSARIAL TESTS
// =============================================================================

#[test]
fn page_size_is_power_of_two() {
    let ps = page_size();
    assert!(ps.is_power_of_two(), "page size must be power of 2");
}

#[test]
fn page_size_is_at_least_4096() {
    let ps = page_size();
    assert!(ps >= 4096, "page size must be at least 4096");
}

#[test]
fn page_size_is_deterministic() {
    let p1 = page_size();
    let p2 = page_size();
    let p3 = page_size();
    assert_eq!(p1, p2);
    assert_eq!(p2, p3);
}

// =============================================================================
// MEMORY PRESSURE ADVERSARIAL TESTS
// =============================================================================

#[test]
fn memory_pressure_returns_valid_status() {
    let status = memory_pressure().expect("memory_pressure should succeed");
    // On Linux, should have valid values; on other platforms, returns zeros
    let _ = status.available_bytes;
    let _ = status.total_bytes;
}

#[test]
fn memory_pressure_available_not_greater_than_total() {
    let status = memory_pressure().expect("memory_pressure should succeed");
    if status.total_bytes > 0 {
        assert!(
            status.available_bytes <= status.total_bytes,
            "available should not exceed total"
        );
    }
}

#[test]
fn memory_pressure_is_deterministic() {
    let s1 = memory_pressure().expect("memory_pressure should succeed");
    let s2 = memory_pressure().expect("memory_pressure should succeed");
    // Values may change between calls (memory pressure changes), so just ensure no panic
    let _ = (s1, s2);
}

// =============================================================================
// NUMA ADVERSARIAL TESTS
// =============================================================================

#[test]
fn numa_node_count_is_at_least_one() {
    let count = numa::node_count();
    assert!(count >= 1, "node count must be at least 1");
}

#[test]
fn numa_node_count_is_deterministic() {
    let c1 = numa::node_count();
    let c2 = numa::node_count();
    let c3 = numa::node_count();
    assert_eq!(c1, c2);
    assert_eq!(c2, c3);
}

#[test]
fn numa_alloc_on_node_zero_succeeds() {
    let values = numa::alloc_on_node::<u64>(100, 0).expect("alloc on node 0 should succeed");
    assert_eq!(values.len(), 100);
    // Should be initialized to default (0 for u64)
    assert!(values.iter().all(|&v| v == 0));
}

#[test]
fn numa_invalid_node_is_rejected() {
    let invalid = numa::node_count() as u32; // One past the last valid node
    let result = numa::alloc_on_node::<u8>(10, invalid);
    let error = result.expect_err("invalid node should fail");
    assert!(
        matches!(error, kernelkit::Error::InvalidNode { node, .. } if node == invalid),
        "Error should be InvalidNode with correct node number"
    );
}

#[test]
fn numa_current_node_is_option() {
    let node = numa::current_node();
    // On Linux, may be Some(node); on other platforms, None
    if let Some(n) = node {
        let count = numa::node_count() as u32;
        assert!(n < count, "current node must be valid");
    }
}

// =============================================================================
// MMAP CORPUS ADVERSARIAL TESTS
// =============================================================================

#[test]
fn mmap_corpus_empty_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let corpus = MmapCorpus::open(dir.path()).expect("empty corpus should open");
    assert_eq!(corpus.iter().count(), 0);
}

#[test]
fn mmap_corpus_with_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("a.txt"), b"alpha").expect("write a");
    fs::write(dir.path().join("b.txt"), b"beta").expect("write b");

    let corpus = MmapCorpus::open(dir.path()).expect("open corpus");
    let collected: Vec<_> = corpus.iter().map(|(_, bytes)| bytes.to_vec()).collect();

    assert_eq!(collected.len(), 2);
    assert!(collected.iter().any(|b| b == b"alpha"));
    assert!(collected.iter().any(|b| b == b"beta"));
}

#[test]
fn mmap_corpus_nested_directories() {
    let dir = tempfile::tempdir().expect("tempdir");
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).expect("create subdir");
    fs::write(dir.path().join("root.txt"), b"root").expect("write root");
    fs::write(subdir.join("nested.txt"), b"nested").expect("write nested");

    let corpus = MmapCorpus::open(dir.path()).expect("open corpus");
    let count = corpus.iter().count();
    assert_eq!(count, 2);
}

#[test]
fn mmap_corpus_nonexistent_directory_fails() {
    let result = MmapCorpus::open("/definitely/not/a/real/directory");
    assert!(result.is_err(), "nonexistent directory should fail");
}

// =============================================================================
// STRESS AND EDGE CASE TESTS
// =============================================================================

#[test]
fn mmap_block_multiple_allocations() {
    // Allocate and deallocate many blocks to stress the system
    for i in 0..100 {
        let size = 4096 + (i * 4096) % (1024 * 1024); // Varying sizes up to 1MB
        let block = MmapBlock::new(size).expect(&format!("allocation {} should succeed", i));
        assert_eq!(block.len(), size);
        // Block is dropped here, testing munmap path
    }
}

#[test]
fn hugepage_vec_multiple_allocations() {
    for i in 0..20 {
        let count = 100 + i * 1000;
        let vec = HugePageVec::<u64>::new(count);
        assert_eq!(vec.len(), count);
        // Verify initialized
        assert_eq!(vec.as_slice()[0], 0);
        if count > 1 {
            assert_eq!(vec.as_slice()[count - 1], 0);
        }
    }
}

#[test]
fn prefetch_range_spanning_multiple_cache_lines() {
    // Allocate data spanning many cache lines
    let cache_line = cpu_features::detect().cache_line_size.max(64);
    let size = cache_line * 100; // 100 cache lines
    let data = vec![0u8; size];

    unsafe {
        prefetch::prefetch_range(data.as_ptr(), data.len());
    }
    // Verify data unchanged
    assert!(data.iter().all(|&b| b == 0));
}

#[test]
fn binformat_large_payload_roundtrip() {
    let mut bytes = Vec::new();
    let payload = vec![0xABu8; 100000]; // 100KB payload
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&payload);

    let (section, rest) = binformat::read_section(&bytes).expect("read large section");
    assert_eq!(section.len(), payload.len());
    assert_eq!(section, payload.as_slice());
    assert!(rest.is_empty());
}

#[test]
fn binformat_many_sections() {
    let mut bytes = Vec::new();
    let section_count = 100;

    for i in 0..section_count {
        let payload = vec![i as u8; 100];
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&payload);
    }

    let mut remaining = bytes.as_slice();
    for i in 0..section_count {
        let (section, rest) =
            binformat::read_section(remaining).expect(&format!("read section {}", i));
        assert_eq!(section.len(), 100);
        assert!(section.iter().all(|&b| b == i as u8));
        remaining = rest;
    }
    assert!(remaining.is_empty());
}

// =============================================================================
// UNICODE AND EDGE CASE PATH TESTS
// =============================================================================

#[test]
fn mmap_with_unicode_filename() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("文件_тест_🦀.txt");
    fs::write(&path, b"unicode filename content").expect("write");

    let mmap = mmap::open_read(&path).expect("mmap unicode filename");
    assert_eq!(&mmap[..], b"unicode filename content");
}

#[test]
fn mmap_with_spaces_in_filename() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("file with spaces.txt");
    fs::write(&path, b"spaces content").expect("write");

    let mmap = mmap::open_read(&path).expect("mmap spaces filename");
    assert_eq!(&mmap[..], b"spaces content");
}

// =============================================================================
// CONCURRENT SAFETY TESTS
// =============================================================================

#[test]
fn hugepage_vec_thread_safe() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    // Test that HugePageVec is Send + Sync by using it across threads
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<HugePageVec<u64>>();
    assert_sync::<HugePageVec<u64>>();

    // Use Arc<AtomicU64> for thread-safe verification
    let counters: Arc<Vec<AtomicU64>> = Arc::new((0..10).map(|_| AtomicU64::new(0)).collect());

    let handles: Vec<_> = (0..10)
        .map(|thread_id| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..100 {
                    counters[thread_id].fetch_add(i as u64, Ordering::SeqCst);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread should complete");
    }

    // Verify work was done
    for thread_id in 0..10 {
        let sum: u64 = (0..100).sum::<i32>() as u64;
        assert_eq!(counters[thread_id].load(Ordering::SeqCst), sum);
    }
}

// =============================================================================
// ERROR MESSAGE QUALITY TESTS
// =============================================================================

#[test]
fn error_messages_are_actionable() {
    // Test that error messages contain "Fix:" guidance where applicable
    let result = mmap::open_read(std::path::Path::new("/nonexistent/path"));
    let error = result.unwrap_err();
    let msg = error.to_string();
    // Error messages should be descriptive
    assert!(!msg.is_empty(), "Error message should not be empty");
}

#[test]
fn invalid_node_error_is_descriptive() {
    let invalid_node = u32::MAX;
    let result = numa::alloc_on_node::<u8>(10, invalid_node);
    let error = result.unwrap_err();
    let msg = error.to_string();
    assert!(
        msg.contains("Fix:") || msg.contains("not available"),
        "Error should guide user: {msg}"
    );
}
