//! Shared file-identity fingerprint used to detect a file changing underneath
//! an active memory mapping (a TOCTOU window between `stat` and `mmap`).
//!
//! This is the single owner for both the single-file mmap path (`mmap.rs`) and
//! the corpus path (`corpus.rs`); previously each module carried a byte-identical
//! copy of the struct, `same_inode`, and the revalidation routine.

use crate::{Error, Result};

/// Identity fingerprint of a file at the moment it was stat-ed.
///
/// On unix this is `(len, dev, ino)`; on other platforms only `len` is
/// available, so identity checks fall back to length-only.
#[derive(Clone, Copy)]
pub(crate) struct FileIdentity {
    len: u64,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

impl FileIdentity {
    pub(crate) fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                len: metadata.len(),
                dev: metadata.dev(),
                ino: metadata.ino(),
            }
        }
        #[cfg(not(unix))]
        {
            Self {
                len: metadata.len(),
            }
        }
    }
}

#[cfg(unix)]
fn same_inode(expected: FileIdentity, current: FileIdentity) -> bool {
    expected.dev == current.dev && expected.ino == current.ino
}

#[cfg(not(unix))]
fn same_inode(_expected: FileIdentity, _current: FileIdentity) -> bool {
    true
}

/// Re-stat `file` and confirm it still matches `expected` and `expected_len`.
///
/// `changed_msg` is the call-site-specific detail attached to the error when
/// the file has changed (e.g. a corpus- vs single-file-specific hint).
///
/// # Errors
///
/// Returns `Err(Error::System)` if the file metadata cannot be read, or if the
/// length or inode identity no longer matches (the file was truncated, grown,
/// or replaced during mapping).
pub(crate) fn validate_file_identity(
    file: &std::fs::File,
    expected: FileIdentity,
    expected_len: u64,
    changed_msg: &'static str,
) -> Result<()> {
    let current_metadata = file.metadata().map_err(|source| Error::System {
        operation: "metadata(revalidate)",
        source,
    })?;
    let current = FileIdentity::from_metadata(&current_metadata);
    if current.len != expected_len || current.len != expected.len || !same_inode(expected, current) {
        return Err(Error::System {
            operation: "mmap(revalidate)",
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, changed_msg),
        });
    }
    Ok(())
}
