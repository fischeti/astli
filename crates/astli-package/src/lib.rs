//! A package: the directory a design lives in, and what its `astli.toml`
//! says about it.
//!
//! A run that names no files finds them from the package it is in.
//! [`Package::find`] walks up from a directory to the package around it, and
//! [`Package::read`] reads a manifest named outright. Every path in a
//! manifest is relative to the directory holding it, the package's root, and
//! none may leave it, since the root is what a package publishes.
//!
//! The manifest also configures the tools: its `[fmt]` and `[lint]` tables
//! are handed over as they are, for the driver to read, so that the
//! formatter and the linter need no `serde`.
//!
//! ```
//! use astli_package::Package;
//!
//! let dir = std::env::temp_dir().join(format!("astli-package-doc-{}", std::process::id()));
//! std::fs::create_dir_all(dir.join("hw/include")).unwrap();
//! std::fs::write(
//!     dir.join("astli.toml"),
//!     "[package]\nname = \"soc\"\nsources = [\"hw\"]\ninclude-dirs = [\"hw/include\"]\n",
//! )
//! .unwrap();
//!
//! let package = Package::find(&dir.join("hw")).unwrap().unwrap();
//! assert_eq!(package.name.as_deref(), Some("soc"));
//! assert_eq!(package.sources, [package.root.join("hw")]);
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```

use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

use astli_text::clean;
use globset::GlobBuilder;
use serde::Deserialize;

/// The manifest's file name.
pub const MANIFEST: &str = "astli.toml";

/// A package's root and what its manifest says.
#[derive(Debug, Clone)]
pub struct Package {
    /// The directory every path in the manifest is relative to, absolute.
    pub root: PathBuf,
    /// The manifest, if the root has one.
    pub manifest: Option<PathBuf>,
    /// What a dependent calls the package; needed once something depends on
    /// it.
    pub name: Option<String>,
    /// Where the walk for the package's files starts, absolute and inside
    /// `root`. The root itself by default.
    pub sources: Vec<PathBuf>,
    /// The header roots a dependent searches, absolute and inside `root`.
    pub include_dirs: Vec<PathBuf>,
    /// Patterns, relative to `root`, for files the walk skips on top of
    /// those `.gitignore` names. `*` does not cross a `/`, and `**` does.
    pub exclude: Vec<String>,
    /// The `[fmt]` table, for the formatter's driver to read.
    pub fmt: toml::Table,
    /// The `[lint]` table, for the linter's driver to read.
    pub lint: toml::Table,
}

impl Package {
    /// The package `dir` is in: the nearest directory at or above it that
    /// holds an `astli.toml`, or else the root of the git repository it is
    /// in. `None` if it is in neither.
    ///
    /// The search stops at the repository's root, since a manifest above it
    /// belongs to something the repository is not part of. A dependency in
    /// a shared store has no `.git`, which is why a manifest is looked for
    /// first.
    pub fn find(dir: &Path) -> Result<Option<Package>, Error> {
        let dir = std::path::absolute(dir).map_err(|err| Error::Io(dir.to_path_buf(), err))?;
        // Cleaned, so that the ancestors of `a/b/..` are those of `a`.
        let dir = clean(&dir);
        for ancestor in dir.ancestors() {
            let manifest = ancestor.join(MANIFEST);
            if manifest.is_file() {
                return Package::read(&manifest).map(Some);
            }
            // A worktree or a submodule has a `.git` file rather than a
            // directory.
            if ancestor.join(".git").exists() {
                return Ok(Some(Package::bare(ancestor.to_path_buf())));
            }
        }
        Ok(None)
    }

    /// Reads the manifest at `path`. The directory holding it is the root.
    pub fn read(path: &Path) -> Result<Package, Error> {
        let text =
            std::fs::read_to_string(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        let wrong = |what: String| Error::Manifest(path.to_path_buf(), what);
        let File { package, fmt, lint } =
            toml::from_str(&text).map_err(|err| wrong(err.to_string().trim_end().to_string()))?;

        let absolute =
            std::path::absolute(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        let root = clean(&absolute)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();

        let inside = |key: &str, written: &Path| -> Result<PathBuf, Error> {
            let cleaned = clean(written);
            let outside = matches!(
                cleaned.components().next(),
                Some(Component::Prefix(_) | Component::RootDir | Component::ParentDir)
            );
            if outside {
                return Err(wrong(format!(
                    "`{key}`: `{}` is outside the package",
                    written.display()
                )));
            }
            // Joining an empty path would add a trailing separator.
            let full = match cleaned.as_os_str().is_empty() {
                true => root.clone(),
                false => root.join(cleaned),
            };
            if !full.exists() {
                return Err(wrong(format!(
                    "`{key}`: `{}` does not exist",
                    written.display()
                )));
            }
            Ok(full)
        };
        let sources = (package.sources.iter())
            .map(|path| inside("sources", path))
            .collect::<Result<_, _>>()?;
        let include_dirs = (package.include_dirs.iter())
            .map(|path| inside("include-dirs", path))
            .collect::<Result<_, _>>()?;
        // Checked now, so that a wrong pattern is reported whether or not
        // the walk reaches what it would match.
        for pattern in &package.exclude {
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|err| wrong(format!("`exclude`: `{pattern}`: {err}")))?;
        }

        Ok(Package {
            root,
            manifest: Some(path.to_path_buf()),
            name: package.name,
            sources,
            include_dirs,
            exclude: package.exclude,
            fmt,
            lint,
        })
    }

    /// A package with no manifest, which takes every default.
    fn bare(root: PathBuf) -> Package {
        Package {
            sources: vec![root.clone()],
            root,
            manifest: None,
            name: None,
            include_dirs: Vec::new(),
            exclude: Vec::new(),
            fmt: toml::Table::new(),
            lint: toml::Table::new(),
        }
    }
}

/// Why a package could not be found or read.
#[derive(Debug)]
pub enum Error {
    /// A file could not be read, or a path made absolute.
    Io(PathBuf, io::Error),
    /// The manifest at the path is malformed, or names what it may not.
    Manifest(PathBuf, String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(path, err) => write!(f, "{}: {err}", path.display()),
            Error::Manifest(path, what) => write!(f, "{}: {what}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(_, err) => Some(err),
            Error::Manifest(..) => None,
        }
    }
}

/// The manifest as written.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct File {
    package: Section,
    fmt: toml::Table,
    lint: toml::Table,
}

/// The `[package]` table.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Section {
    name: Option<String>,
    sources: Vec<PathBuf>,
    include_dirs: Vec<PathBuf>,
    exclude: Vec<String>,
}

impl Default for Section {
    fn default() -> Section {
        Section {
            name: None,
            sources: vec![PathBuf::from(".")],
            include_dirs: Vec::new(),
            exclude: Vec::new(),
        }
    }
}
