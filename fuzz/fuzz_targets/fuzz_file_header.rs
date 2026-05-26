#![no_main]

use kernelkit::binformat::FileHeader;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let magic_len = (data[0] as usize % 8) + 1;
    if data.len() < magic_len + 8 {
        return;
    }
    let magic = &data[..magic_len];
    let max_ver = u64::from_le_bytes(data[magic_len..magic_len + 8].try_into().unwrap());
    let _ = FileHeader::read_from(data, magic, max_ver);
});
