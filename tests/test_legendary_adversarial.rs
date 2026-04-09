#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unnecessary_wraps,
    missing_docs
)]

use kernelkit::readahead::readahead;
use kernelkit::{HugePageVec, MmapBlock, MmapCorpus};
use std::error::Error;
use std::fs;

#[test]
fn test_legendary_adversarial_hugepagevec_massive() -> Result<(), Box<dyn Error>> {
    // Attempting to allocate an absurd amount of memory should gracefully fall back or fail safely.
    // Given the constraints and type size, checking boundary limits.
    let _count = usize::MAX / std::mem::size_of::<u64>() + 1;
    // OOM allocation simulation should gracefully fail with new_fallible
    let result = HugePageVec::<u64>::new_fallible(usize::MAX / 2);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_legendary_adversarial_mmapblock_zero() -> Result<(), Box<dyn Error>> {
    let res = MmapBlock::new(0);
    assert!(res.is_err());
    Ok(())
}

#[test]
fn test_legendary_adversarial_mmapblock_huge() -> Result<(), Box<dyn Error>> {
    // Allocating an impossible size should return Err rather than panic.
    let res = MmapBlock::new(usize::MAX - 4095);
    assert!(res.is_err());
    Ok(())
}

#[test]
fn test_legendary_adversarial_mmapcorpus_empty() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let corpus = MmapCorpus::open(dir.path())?;
    assert_eq!(corpus.mappings.len(), 0);
    Ok(())
}

#[test]
fn test_legendary_adversarial_mmapcorpus_null_byte_path() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    // We can't actually write a file with a null byte in its name on standard UNIX,
    // but we can try to pass a path with a null byte to open.
    let null_path = dir.path().join("test\0dir");
    let res = MmapCorpus::open(null_path);
    assert!(res.is_err());
    Ok(())
}

#[test]
fn test_legendary_adversarial_readahead_overflow() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.txt");
    fs::write(&path, b"test")?;
    let file = fs::File::open(&path)?;
    // offset at MAX, count at MAX shouldn't crash the engine, just return an error or be ignored
    let res = readahead(&file, u64::MAX, usize::MAX);
    // Might fail depending on OS, or be a no-op on non-linux. We just assert it doesn't panic.
    if cfg!(target_os = "linux") {
        assert!(
            res.is_err(),
            "readahead should fail with invalid bounds on linux"
        );
    } else {
        assert!(res.is_ok(), "readahead is noop on non-linux");
    }
    Ok(())
}

#[test]
fn test_legendary_adversarial_binformat_invalid_magic() -> Result<(), Box<dyn Error>> {
    let payload = vec![0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00];
    let res = kernelkit::binformat::FileHeader::read_from(&payload, b"KIT1", 1);
    assert!(res.is_err(), "Engine must reject invalid magic bytes");
    Ok(())
}

#[test]
fn test_legendary_adversarial_binformat_short_payload() -> Result<(), Box<dyn Error>> {
    let payload = vec![0x4B, 0x4B, 0x49, 0x54]; // Valid magic but too short
    let res = kernelkit::binformat::FileHeader::read_from(&payload, b"KIT1", 1);
    assert!(res.is_err(), "Engine must handle short payloads safely");
    Ok(())
}

#[test]
fn test_legendary_adversarial_hugepagevec_zero_sized() -> Result<(), Box<dyn Error>> {
    let vec = HugePageVec::<()>::new(100);
    assert_eq!(
        vec.len(),
        100,
        "Zero-sized huge page vec should return the right len"
    );
    Ok(())
}

#[test]
fn test_legendary_adversarial_mmapblock_alternating_patterns() -> Result<(), Box<dyn Error>> {
    let block = MmapBlock::new(4096).unwrap();
    let ptr = block.as_mut_ptr();
    unsafe {
        for i in 0..4096 {
            ptr.add(i).write(if i % 2 == 0 { 0xFF } else { 0x00 });
        }
        for i in 0..4096 {
            assert_eq!(
                ptr.add(i).read(),
                if i % 2 == 0 { 0xFF } else { 0x00 },
                "Pattern must match"
            );
        }
    }
    Ok(())
}
