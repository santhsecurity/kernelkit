#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use kernelkit::{HugePageVec, MmapBlock};
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_legendary_property_hugepagevec(size in 0usize..10000) {
        let vec = HugePageVec::<u8>::new(size);
        prop_assert_eq!(vec.len(), size);
        prop_assert_eq!(vec.is_empty(), size == 0);
    }

    #[test]
    fn test_legendary_property_mmapblock(size in 1usize..10000) {
        let block = MmapBlock::new(size);
        prop_assert!(block.is_ok());
        if let Ok(b) = block {
            prop_assert_eq!(b.len(), size);
            prop_assert!(!b.is_empty());
        }
    }

    #[test]
    fn test_legendary_property_readahead(offset in 0u64..1000, size in 0usize..1000) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"test payload").unwrap();
        let file = std::fs::File::open(&path).unwrap();

        let res = kernelkit::readahead::readahead(&file, offset, size);
        prop_assert!(res.is_ok() || res.is_err()); // Ensure it doesn't panic
    }

    #[test]
    fn test_legendary_property_mmapcorpus(content in ".*") {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, content.as_bytes()).unwrap();

        let corpus = kernelkit::MmapCorpus::open(dir.path());
        prop_assert!(corpus.is_ok());
        if let Ok(c) = corpus {
            let items: Vec<_> = c.iter().collect();
            prop_assert_eq!(items.len(), 1);
            prop_assert_eq!(items[0].1, content.as_bytes());
        }
    }

    #[test]
    fn test_legendary_property_mmapblock_overflow_probes(size in usize::MAX - 4096..=usize::MAX) {
        let block = MmapBlock::new(size);
        prop_assert!(block.is_err(), "Must reject sizes that overflow");
    }

    #[test]
    fn test_legendary_property_hugepagevec_overflow_probes(size in usize::MAX / 2..=usize::MAX) {
        let result = HugePageVec::<u64>::new_fallible(size);
        prop_assert!(result.is_err(), "Must reject hugepage sizes that overflow limits");
    }

    #[test]
    fn test_legendary_property_binformat_file_header_overflow(version in u64::MAX - 1000..=u64::MAX) {
        let header = kernelkit::binformat::FileHeader {
            magic: b"TEST",
            version,
        };
        let mut bytes = Vec::new();
        header.write_to(&mut bytes).unwrap();
        // Just verify it doesn't crash on writing or reading
        let read = kernelkit::binformat::FileHeader::read_from(&bytes, b"TEST", u64::MAX);
        prop_assert!(read.is_ok(), "Must handle max u64 bounds safely without overflow");
    }

    #[test]
    fn test_legendary_property_read_section_overflow_probes(size in u64::MAX - 100..=u64::MAX) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(b"abc"); // Truncated compared to huge size

        let error = kernelkit::binformat::read_section(&bytes).expect_err("Must reject overflow lengths");
        prop_assert!(
            matches!(error, kernelkit::Error::SectionTooLarge { .. })
                || matches!(
                    error,
                    kernelkit::Error::UnexpectedEof {
                        context: "section payload",
                        ..
                    }
                ),
            "Must report correct error kind"
        );
    }
}
