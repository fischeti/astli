//! Reading a file, and normalising the path that named it.
//!
//! # Reading is a trait
//!
//! The store is told what a path holds rather than going to look, so that
//! everything above it is testable without a filesystem and an editor can
//! answer out of the buffers it is holding unsaved.
//!
//! The bound is `Sync` because the unit of parallelism is a file and a reader
//! is the one thing several of them genuinely share. It is on the trait so
//! that a reader which caches says `Mutex` where it would otherwise have said
//! `RefCell`, and hears about it at the `impl` rather than from a call site
//! several layers away. See `docs/plan.md`.

use std::path::{Component, Path, PathBuf};

/// Reads the files a compilation names.
pub trait Reader: Sync {
    /// The contents of `path`, or `None` if there is nothing to read there.
    ///
    /// Probing and reading are one question on purpose. Two would be two
    /// answers that can disagree, and a caller only ever wants the file.
    fn read(&self, path: &Path) -> Option<String>;
}

/// The filesystem.
pub struct Disk;

impl Reader for Disk {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

/// `path` with `.` and `..` resolved textually.
///
/// Textually, because the check for a file that is already open compares
/// paths and `dir/../f.svh` is the same file as `f.svh`. Two names that reach
/// one file another way -- a symlink, a hard link -- still read as two.
pub fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            // A `..` cancels the segment before it, but only a real one:
            // `../..` is two levels up rather than none, and `/..` is `/`.
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
