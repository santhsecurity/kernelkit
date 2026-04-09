use kernelkit::Error;
use kernelkit::mmap::{MmapAdvice, open_read, open_read_with_size, open_with_advice};
use std::io::Write;
use std::sync::Arc;
use std::thread;
use tempfile::NamedTempFile;

macro_rules! mmap_size_test {
    ($name:ident, $size:expr) => {
        #[test]
        fn $name() {
            let mut file = NamedTempFile::new().expect("failed to create temp file");
            let data: Vec<u8> = (0..$size).map(|i| (i % 256) as u8).collect();
            file.write_all(&data).expect("failed to write data");

            let path = file.path();

            let mmap = open_read(path).expect("failed to mmap file");
            assert_eq!(mmap.len(), $size);
            assert_eq!(&mmap[..], &data[..]);

            let mmap_with_size =
                open_read_with_size(path, $size as u64).expect("failed to mmap file with size");
            assert_eq!(mmap_with_size.len(), $size);
            assert_eq!(&mmap_with_size[..], &data[..]);
        }
    };
}

mmap_size_test!(test_mmap_size_1, 1);
mmap_size_test!(test_mmap_size_2, 2);
mmap_size_test!(test_mmap_size_3, 3);
mmap_size_test!(test_mmap_size_4, 4);
mmap_size_test!(test_mmap_size_5, 5);
mmap_size_test!(test_mmap_size_7, 7);
mmap_size_test!(test_mmap_size_8, 8);
mmap_size_test!(test_mmap_size_15, 15);
mmap_size_test!(test_mmap_size_16, 16);
mmap_size_test!(test_mmap_size_17, 17);
mmap_size_test!(test_mmap_size_31, 31);
mmap_size_test!(test_mmap_size_32, 32);
mmap_size_test!(test_mmap_size_33, 33);
mmap_size_test!(test_mmap_size_63, 63);
mmap_size_test!(test_mmap_size_64, 64);
mmap_size_test!(test_mmap_size_65, 65);
mmap_size_test!(test_mmap_size_127, 127);
mmap_size_test!(test_mmap_size_128, 128);
mmap_size_test!(test_mmap_size_129, 129);
mmap_size_test!(test_mmap_size_255, 255);
mmap_size_test!(test_mmap_size_256, 256);
mmap_size_test!(test_mmap_size_257, 257);
mmap_size_test!(test_mmap_size_511, 511);
mmap_size_test!(test_mmap_size_512, 512);
mmap_size_test!(test_mmap_size_513, 513);
mmap_size_test!(test_mmap_size_1023, 1023);
mmap_size_test!(test_mmap_size_1024, 1024);
mmap_size_test!(test_mmap_size_1025, 1025);
mmap_size_test!(test_mmap_size_2047, 2047);
mmap_size_test!(test_mmap_size_2048, 2048);
mmap_size_test!(test_mmap_size_2049, 2049);
mmap_size_test!(test_mmap_size_4095, 4095);
mmap_size_test!(test_mmap_size_4096, 4096);
mmap_size_test!(test_mmap_size_4097, 4097);
mmap_size_test!(test_mmap_size_8191, 8191);
mmap_size_test!(test_mmap_size_8192, 8192);
mmap_size_test!(test_mmap_size_8193, 8193);
mmap_size_test!(test_mmap_size_16383, 16383);
mmap_size_test!(test_mmap_size_16384, 16384);
mmap_size_test!(test_mmap_size_16385, 16385);
mmap_size_test!(test_mmap_size_32767, 32767);
mmap_size_test!(test_mmap_size_32768, 32768);
mmap_size_test!(test_mmap_size_65535, 65535);
mmap_size_test!(test_mmap_size_65536, 65536);
mmap_size_test!(test_mmap_size_131072, 131072);
mmap_size_test!(test_mmap_size_262144, 262144);
mmap_size_test!(test_mmap_size_524288, 524288);
mmap_size_test!(test_mmap_size_1048576, 1048576);
mmap_size_test!(test_mmap_size_2097152, 2097152);
mmap_size_test!(test_mmap_size_4194304, 4194304);
mmap_size_test!(test_mmap_size_8388608, 8388608);
mmap_size_test!(test_mmap_size_10000000, 10000000);

macro_rules! mmap_correctness_test {
    ($name:ident, $size:expr) => {
        #[test]
        fn $name() {
            let mut file = NamedTempFile::new().expect("failed to create temp file");
            let data: Vec<u8> = (0..$size).map(|i| (i % 256) as u8).collect();
            file.write_all(&data).expect("failed to write data");

            let path = file.path();

            let fs_read_data = std::fs::read(path).expect("std::fs::read failed");
            let mmap = open_read(path).expect("failed to mmap file");

            assert_eq!(&mmap[..], &fs_read_data[..]);
            assert_eq!(mmap.len(), fs_read_data.len());
        }
    };
}

