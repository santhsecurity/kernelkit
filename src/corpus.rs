//! Memory-mapped local corpus iteration helpers.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::{Error, Result};

/// Memory-mapped corpus handle for large local datasets.
#[derive(Debug)]
pub struct MmapCorpus {
    /// Base directory containing the corpus files.
    pub base: PathBuf,
    /// Active memory mappings.
    pub mappings: Vec<MmapRegion>,
}

/// A single mapped file region.
#[derive(Debug)]
pub struct MmapRegion {
    mmap: Mmap,
    path: PathBuf,
    size: u64,
}

impl MmapCorpus {
    /// Open a directory of files as a memory-mapped corpus.
    ///
    /// Limits individual files to 1GB and the total corpus to 10GB.
    /// Use [`MmapCorpus::open_with_limits`] for custom limits.
    ///
    /// Linux applies `MADV_SEQUENTIAL` and `MADV_HUGEPAGE` to each mapping.
    /// # Errors
    /// Returns an error if reading the directory fails.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_limits(dir, 1024 * 1024 * 1024, 10 * 1024 * 1024 * 1024)
    }

    /// Open a directory of files as a memory-mapped corpus with size limits.
    ///
    /// # Limits
    /// * `max_file_bytes`: The maximum size of a single file in the corpus.
    /// * `max_total_bytes`: The maximum combined size of all mapped files.
    /// # Errors
    /// Returns an error if reading the directory fails.
    pub fn open_with_limits(
        dir: impl AsRef<Path>,
        max_file_bytes: u64,
        max_total_bytes: u64,
    ) -> Result<Self> {
        let base = dir.as_ref().to_path_buf();
        let mut file_paths = Vec::new();
        collect_files(&base, &mut file_paths)?;
        file_paths.sort();

        let mut mappings = Vec::with_capacity(file_paths.len());
        let mut total_bytes = 0u64;

        for path in file_paths {
            let file = File::open(&path).map_err(|source| Error::System {
                operation: "open",
                source,
            })?;
            let metadata = file.metadata().map_err(|source| Error::System {
                operation: "metadata",
                source,
            })?;

            let size = metadata.len();
            if size > max_file_bytes {
                return Err(Error::System {
                    operation: "open_with_limits",
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("file size {size} exceeds limit of {max_file_bytes}"),
                    ),
                });
            }

            total_bytes = total_bytes.saturating_add(size);
            if total_bytes > max_total_bytes {
                return Err(Error::System {
                    operation: "open_with_limits",
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("total corpus size exceeds limit of {max_total_bytes}"),
                    ),
                });
            }

            // SAFETY: mapping remains tied to the returned `Mmap`.
            let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(|source| {
                Error::System {
                    operation: "mmap",
                    source,
                }
            })?;
            advise_sequential(&mmap);

            mappings.push(MmapRegion { mmap, path, size });
        }

        Ok(Self { base, mappings })
    }

    /// Iterate over all files as memory-mapped byte slices.
    pub fn iter(&self) -> impl Iterator<Item = (&Path, &[u8])> {
        self.mappings
            .iter()
            .map(|region| (region.path.as_path(), region.mmap.as_ref()))
    }
}

impl MmapRegion {
    /// Path of the mapped file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Size of the mapped file.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Borrow the mapped bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.mmap.as_ref()
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|source| Error::System {
        operation: "read_dir",
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| Error::System {
            operation: "read_dir",
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| Error::System {
            operation: "file_type",
            source,
        })?;

        if file_type.is_symlink() {
            return Err(Error::System {
                operation: "collect_files",
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "symlinks are not permitted in corpus",
                ),
            });
        }

        if file_type.is_dir() {
            collect_files(&path, out)?;
        } else if file_type.is_file() {
            out.push(path);
        } else {
            return Err(Error::System {
                operation: "collect_files",
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unsupported file type in corpus: {}", path.display()),
                ),
            });
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn advise_sequential(mmap: &Mmap) {
    if mmap.is_empty() {
        return;
    }

    // SAFETY: advisory only; the mapping is valid for `mmap.len()` bytes.
    let ptr = mmap.as_ptr().cast::<libc::c_void>().cast_mut();
    let _ = unsafe { libc::madvise(ptr, mmap.len(), libc::MADV_SEQUENTIAL) };
    let _ = unsafe { libc::madvise(ptr, mmap.len(), libc::MADV_HUGEPAGE) };
}

#[cfg(not(target_os = "linux"))]
fn advise_sequential(_mmap: &Mmap) {}

#[cfg(test)]
mod tests {
    use super::MmapCorpus;
    use std::fs;

    #[test]
    fn iterates_over_mapped_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.txt"), b"alpha").expect("write a");
        fs::write(dir.path().join("b.txt"), b"beta").expect("write b");

        let corpus = MmapCorpus::open(dir.path()).expect("open corpus");
        let collected: Vec<_> = corpus.iter().map(|(_, bytes)| bytes.to_vec()).collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.iter().any(|bytes| bytes == b"alpha"));
        assert!(collected.iter().any(|bytes| bytes == b"beta"));
    }
}
