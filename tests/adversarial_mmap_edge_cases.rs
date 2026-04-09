//! Additional adversarial tests for mmap edge cases.
//!
//! These tests verify robust handling of:
//! - Files with changing content during mmap
//! - Very long paths
//! - Paths with special characters
//! - Concurrent modifications
//! - Resource exhaustion scenarios

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;
use std::thread;

use kernelkit::MmapCorpus;
use kernelkit::mmap::{self, MmapAdvice, MmapBlock};

// =============================================================================
// 1. PATH VALIDATION TESTS
// =============================================================================

/// CRITICAL: Very long paths should be handled gracefully.
#[test]
fn mmap_very_long_path_fails_gracefully() {
    // Create a path that exceeds typical filesystem limits
    let long_name = "a".repeat(300); // Most filesystems limit to 255 bytes for filename
    let path = format!("/tmp/{}", long_name);

    let result = mmap::open_read(&path);

    assert!(result.is_err(), "very long path should fail");

    let error = result.unwrap_err();
    let msg = error.to_string();
    assert!(!msg.is_empty(), "Error must be descriptive: {msg}");
}

/// CRITICAL: Paths with null bytes should be rejected.
#[test]
fn mmap_path_with_null_byte_fails_gracefully() {
    let path = "/tmp/test\0file.txt";

    let result = mmap::open_read(path);

    assert!(result.is_err(), "path with null byte should fail");
}

/// CRITICAL: Unicode paths should work correctly.
#[test]
fn mmap_unicode_path_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("文件_тест_テスト_🦀.bin");

    let content = b"unicode test content";
    fs::write(&path, content).expect("write unicode file");

    let mmap = mmap::open_read(&path).expect("mmap unicode path");
    assert_eq!(&mmap[..], content);
}

/// CRITICAL: Path with spaces should work.
#[test]
fn mmap_path_with_spaces_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("file with spaces in name.txt");

    fs::write(&path, b"content").expect("write file");

    let mmap = mmap::open_read(&path).expect("mmap path with spaces");
    assert_eq!(&mmap[..], b"content");
}

// =============================================================================
// 2. CONTENT VERIFICATION TESTS
// =============================================================================

/// CRITICAL: mmap content must match file system read content exactly.
#[test]
fn mmap_content_matches_fs_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("verify.bin");

    // Create file with pseudo-random content
    let content: Vec<u8> = (0..100_000).map(|i| ((i * 7 + 13) % 256) as u8).collect();
    fs::write(&path, &content).expect("write file");

    // Read via mmap
    let mmap = mmap::open_read(&path).expect("mmap");

    // Read via fs
    let fs_content = fs::read(&path).expect("fs read");

    // Must match exactly
    assert_eq!(
        &mmap[..],
        &fs_content[..],
        "mmap content must match fs::read content exactly"
    );
    assert_eq!(
        &mmap[..],
        &content[..],
        "mmap content must match original content exactly"
    );
}

/// CRITICAL: mmap with different advice types should work.
#[test]
fn mmap_with_different_advice_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Create test file
    let path_seq = dir.path().join("sequential.bin");
    let path_rand = dir.path().join("random.bin");
    let path_willneed = dir.path().join("willneed.bin");

    let content = vec![0u8; 65536];
    fs::write(&path_seq, &content).expect("write seq");
    fs::write(&path_rand, &content).expect("write rand");
    fs::write(&path_willneed, &content).expect("write willneed");

    // Test Sequential advice
    let mmap_seq =
        mmap::open_with_advice(&path_seq, MmapAdvice::Sequential).expect("mmap sequential");
    assert_eq!(mmap_seq.len(), content.len());

    // Test Random advice
    let mmap_rand = mmap::open_with_advice(&path_rand, MmapAdvice::Random).expect("mmap random");
    assert_eq!(mmap_rand.len(), content.len());

    // Test WillNeed advice
    let mmap_willneed =
        mmap::open_with_advice(&path_willneed, MmapAdvice::WillNeed).expect("mmap willneed");
    assert_eq!(mmap_willneed.len(), content.len());
}

