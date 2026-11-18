//! File reading abstractions and path normalization.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Where files are read from: the disk, an editor's unsaved buffers, or a
/// tree a test makes up.
///
/// An `` `include `` is found by asking [`exists`](Reader::exists) of each
/// place it could be, and read from the first that holds it; a file that
/// exists and cannot be read is an error rather than a reason to look on.
pub trait Reader: Sync {
    /// Returns `true` if `path` names a file this reader holds.
    fn exists(&self, path: &Path) -> bool;

    /// Reads the text of `path`, or says why it cannot be read.
    fn read(&self, path: &Path) -> io::Result<String>;
}

/// The filesystem.
pub struct Disk;

/// What a Git LFS pointer begins with.
const LFS_POINTER: &str = "version https://git-lfs.github.com/spec/v1";

impl Reader for Disk {
    /// Anything but a directory: an include of a device such as `/dev/null`
    /// reads as any file does.
    fn exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|it| !it.is_dir())
    }

    /// A Git LFS pointer is an error rather than text, since it would
    /// otherwise be read as a file that does not parse, with no word of
    /// why. The error says what fetches the file: `git lfs pull` in a git
    /// working copy, and outside one, which a dependency's checkout is,
    /// asking for its LFS files.
    fn read(&self, path: &Path) -> io::Result<String> {
        let text = std::fs::read_to_string(path)?;
        if !is_lfs_pointer(&text) {
            return Ok(text);
        }
        let absolute = std::path::absolute(path)?;
        let working = absolute.ancestors().any(|dir| dir.join(".git").exists());
        let fetch = match working {
            true => "`git lfs pull` fetches it",
            false => "to fetch a dependency's, set `lfs = true` on its requirement",
        };
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "a Git LFS pointer, not the file it stands for, which was never fetched: {fetch}"
            ),
        ))
    }
}

/// Returns `true` if `text` is what a commit holds in place of a file stored
/// with Git LFS, which stands in for it in a checkout that did not fetch it.
pub fn is_lfs_pointer(text: &str) -> bool {
    text.starts_with(LFS_POINTER)
}

/// Files held in memory, keyed by the path they are read at: the buffers an
/// editor has not saved, or a tree a test makes up.
impl Reader for HashMap<PathBuf, String> {
    fn exists(&self, path: &Path) -> bool {
        self.contains_key(path)
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        self.get(path)
            .cloned()
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

/// Normalizes a file path textually by resolving `.` and `..` components
/// without accessing the filesystem.
pub fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) => {}
                _ => out.push(part),
            },
            _ => out.push(part),
        }
    }
    out
}
