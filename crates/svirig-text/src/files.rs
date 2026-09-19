//! File reading abstractions and path normalization.

use std::path::{Component, Path, PathBuf};

/// Interface for reading file contents by path.
///
/// Abstracting file access allows testing without filesystem dependencies and
/// supports reading from memory buffers (e.g. for language servers or test fixtures).
pub trait Reader: Sync {
    /// Reads the contents of `path`, returning `None` if the file does not exist
    /// or cannot be read.
    fn read(&self, path: &Path) -> Option<String>;
}

/// Standard filesystem reader implementation using [`std::fs`].
pub struct Disk;

impl Reader for Disk {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
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
