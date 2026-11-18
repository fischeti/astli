//! File reading abstractions and path normalization.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// Where files are read from: the disk, an editor's unsaved buffers, or a
/// tree a test makes up.
///
/// An `` `include `` is found by asking [`exists`](Reader::exists) of each
/// place it could be, and read from the first that holds it.
pub trait Reader: Sync {
    /// Returns `true` if `path` names a file this reader holds.
    fn exists(&self, path: &Path) -> bool;

    /// Reads the contents of `path`, returning `None` if it cannot be read.
    fn read(&self, path: &Path) -> Option<String>;
}

/// The filesystem.
pub struct Disk;

impl Reader for Disk {
    /// Anything but a directory: an include of a device such as `/dev/null`
    /// reads as any file does.
    fn exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|it| !it.is_dir())
    }

    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

/// Files held in memory, keyed by the path they are read at: the buffers an
/// editor has not saved, or a tree a test makes up.
impl Reader for HashMap<PathBuf, String> {
    fn exists(&self, path: &Path) -> bool {
        self.contains_key(path)
    }

    fn read(&self, path: &Path) -> Option<String> {
        self.get(path).cloned()
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