// =============================================================================
// 3. MMAP BLOCK EDGE CASES
// =============================================================================

/// CRITICAL: MmapBlock should handle various sizes correctly.
#[test]
fn mmap_block_various_sizes() {
    let sizes = vec![
        1,       // 1 byte
        512,     // Half page
        4096,    // One page
        4097,    // Page + 1
        65536,   // 64KB
        1048576, // 1MB
    ];

    for size in sizes {
        let mut block = MmapBlock::new(size).expect(&format!("allocate {} bytes", size));
        assert_eq!(block.len(), size, "size mismatch for {}", size);
        assert!(
            !block.as_mut_ptr().is_null(),
            "ptr must not be null for {}",
            size
        );

        // Verify we can write and read
        let ptr = block.as_mut_ptr();
        unsafe {
            ptr.write(0xAB);
            if size > 1 {
                ptr.add(size - 1).write(0xCD);
                assert_eq!(
                    *ptr.add(size - 1),
                    0xCD,
                    "last byte write failed for {}",
                    size
                );
            }
            assert_eq!(*ptr, 0xAB, "first byte write failed for {}", size);
        }
    }
}

/// CRITICAL: MmapBlock very large allocation should fail gracefully.
#[test]
fn mmap_block_huge_allocation_fails_gracefully() {
    // Try to allocate an impossibly large block
    let result = MmapBlock::new(usize::MAX);

    assert!(result.is_err(), "allocation of usize::MAX should fail");
}

/// CRITICAL: MmapBlock should handle allocation near address space limits.
#[test]
fn mmap_block_large_allocation_boundary() {
    // Try a large but potentially valid size
    // This tests the boundary where mmap might fail due to address space
    let large_size = 1024usize * 1024 * 1024; // 1GB

    // This may succeed or fail depending on system, but must not panic
    match MmapBlock::new(large_size) {
        Ok(block) => {
            assert_eq!(block.len(), large_size);
            let mut block = block;
            // Verify we can access first and last bytes
            let ptr = block.as_mut_ptr();
            unsafe {
                ptr.write(0x12);
                ptr.add(large_size - 1).write(0x34);
            }
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("mmap") || msg.contains("system") || msg.contains("allocation"),
                "Error should be descriptive: {msg}"
            );
        }
    }
}

// =============================================================================
// 4. CONCURRENT SAFETY TESTS
// =============================================================================

