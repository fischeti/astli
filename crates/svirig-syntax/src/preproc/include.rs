//! Finding the file an `` `include `` names.
//!
//! Expanded mode only. The formatter never follows an include
//! ([D6](../../../../docs/plan.md)): each file is formatted alone, and a
//! header's text is not part of the file being formatted.
//!
//! # Reading is a trait
//!
//! Resolution is a list of candidates and the first one that reads wins, so
//! the only thing it needs from the world is "give me this file, or don't".
//! Leaving that to the caller keeps the preprocessor testable without a
//! filesystem, and lets an editor answer out of the buffers it is holding
//! unsaved.
//!
//! # Where it looks
//!
//! 22.4 gives the quoted form the including file's own directory and then an
//! implementation-defined search, and reserves the angle form for the files
//! the implementation supplies. This one supplies none, so the angle list is
//! empty until a driver fills it.

use std::path::{Component, Path, PathBuf};

/// How deep `` `include `` may nest before it is treated as runaway.
///
/// 22.4 requires at least 15 levels, and real code stays far below that. The
/// limit is set high because it is a backstop rather than a rule: a cycle is
/// normally caught by the path check, and this only has to catch the ones that
/// check cannot see.
pub const MAX_DEPTH: usize = 200;

/// Reads the files an `` `include `` names.
pub trait Files {
    /// The contents of `path`, or `None` if there is nothing to read there.
    ///
    /// Probing and reading are one question on purpose. Two would be two
    /// answers that can disagree, and the resolver only ever wants the file.
    fn read(&self, path: &Path) -> Option<String>;
}

/// The filesystem.
pub struct Disk;

impl Files for Disk {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

/// Where `` `include `` looks, and what it reads with.
pub struct Includes<'a> {
    /// Searched for `` `include "f.svh" ``, after the directory of the file the
    /// include is used in.
    pub quoted: Vec<PathBuf>,
    /// Searched for `` `include <f.svh> ``. Empty unless a driver fills it:
    /// 22.4 reserves this form for files the implementation supplies, and this
    /// one supplies none.
    pub angle: Vec<PathBuf>,
    pub files: &'a dyn Files,
}

impl Includes<'static> {
    /// Nothing on the search path, reading from the filesystem.
    pub fn new() -> Includes<'static> {
        Includes {
            quoted: Vec::new(),
            angle: Vec::new(),
            files: &Disk,
        }
    }
}

impl Default for Includes<'static> {
    fn default() -> Includes<'static> {
        Includes::new()
    }
}

impl Includes<'_> {
    /// Finds `name` and reads it, or `None` if nothing on the search path has
    /// it.
    ///
    /// `used_in` is the file the `` `include `` is used in, which is the file
    /// it is written in unless a macro carried it somewhere else.
    pub fn resolve(
        &self,
        name: &str,
        used_in: Option<&Path>,
        angle: bool,
    ) -> Option<(PathBuf, String)> {
        self.search(name, used_in, angle)
            .into_iter()
            .find_map(|candidate| {
                let text = self.files.read(&candidate)?;
                Some((candidate, text))
            })
    }

    /// The candidates for `name`, in the order they are to be tried.
    fn search(&self, name: &str, used_in: Option<&Path>, angle: bool) -> Vec<PathBuf> {
        let name = Path::new(name);
        // An absolute name says where it is; there is nothing to search.
        if name.is_absolute() {
            return vec![clean(name)];
        }

        let own = (!angle).then(|| used_in.and_then(Path::parent)).flatten();
        let list = if angle { &self.angle } else { &self.quoted };

        own.into_iter()
            .chain(list.iter().map(PathBuf::as_path))
            .map(|dir| clean(&dir.join(name)))
            .collect()
    }
}

/// `path` with `.` and `..` resolved textually.
///
/// Textually, because the cycle check compares paths and `dir/../f.svh` is the
/// same file as `f.svh`. Two names that reach one file another way -- a
/// symlink, a hard link -- still read as two; [`MAX_DEPTH`] is what stops
/// those.
fn clean(path: &Path) -> PathBuf {
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
