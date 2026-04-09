//! Extreme scale tests for kernelkit.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn page_size_is_power_of_two() {
    let ps = kernelkit::page_size();
    assert!(ps.is_power_of_two(), "page size {ps} is not power of two");
    assert!(ps >= 4096, "page size {ps} is suspiciously small");
}

#[test]
fn cpu_features_detect_is_idempotent() {
    let a = kernelkit::cpu_features::detect();
    let b = kernelkit::cpu_features::detect();
    assert_eq!(a, b);
}

#[test]
fn cpu_features_cache_sizes_reasonable() {
    let f = kernelkit::cpu_features::detect();
    assert!(
        f.cache_line_size >= 32,
        "cache line {} too small",
        f.cache_line_size
    );
    assert!(
        f.cache_line_size <= 256,
        "cache line {} too large",
        f.cache_line_size
    );
    assert!(f.l1_size > 0, "L1 size is 0");
}

#[test]
fn mmap_open_read_nonexistent() {
    let result = kernelkit::mmap::open_read(std::path::Path::new(
        "/nonexistent/path/that/does/not/exist",
    ));
    assert!(result.is_err());
}

#[test]
fn mmap_open_read_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.txt");
    std::fs::write(&path, b"").unwrap();
    let mmap = kernelkit::mmap::open_read(&path).unwrap();
    assert_eq!(mmap.len(), 0);
}

#[test]
fn mmap_open_read_1_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("one.txt");
    std::fs::write(&path, b"X").unwrap();
    let mmap = kernelkit::mmap::open_read(&path).unwrap();
    assert_eq!(mmap.len(), 1);
    assert_eq!(mmap[0], b'X');
}

#[test]
fn mmap_open_read_1mb() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.bin");
    std::fs::write(&path, vec![0xAB_u8; 1_000_000]).unwrap();
    let mmap = kernelkit::mmap::open_read(&path).unwrap();
    assert_eq!(mmap.len(), 1_000_000);
    assert_eq!(mmap[0], 0xAB);
    assert_eq!(mmap[999_999], 0xAB);
}

#[test]
fn binformat_read_from_empty() {
    let result = kernelkit::binformat::FileHeader::read_from(b"", b"TEST", 1);
    assert!(result.is_err());
}

#[test]
fn binformat_read_section_empty() {
    let result = kernelkit::binformat::read_section(b"");
    assert!(result.is_err());
}

#[test]
fn readahead_on_devnull() {
    let file = std::fs::File::open("/dev/null").unwrap();
    let _ = kernelkit::readahead::readahead(&file, 0, 4096);
}

#[test]
fn evict_pages_on_devnull() {
    let file = std::fs::File::open("/dev/null").unwrap();
    let _ = kernelkit::readahead::evict_pages(&file, 0, 0);
}
