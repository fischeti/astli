//! What a build passes the preprocessor from outside the source.

use std::path::PathBuf;

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

    /// Defines `name` as `body` before the first file, as if by
    /// `` `define name body ``. A later definition of the same name wins.
    pub fn define(mut self, name: impl Into<String>, body: impl Into<String>) -> Build {
        self.defines.push((name.into(), body.into()));
        self
    }

    /// Returns `true` if the build neither searches a directory nor defines
    /// anything.
    pub fn is_empty(&self) -> bool {
        self.includes.quoted.is_empty() && self.includes.angle.is_empty() && self.defines.is_empty()
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
