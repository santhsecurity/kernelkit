#![no_main]
use libfuzzer_sys::fuzz_target;
use kernelkit::HugePageVec;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 { return; }

    // Use first 2 bytes as element count (0-65535)
    let count = u16::from_le_bytes([data[0], data[1]]) as usize;
    // Cap at reasonable size to avoid OOM
    let count = count.min(4096);

    // Test u8 allocation
    let mut vec = HugePageVec::<u8>::new(count);
    let slice = vec.as_mut_slice();
    assert_eq!(slice.len(), count);

    // Fill with data
    for (i, byte) in data[2..].iter().enumerate() {
        if i >= count { break; }
        slice[i] = *byte;
    }

    // Verify written data persists
    for (i, byte) in data[2..].iter().enumerate() {
        if i >= count { break; }
        assert_eq!(vec.as_slice()[i], *byte);
    }

    // Test u64 allocation
    let count64 = count.min(512);
    let mut vec64 = HugePageVec::<u64>::new(count64);
    let slice64 = vec64.as_mut_slice();
    assert_eq!(slice64.len(), count64);
    if count64 > 0 {
        slice64[0] = u64::MAX;
        assert_eq!(vec64.as_slice()[0], u64::MAX);
    }

    // Test zero-length
    let empty = HugePageVec::<u32>::new(0);
    assert!(empty.as_slice().is_empty());
});
