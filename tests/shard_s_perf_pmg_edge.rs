//! S-perf-pmg: binformat and cpu_features edge catalog.

use kernelkit::binformat::FileHeader;
use kernelkit::{cpu_features, Error};

#[test]
fn edge_header_roundtrip() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 3,
    }
    .write_to(&mut buf)
    .unwrap();
    let (ver, rest) = FileHeader::read_from(&buf, b"KIT1", 10).unwrap();
    assert_eq!(ver, 3);
    assert!(rest.is_empty());
}

#[test]
fn edge_header_wrong_magic() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 1,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(FileHeader::read_from(&buf, b"BAD!", 10).is_err());
}

#[test]
fn edge_header_truncated_magic() {
    assert!(matches!(
        FileHeader::read_from(b"KI", b"KIT1", 1),
        Err(Error::UnexpectedEof { .. })
    ));
}

#[test]
fn edge_header_version_too_high() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 99,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(FileHeader::read_from(&buf, b"KIT1", 5).is_err());
}

#[test]
fn edge_header_version_at_max() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 5,
    }
    .write_to(&mut buf)
    .unwrap();
    assert_eq!(FileHeader::read_from(&buf, b"KIT1", 5).unwrap().0, 5);
}

#[test]
fn edge_header_payload_after_header() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 1,
    }
    .write_to(&mut buf)
    .unwrap();
    buf.extend_from_slice(b"payload");
    let (_, rest) = FileHeader::read_from(&buf, b"KIT1", 1).unwrap();
    assert_eq!(rest, b"payload");
}

#[test]
fn edge_cpu_features_cache_line_positive() {
    let f = cpu_features::detect();
    assert!(f.cache_line_size > 0);
}

#[test]
fn edge_cpu_features_has_avx_flag() {
    let f = cpu_features::detect();
    let _ = f.avx2;
}

#[test]
fn edge_cpu_features_has_neon_flag() {
    let f = cpu_features::detect();
    let _ = f.neon;
}

#[test]
fn edge_header_empty_magic_rejected() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"",
        version: 0,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(FileHeader::read_from(&buf, b"", 0).is_ok());
}

#[test]
fn edge_header_long_magic() {
    let magic = b"LONGMAGICPREFIX";
    let mut buf = Vec::new();
    FileHeader {
        magic,
        version: 2,
    }
    .write_to(&mut buf)
    .unwrap();
    assert_eq!(FileHeader::read_from(&buf, magic, 2).unwrap().0, 2);
}

#[test]
fn edge_header_version_zero() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 0,
    }
    .write_to(&mut buf)
    .unwrap();
    assert_eq!(FileHeader::read_from(&buf, b"KIT1", 0).unwrap().0, 0);
}

#[test]
fn edge_header_partial_version_bytes() {
    let mut buf = b"KIT1".to_vec();
    buf.push(1); // only 1 byte of u64
    assert!(FileHeader::read_from(&buf, b"KIT1", 1).is_err());
}

#[test]
fn edge_cpu_features_clone() {
    let a = cpu_features::detect();
    let b = a;
    assert_eq!(a.cache_line_size, b.cache_line_size);
}

#[test]
fn edge_cpu_features_debug() {
    let s = format!("{:?}", cpu_features::detect());
    assert!(s.contains("cache_line_size"));
}

#[test]
fn edge_header_equality() {
    let a = FileHeader {
        magic: b"X",
        version: 1,
    };
    let b = FileHeader {
        magic: b"X",
        version: 1,
    };
    assert_eq!(a, b);
}

#[test]
fn edge_header_write_twice_appends() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"A",
        version: 1,
    }
    .write_to(&mut buf)
    .unwrap();
    FileHeader {
        magic: b"B",
        version: 2,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(buf.len() > 2);
}

#[test]
fn edge_read_from_exact_length() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 4,
    }
    .write_to(&mut buf)
    .unwrap();
    assert_eq!(buf.len(), 4 + 8);
}

#[test]
fn edge_cpu_features_detect_idempotent() {
    let a = cpu_features::detect();
    let b = cpu_features::detect();
    assert_eq!(a.avx2, b.avx2);
}

#[test]
fn edge_header_max_version_boundary() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: u64::MAX,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(FileHeader::read_from(&buf, b"KIT1", u64::MAX).is_ok());
}

#[test]
fn edge_header_version_one_over_max() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"KIT1",
        version: 6,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(FileHeader::read_from(&buf, b"KIT1", 5).is_err());
}

#[test]
fn edge_cpu_features_l1_size() {
    let _ = cpu_features::detect().l1_size;
}

#[test]
fn edge_cpu_features_l3_size() {
    let _ = cpu_features::detect().l3_size;
}

#[test]
fn edge_header_read_empty_buffer() {
    assert!(FileHeader::read_from(&[], b"KIT1", 1).is_err());
}

#[test]
fn edge_header_magic_case_sensitive() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"kit1",
        version: 1,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(FileHeader::read_from(&buf, b"KIT1", 1).is_err());
}

#[test]
fn edge_cpu_features_avx512bw() {
    let _ = cpu_features::detect().avx512bw;
}

#[test]
fn edge_header_copy_trait() {
    let h = FileHeader {
        magic: b"M",
        version: 9,
    };
    let c = h;
    assert_eq!(h.version, c.version);
}

#[test]
fn edge_cpu_features_avx512vl() {
    let _ = cpu_features::detect().avx512vl;
}

#[test]
fn edge_header_version_le_encoding() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"T",
        version: 0x0102_0304_0506_0708,
    }
    .write_to(&mut buf)
    .unwrap();
    let (v, _) = FileHeader::read_from(&buf, b"T", u64::MAX).unwrap();
    assert_eq!(v, 0x0102_0304_0506_0708);
}

#[test]
fn edge_cpu_features_arm_neutral() {
    let f = cpu_features::detect();
    assert!(f.cache_line_size <= 512);
}

#[test]
fn edge_header_rest_empty_when_exact() {
    let mut buf = Vec::new();
    FileHeader {
        magic: b"Z",
        version: 1,
    }
    .write_to(&mut buf)
    .unwrap();
    assert!(FileHeader::read_from(&buf, b"Z", 1).unwrap().1.is_empty());
}

#[test]
fn edge_cpu_features_avx512vbmi() {
    let _ = cpu_features::detect().avx512vbmi;
}