mmap_correctness_test!(test_mmap_correctness_1, 1);
mmap_correctness_test!(test_mmap_correctness_2, 2);
mmap_correctness_test!(test_mmap_correctness_3, 3);
mmap_correctness_test!(test_mmap_correctness_4, 4);
mmap_correctness_test!(test_mmap_correctness_5, 5);
mmap_correctness_test!(test_mmap_correctness_7, 7);
mmap_correctness_test!(test_mmap_correctness_8, 8);
mmap_correctness_test!(test_mmap_correctness_15, 15);
mmap_correctness_test!(test_mmap_correctness_16, 16);
mmap_correctness_test!(test_mmap_correctness_17, 17);
mmap_correctness_test!(test_mmap_correctness_31, 31);
mmap_correctness_test!(test_mmap_correctness_32, 32);
mmap_correctness_test!(test_mmap_correctness_33, 33);
mmap_correctness_test!(test_mmap_correctness_63, 63);
mmap_correctness_test!(test_mmap_correctness_64, 64);
mmap_correctness_test!(test_mmap_correctness_65, 65);
mmap_correctness_test!(test_mmap_correctness_127, 127);
mmap_correctness_test!(test_mmap_correctness_128, 128);
mmap_correctness_test!(test_mmap_correctness_129, 129);
mmap_correctness_test!(test_mmap_correctness_255, 255);
mmap_correctness_test!(test_mmap_correctness_256, 256);
mmap_correctness_test!(test_mmap_correctness_257, 257);
mmap_correctness_test!(test_mmap_correctness_511, 511);
mmap_correctness_test!(test_mmap_correctness_512, 512);
mmap_correctness_test!(test_mmap_correctness_513, 513);
mmap_correctness_test!(test_mmap_correctness_1023, 1023);
mmap_correctness_test!(test_mmap_correctness_1024, 1024);
mmap_correctness_test!(test_mmap_correctness_1025, 1025);
mmap_correctness_test!(test_mmap_correctness_2047, 2047);
mmap_correctness_test!(test_mmap_correctness_2048, 2048);
mmap_correctness_test!(test_mmap_correctness_2049, 2049);
mmap_correctness_test!(test_mmap_correctness_4095, 4095);
mmap_correctness_test!(test_mmap_correctness_4096, 4096);
mmap_correctness_test!(test_mmap_correctness_4097, 4097);
mmap_correctness_test!(test_mmap_correctness_8191, 8191);
mmap_correctness_test!(test_mmap_correctness_8192, 8192);
mmap_correctness_test!(test_mmap_correctness_8193, 8193);
mmap_correctness_test!(test_mmap_correctness_16383, 16383);
mmap_correctness_test!(test_mmap_correctness_16384, 16384);
mmap_correctness_test!(test_mmap_correctness_16385, 16385);
mmap_correctness_test!(test_mmap_correctness_32767, 32767);
mmap_correctness_test!(test_mmap_correctness_32768, 32768);
mmap_correctness_test!(test_mmap_correctness_65535, 65535);
mmap_correctness_test!(test_mmap_correctness_65536, 65536);
mmap_correctness_test!(test_mmap_correctness_131072, 131072);
mmap_correctness_test!(test_mmap_correctness_262144, 262144);
mmap_correctness_test!(test_mmap_correctness_524288, 524288);
mmap_correctness_test!(test_mmap_correctness_1048576, 1048576);
mmap_correctness_test!(test_mmap_correctness_2097152, 2097152);
mmap_correctness_test!(test_mmap_correctness_4194304, 4194304);
mmap_correctness_test!(test_mmap_correctness_8388608, 8388608);
mmap_correctness_test!(test_mmap_correctness_10000000, 10000000);

#[test]
fn test_mmap_empty_file() {
    let file = NamedTempFile::new().expect("failed to create temp file");
    let path = file.path();

    // According to mmap semantics, mapping a 0-byte file should fail
    // kernelkit might handle this specifically. Let's see what it returns.
    let mmap_res = open_read(path);
    assert!(mmap_res.is_err() || mmap_res.unwrap().is_empty());
}

#[test]
fn test_concurrent_mmap_8_threads() {
    let mut file = NamedTempFile::new().expect("failed to create temp file");
    let size = 1024 * 1024; // 1MB
    let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    file.write_all(&data).expect("failed to write data");

    let path = Arc::new(file.path().to_path_buf());

    let mut threads = vec![];
    for _ in 0..8 {
        let path_clone = path.clone();
        threads.push(thread::spawn(move || {
            let mmap = open_read(&*path_clone).expect("failed to mmap file concurrently");
            assert_eq!(mmap.len(), size);
            // Verify a few elements to ensure correctness without copying the whole thing again
            assert_eq!(mmap[0], 0);
            assert_eq!(mmap[size - 1], ((size - 1) % 256) as u8);

            // Drop it immediately to test concurrency overlapping correctly
            drop(mmap);
        }));
    }

    for t in threads {
        t.join().expect("thread failed");
    }
}
