//! Helpers for small self-describing binary formats.

use std::io::{self, Write};

use crate::{Error, Result};

const U64_WIDTH: usize = std::mem::size_of::<u64>();

/// Fixed binary header containing a magic prefix and format version.
///
/// The wire format is:
/// - raw `magic` bytes
/// - little-endian `u64` version
///
/// # Examples
///
/// ```
/// let header = kernelkit::FileHeader {
///     magic: b"KIT1",
///     version: 2,
/// };
/// let mut bytes = Vec::new();
/// header.write_to(&mut bytes).expect("write header");
///
/// let (version, rest) = kernelkit::binformat::FileHeader::read_from(&bytes, b"KIT1", 2)?;
/// assert_eq!(version, 2);
/// assert!(rest.is_empty());
/// # Ok::<(), kernelkit::Error>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileHeader<'a> {
    /// Format magic prefix.
    pub magic: &'a [u8],
    /// Format version encoded after the magic.
    pub version: u64,
}

impl FileHeader<'_> {
    /// Write the header to a byte sink.
    /// # Errors
    /// Returns an error if writing to the buffer fails.
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(self.magic)?;
        w.write_all(&self.version.to_le_bytes())
    }

    /// Read and validate a header from the start of `data`.
    ///
    /// Returns the decoded version and the remaining payload bytes.
    /// # Errors
    /// Returns an error if reading the header fails.
    pub fn read_from<'b>(
        data: &'b [u8],
        expected_magic: &[u8],
        max_version: u64,
    ) -> Result<(u64, &'b [u8])> {
        if data.len() < expected_magic.len() {
            return Err(Error::UnexpectedEof {
                context: "file header magic",
                needed: expected_magic.len(),
                remaining: data.len(),
            });
        }

        let (actual_magic, after_magic) = data.split_at(expected_magic.len());
        if actual_magic != expected_magic {
            return Err(Error::InvalidMagic);
        }

        if after_magic.len() < U64_WIDTH {
            return Err(Error::UnexpectedEof {
                context: "file header version",
                needed: U64_WIDTH,
                remaining: after_magic.len(),
            });
        }

        let (version_bytes, rest) = after_magic.split_at(U64_WIDTH);
        let version = u64::from_le_bytes(version_bytes.try_into().unwrap_or([0; 8]));
        if version > max_version {
            return Err(Error::UnsupportedVersion {
                version,
                max_version,
            });
        }

        Ok((version, rest))
    }
}

/// Read a length-prefixed binary section.
///
/// The wire format is:
/// - little-endian `u64` payload length
/// - payload bytes
///
/// Returns the payload and the remaining bytes after the section.
///
/// # Examples
///
/// ```
/// let mut bytes = Vec::new();
/// bytes.extend_from_slice(&(3_u64).to_le_bytes());
/// bytes.extend_from_slice(b"abc");
/// bytes.extend_from_slice(b"tail");
///
/// let (section, rest) = kernelkit::binformat::read_section(&bytes)?;
/// assert_eq!(section, b"abc");
/// assert_eq!(rest, b"tail");
/// # Ok::<(), kernelkit::Error>(())
/// ```
/// # Errors
/// Returns an error if reading the section fails.
pub fn read_section(data: &[u8]) -> Result<(&[u8], &[u8])> {
    if data.len() < U64_WIDTH {
        return Err(Error::UnexpectedEof {
            context: "section length",
            needed: U64_WIDTH,
            remaining: data.len(),
        });
    }

    let (length_bytes, after_length) = data.split_at(U64_WIDTH);
    let declared_len = u64::from_le_bytes(length_bytes.try_into().unwrap_or([0; 8]));
    let payload_len = usize::try_from(declared_len).map_err(|_| Error::SectionTooLarge {
        length: declared_len,
    })?;

    if after_length.len() < payload_len {
        return Err(Error::UnexpectedEof {
            context: "section payload",
            needed: payload_len,
            remaining: after_length.len(),
        });
    }

    Ok(after_length.split_at(payload_len))
}

#[cfg(test)]
mod tests {
    use super::{FileHeader, read_section};
    use proptest::prelude::*;

    #[test]
    fn file_header_round_trip() {
        let header = FileHeader {
            magic: b"KIT1",
            version: 3,
        };
        let mut bytes = Vec::new();
        header.write_to(&mut bytes).expect("write header");
        bytes.extend_from_slice(b"payload");

        let (version, rest) = FileHeader::read_from(&bytes, b"KIT1", 3).expect("read header");
        assert_eq!(version, 3);
        assert_eq!(rest, b"payload");
    }

    #[test]
    fn read_section_round_trip() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(4_u64).to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(b"tail");

