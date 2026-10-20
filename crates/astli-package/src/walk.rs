//! The files a package holds.

use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use crate::{Error, MANIFEST, Package};

/// The extensions of the files a walk yields: units, and headers written to
/// be included. A header of any other extension is found by being included.
const EXTENSIONS: [&str; 4] = ["sv", "svh", "v", "vh"];

impl Package {
    /// The SystemVerilog files the package holds, absolute and sorted: those
    /// under its `sources`, less what a `.gitignore` or `exclude` names and
    /// what a package nested in it holds.
    ///
    /// Only the `.gitignore` files at or below the root apply, whether or not
    /// the root is in a repository, so that a dependency reads the same from
    /// a shared store as from its own checkout, and a package checked out
    /// where an enclosing repository ignores it is still walked. A global
    /// ignore file and `.git/info/exclude` do not apply, since they differ
    /// between machines.
    pub fn walk(&self) -> Result<Vec<PathBuf>, Error> {
        let filter = Filter {
            root: self.root.clone(),
            sources: self.sources.clone(),
            exclude: patterns(&self.manifest, "exclude", &self.exclude)?,
        };
        let mut builder = WalkBuilder::new(&self.root);
        builder
            .standard_filters(false)
            .git_ignore(true)
            .require_git(false)
            .filter_entry(move |entry| {
                let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
                filter.keeps(entry.path(), is_dir)
            });

        let mut files = Vec::new();
        for entry in builder.build() {
            let entry = entry.map_err(|err| Error::Walk(err.to_string()))?;
            let path = entry.path();
            let wanted = (path.extension())
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| EXTENSIONS.contains(&extension));
            // Follows a link, which the walk itself does not.
            if wanted && path.is_file() {
                files.push(entry.into_path());
            }
        }
        files.sort();
        Ok(files)
    }
}

/// The patterns the manifest at `manifest` gives for `key`, matched against
/// a path relative to the root.
pub(crate) fn patterns(
    manifest: &Option<PathBuf>,
    key: &str,
    patterns: &[String],
) -> Result<GlobSet, Error> {
    let wrong = |what: String| Error::Manifest(manifest.clone().unwrap_or_default(), what);
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|err| wrong(format!("`{key}`: `{pattern}`: {err}")))?;
        set.add(glob);
    }
    set.build().map_err(|err| wrong(err.to_string()))
}

/// What the walk enters or yields, decided before it reads a directory, so
/// that what it leaves out costs nothing.
struct Filter {
    root: PathBuf,
    sources: Vec<PathBuf>,
    exclude: GlobSet,
}

impl Filter {
    fn keeps(&self, path: &Path, is_dir: bool) -> bool {
        if path == self.root {
            return true;
        }
        if path.file_name().is_some_and(|name| name == ".git") {
            return false;
        }
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        if self.exclude.is_match(relative) {
            return false;
        }
        // Another package, such as vendored IP, which would clash with the
        // dependency it copies.
        if is_dir && path.join(MANIFEST).is_file() {
            return false;
        }
        // Under a source, or a directory on the way to one.
        (self.sources.iter())
            .any(|source| path.starts_with(source) || (is_dir && source.starts_with(path)))
    }
}
