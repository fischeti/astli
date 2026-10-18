//! What a build passes the preprocessor from outside the source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::include::Includes;

/// The name the buffer of a [`Build`]'s definitions is registered under, so
/// that a diagnostic or a definition's origin can say where it came from.
pub const COMMAND_LINE: &str = "<command-line>";

/// Include directories and definitions, as a filelist or a command line gives
/// them.
#[derive(Debug, Clone, Default)]
pub struct Build {
    pub(crate) includes: Includes,
    defines: Vec<(String, String)>,
}

impl Build {
    /// Creates a build with no include directories and nothing defined.
    pub fn new() -> Build {
        Build::default()
    }

    /// Searches `dir` for `` `include "name" ``, after the including file's
    /// own directory.
    pub fn include_dir(mut self, dir: impl Into<PathBuf>) -> Build {
        self.includes.quoted.push(dir.into());
        self
    }

    /// Searches `dir` for `` `include <name> ``.
    pub fn system_include_dir(mut self, dir: impl Into<PathBuf>) -> Build {
        self.includes.angle.push(dir.into());
        self
    }

    /// Searches `files` for an `` `include "name" `` that no directory
    /// resolves: the one whose path ends in `name`, which must be the only
    /// one. Replaces the files given before.
    ///
    /// A package's include directories are what its dependents search, and
    /// this is how its own files find a header without one.
    pub fn include_files(mut self, files: impl IntoIterator<Item = PathBuf>) -> Build {
        let mut by_suffix: HashMap<_, Vec<PathBuf>> = HashMap::new();
        for file in files {
            if let Some(name) = file.file_name() {
                by_suffix.entry(name.to_os_string()).or_default().push(file);
            }
        }
        self.includes.by_suffix = Arc::new(by_suffix);
        self
    }

    /// Defines `name` as `body` before the first file, as if by
    /// `` `define name body ``. A later definition of the same name wins.
    pub fn define(mut self, name: impl Into<String>, body: impl Into<String>) -> Build {
        self.defines.push((name.into(), body.into()));
        self
    }

    /// Where `` `include "name" ``, or `` `include <name> `` when `angle`,
    /// written in `used_in` may be found, in the order the preprocessor looks.
    pub fn search(&self, name: &str, used_in: Option<&Path>, angle: bool) -> Vec<PathBuf> {
        self.includes.search(name, used_in, angle)
    }

    /// The file `` `include "name" ``, or `` `include <name> `` when `angle`,
    /// written in `used_in` names: the first of [`search`](Build::search)
    /// that `exists` holds, or else the one of the
    /// [`include_files`](Build::include_files) whose path ends in `name`.
    pub fn resolve(
        &self,
        name: &str,
        used_in: Option<&Path>,
        angle: bool,
        exists: impl Fn(&Path) -> bool,
    ) -> Resolved {
        let searched = self.search(name, used_in, angle);
        if let Some(found) = searched.into_iter().find(|candidate| exists(candidate)) {
            return Resolved::Found(found);
        }
        if angle {
            return Resolved::NotFound;
        }
        Resolved::from_suffixed(self.includes.suffixed(name))
    }

    /// Returns `true` if the build neither searches a directory nor defines
    /// anything.
    pub fn is_empty(&self) -> bool {
        self.includes.quoted.is_empty()
            && self.includes.angle.is_empty()
            && self.includes.by_suffix.is_empty()
            && self.defines.is_empty()
    }

    /// The definitions as `` `define `` lines, which lexes them the way a
    /// file's are and gives each one a place to point at.
    pub(crate) fn command_line(&self) -> Option<String> {
        if self.defines.is_empty() {
            return None;
        }
        let mut text = String::new();
        for (name, body) in &self.defines {
            text.push_str(&format!("`define {name} {body}\n"));
        }
        Some(text)
    }
}

/// What an `` `include `` names, as [`Build::resolve`] finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The file it names.
    Found(PathBuf),
    /// No directory holds it, and more than one file's path ends in it.
    Ambiguous(Vec<PathBuf>),
    /// Nothing holds it.
    NotFound,
}

impl Resolved {
    /// What the files whose path ends in a name make of it.
    pub(crate) fn from_suffixed(mut found: Vec<PathBuf>) -> Resolved {
        match found.len() {
            0 => Resolved::NotFound,
            1 => Resolved::Found(found.remove(0)),
            _ => Resolved::Ambiguous(found),
        }
    }
}