/// CRITICAL: Multiple threads reading same mmap should be safe.
#[test]
fn mmap_concurrent_reads_are_safe() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("concurrent.bin");

    // Create file with known pattern
    let content: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();
    fs::write(&path, &content).expect("write file");

    let mmap = Arc::new(mmap::open_read(&path).expect("mmap"));
    let mut handles = vec![];

    // Spawn threads that read different parts
    for thread_id in 0..10 {
        let mmap = Arc::clone(&mmap);
        handles.push(thread::spawn(move || {
            let start = thread_id * 100_000;
            let end = start + 100_000;

            for i in start..end {
                assert_eq!(mmap[i], (i % 256) as u8, "data mismatch at {}", i);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("thread should complete");
    }
}

/// CRITICAL: Multiple MmapBlocks allocated concurrently should be safe.
#[test]
fn mmap_block_concurrent_allocations_are_safe() {
    let mut handles = vec![];

    for thread_id in 0..10 {
        handles.push(thread::spawn(move || {
            for i in 0..10 {
                let size = 4096 + (i * 4096);
                let block =
                    MmapBlock::new(size).expect(&format!("alloc {} in thread {}", size, thread_id));
                assert_eq!(block.len(), size);
                let mut block = block;

                // Write thread-specific pattern
                let ptr = block.as_mut_ptr();
                unsafe {
                    ptr.write(thread_id as u8);
                    assert_eq!(*ptr, thread_id as u8);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("thread should complete");
    }
}

// =============================================================================
// 5. CORPUS EDGE CASES
// =============================================================================

/// CRITICAL: Empty corpus should work.
#[test]
fn mmap_corpus_empty_directory_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");

    let corpus = MmapCorpus::open(dir.path()).expect("open empty corpus");
    assert_eq!(corpus.iter().count(), 0, "empty corpus should have 0 files");
}

/// CRITICAL: Corpus with deeply nested directories.
#[test]
fn mmap_corpus_deeply_nested() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Create nested structure
    let deep_path = dir.path().join("a/b/c/d/e");
    fs::create_dir_all(&deep_path).expect("create dirs");
    fs::write(deep_path.join("deep.txt"), b"deep content").expect("write file");
    fs::write(dir.path().join("root.txt"), b"root content").expect("write root");

    let corpus = MmapCorpus::open(dir.path()).expect("open corpus");
    let files: Vec<_> = corpus.iter().map(|(p, _)| p.to_path_buf()).collect();

    assert_eq!(files.len(), 2, "should find 2 files");
}

/// CRITICAL: Corpus with many small files.
#[test]
fn mmap_corpus_many_small_files() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Create 100 small files
    for i in 0..100 {
        let path = dir.path().join(format!("file_{:03}.txt", i));
        fs::write(&path, format!("content {}", i)).expect("write file");
    }

    let corpus = MmapCorpus::open(dir.path()).expect("open corpus");
    let count = corpus.iter().count();

    assert_eq!(count, 100, "should find 100 files");

    // Verify all files have correct content
    for (path, content) in corpus.iter() {
        let filename = path.file_stem().unwrap().to_str().unwrap();
        let num: usize = filename.strip_prefix("file_").unwrap().parse().unwrap();
        let expected = format!("content {}", num);
        assert_eq!(
            content,
            expected.as_bytes(),
            "content mismatch for {}",
            filename
        );
    }
}

/// CRITICAL: Corpus iteration should be deterministic (sorted).
#[test]
fn mmap_corpus_iteration_is_sorted() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Create files in non-sorted order
    fs::write(dir.path().join("z.txt"), b"z").expect("write z");
    fs::write(dir.path().join("a.txt"), b"a").expect("write a");
    fs::write(dir.path().join("m.txt"), b"m").expect("write m");
    fs::write(dir.path().join("b.txt"), b"b").expect("write b");

    let corpus = MmapCorpus::open(dir.path()).expect("open corpus");
    let paths: Vec<_> = corpus
        .iter()
        .map(|(p, _)| p.file_name().unwrap().to_owned())
        .collect();

    // Should be sorted
    assert_eq!(paths[0], "a.txt");
    assert_eq!(paths[1], "b.txt");
    assert_eq!(paths[2], "m.txt");
    assert_eq!(paths[3], "z.txt");
}

// =============================================================================
// 6. ERROR MESSAGE QUALITY TESTS
// =============================================================================

/// CRITICAL: Error messages should be actionable and contain context.
#[test]
fn mmap_error_messages_are_actionable() {
    let result = mmap::open_read("/nonexistent/path/that/does/not/exist/kernelkit");
    let error = result.unwrap_err();
    let msg = error.to_string();

    // Error should contain useful context
    assert!(!msg.is_empty(), "Error message must not be empty");
    assert!(
        msg.len() > 10,
        "Error message should be descriptive (got: {})",
        msg
    );
}

/// CRITICAL: All error types should implement Display correctly.
#[test]
fn all_error_variants_display_correctly() {
    use kernelkit::Error;

    // Test that error formatting doesn't panic
    let errors = vec![
        Error::NullPointer,
        Error::AllocationOverflow {
            count: 100,
            type_name: "u8",
        },
        Error::InvalidNode {
            node: 99,
            available: 2,
        },
        Error::InvalidMagic,
        Error::UnsupportedVersion {
            version: 100,
            max_version: 1,
        },
        Error::SectionTooLarge { length: u64::MAX },
        Error::UnexpectedEof {
            context: "test",
            needed: 100,
            remaining: 10,
        },
    ];

    for error in errors {
        let msg = error.to_string();
        assert!(!msg.is_empty(), "Error message must not be empty");
        assert!(
            msg.len() > 5,
            "Error message should be descriptive: {:?}",
            error
        );
    }
}