        let (payload, rest) = read_section(&bytes).expect("read section");
        assert_eq!(payload, b"data");
        assert_eq!(rest, b"tail");
    }

    #[test]
    fn read_header_rejects_wrong_magic() {
        let mut bytes = Vec::new();
        FileHeader {
            magic: b"BAD!",
            version: 1,
        }
        .write_to(&mut bytes)
        .expect("write header");

        let error = FileHeader::read_from(&bytes, b"KIT1", 1).expect_err("wrong magic");
        assert!(matches!(error, crate::Error::InvalidMagic));
    }

    #[test]
    fn read_header_rejects_truncated_magic() {
        let error = FileHeader::read_from(b"KI", b"KIT1", 1).expect_err("truncated magic");
        assert!(matches!(
            error,
            crate::Error::UnexpectedEof {
                context: "file header magic",
                ..
            }
        ));
    }

    #[test]
    fn read_header_rejects_truncated_version() {
        let error =
            FileHeader::read_from(b"KIT1\x01\x00\x00", b"KIT1", 1).expect_err("truncated version");
        assert!(matches!(
            error,
            crate::Error::UnexpectedEof {
                context: "file header version",
                ..
            }
        ));
    }

    #[test]
    fn read_header_rejects_newer_version() {
        let mut bytes = Vec::new();
        FileHeader {
            magic: b"KIT1",
            version: 9,
        }
        .write_to(&mut bytes)
        .expect("write header");

        let error = FileHeader::read_from(&bytes, b"KIT1", 3).expect_err("newer version");
        assert!(matches!(
            error,
            crate::Error::UnsupportedVersion {
                version: 9,
                max_version: 3
            }
        ));
    }

    #[test]
    fn read_section_rejects_truncated_length() {
        let error = read_section(&[1, 2, 3]).expect_err("truncated length");
        assert!(matches!(
            error,
            crate::Error::UnexpectedEof {
                context: "section length",
                ..
            }
        ));
    }

    #[test]
    fn read_section_rejects_truncated_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(5_u64).to_le_bytes());
        bytes.extend_from_slice(b"abc");

        let error = read_section(&bytes).expect_err("truncated payload");
        assert!(matches!(
            error,
            crate::Error::UnexpectedEof {
                context: "section payload",
                ..
            }
        ));
    }

    #[test]
    fn read_section_zero_length() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(0_u64).to_le_bytes());
        bytes.extend_from_slice(b"tail");

        let (payload, rest) = read_section(&bytes).expect("zero-length section");
        assert!(payload.is_empty());
        assert_eq!(rest, b"tail");
    }

    #[test]
    fn nested_sections() {
        let mut inner = Vec::new();
        inner.extend_from_slice(&(3_u64).to_le_bytes());
        inner.extend_from_slice(b"abc");

        let mut outer = Vec::new();
        outer.extend_from_slice(&(inner.len() as u64).to_le_bytes());
        outer.extend_from_slice(&inner);
        outer.extend_from_slice(b"after");

        let (inner_data, rest) = read_section(&outer).expect("outer section");
        assert_eq!(rest, b"after");

        let (payload, remaining) = read_section(inner_data).expect("inner section");
        assert_eq!(payload, b"abc");
        assert!(remaining.is_empty());
    }

    #[test]
    fn read_header_accepts_zero_length_magic() {
        let bytes = 7u64.to_le_bytes();
        let (version, rest) = FileHeader::read_from(&bytes, b"", u64::MAX).expect("header");
        assert_eq!(version, 7);
        assert!(rest.is_empty());
    }

    #[test]
    fn read_header_accepts_max_u64_version() {
        let mut bytes = Vec::new();
        FileHeader {
            magic: b"KIT1",
            version: u64::MAX,
        }
        .write_to(&mut bytes)
        .expect("write header");

        let (version, rest) = FileHeader::read_from(&bytes, b"KIT1", u64::MAX).expect("read");
        assert_eq!(version, u64::MAX);
        assert!(rest.is_empty());
    }

    #[test]
    fn read_section_consumes_exact_remaining_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(4_u64).to_le_bytes());
        bytes.extend_from_slice(b"data");

        let (payload, rest) = read_section(&bytes).expect("exact section");
        assert_eq!(payload, b"data");
        assert!(rest.is_empty());
    }

    #[test]
    fn read_header_zero_magic_with_payload_rest() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&5u64.to_le_bytes());
        bytes.extend_from_slice(b"tail");

        let (version, rest) = FileHeader::read_from(&bytes, b"", 5).expect("header");
        assert_eq!(version, 5);
        assert_eq!(rest, b"tail");
    }

    #[test]
    fn read_section_rejects_platform_exhaustion_length() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&u64::MAX.to_le_bytes());
        let error = read_section(&bytes).expect_err("oversized section");
        assert!(
            matches!(error, crate::Error::SectionTooLarge { .. })
                || matches!(
                    error,
                    crate::Error::UnexpectedEof {
                        context: "section payload",
                        ..
                    }
                )
        );
    }

    #[test]
    fn read_header_rejects_zero_magic_when_version_too_new() {
        let bytes = 9u64.to_le_bytes();
        let error = FileHeader::read_from(&bytes, b"", 3).expect_err("version too new");
        assert!(matches!(
            error,
            crate::Error::UnsupportedVersion {
                version: 9,
                max_version: 3
            }
        ));
    }

    proptest! {
        #[test]
        fn file_header_write_then_read_is_identity(
            magic in prop::collection::vec(any::<u8>(), 0..8),
            version in any::<u64>(),
            tail in prop::collection::vec(any::<u8>(), 0..32)
        ) {
            let leaked_magic: &'static [u8] = Box::leak(magic.into_boxed_slice());
            let header = FileHeader { magic: leaked_magic, version };
            let mut bytes = Vec::new();
            header.write_to(&mut bytes).unwrap();
            bytes.extend_from_slice(&tail);

            let (decoded, rest) = FileHeader::read_from(&bytes, leaked_magic, u64::MAX).unwrap();
            prop_assert_eq!(decoded, version);
            prop_assert_eq!(rest, tail.as_slice());
        }
    }
}
