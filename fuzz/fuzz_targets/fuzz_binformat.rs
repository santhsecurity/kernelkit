#![no_main]
use libfuzzer_sys::fuzz_target;
use kernelkit::binformat::FileHeader;

fuzz_target!(|data: &[u8]| {
    let _ = FileHeader::read_from(data, b"TEST", u64::MAX);
    let _ = kernelkit::binformat::read_section(data);
});
