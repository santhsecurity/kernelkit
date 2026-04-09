#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kernelkit::readahead::readahead;
use kernelkit::{HugePageVec, MmapBlock, MmapCorpus};
use std::error::Error;
use std::fs;

#[test]
fn test_legendary_unit_hugepagevec() -> Result<(), Box<dyn Error>> {
    let mut vec = HugePageVec::<u8>::new(10);
    assert_eq!(vec.len(), 10);
    assert!(!vec.is_empty());
    vec.as_mut_slice()[0] = 42;
    assert_eq!(vec.as_slice()[0], 42);
    Ok(())
}

#[test]
fn test_legendary_unit_mmapblock() -> Result<(), Box<dyn Error>> {
    let mut block = MmapBlock::new(4096)?;
    assert_eq!(block.len(), 4096);
    assert!(!block.is_empty());
    assert!(block.numa_node().is_none());

    let ptr = block.as_mut_ptr();
    unsafe {
        ptr.write(1);
        assert_eq!(ptr.read(), 1);
    }
    Ok(())
}

#[test]
fn test_legendary_unit_mmapcorpus() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("test.txt"), b"hello corpus")?;
    let corpus = MmapCorpus::open(dir.path())?;

    let items: Vec<_> = corpus.iter().collect();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].1, b"hello corpus");
    Ok(())
}

#[test]
fn test_legendary_unit_readahead() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.txt");
    fs::write(&path, b"readahead data")?;

    let file = fs::File::open(&path)?;
    readahead(&file, 0, 10)?;
    Ok(())
}

use std::sync::Arc;
use std::thread;

#[test]
fn test_legendary_unit_concurrent_hugepagevec() -> Result<(), Box<dyn Error>> {
    let vec = Arc::new(kernelkit::HugePageVec::<u32>::new(100));
    let mut handles = vec![];
    for _ in 0..32 {
        let v = vec.clone();
        handles.push(thread::spawn(move || {
            let slice = v.as_slice();
            assert_eq!(slice.len(), 100);
            let mut sum = 0;
            for val in slice {
                sum += val;
            }
            assert_eq!(sum, 0); // They are all initialized to 0
        }));
    }
    for handle in handles {
        assert!(handle.join().is_ok(), "Thread panicked");
    }
    Ok(())
}

#[test]
fn test_legendary_unit_concurrent_mmapblock() -> Result<(), Box<dyn Error>> {
    // 32 threads hammering MmapBlock creation and destruction
    let mut handles = vec![];
    for _ in 0..32 {
        handles.push(thread::spawn(|| {
            for _ in 0..10 {
                let block = MmapBlock::new(4096).unwrap();
                assert_eq!(block.len(), 4096);
                let mut block = block;
                let ptr = block.as_mut_ptr();
                unsafe {
                    ptr.write(0xAA);
                    assert_eq!(ptr.read(), 0xAA);
                }
            }
        }));
    }
    for handle in handles {
        assert!(handle.join().is_ok(), "Thread panicked");
    }
    Ok(())
}

#[test]
fn test_legendary_unit_concurrent_mmapcorpus() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("corpus_test.txt");
    fs::write(&path, b"shared corpus data")?;

    // Multiple threads opening and reading the same MmapCorpus
    let mut handles = vec![];
    let path_buf = dir.path().to_path_buf();
    for _ in 0..32 {
        let p = path_buf.clone();
        handles.push(thread::spawn(move || {
            let corpus = MmapCorpus::open(&p).unwrap();
            let items: Vec<_> = corpus.iter().collect();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].1, b"shared corpus data");
        }));
    }
    for handle in handles {
        assert!(handle.join().is_ok(), "Thread panicked");
    }
    Ok(())
}
