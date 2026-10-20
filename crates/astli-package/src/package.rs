//! Finding a package's root, and reading its manifest.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use astli_text::clean;
use serde::Deserialize;

use crate::Error;
use crate::features::{self, Feature};
use crate::walk::patterns;

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
    /// `include` by default, where it exists.
    pub include_dirs: Vec<PathBuf>,
    /// Patterns, relative to `root`, for files the walk skips on top of
    /// those `.gitignore` names. `*` does not cross a `/`, and `**` does.
    pub exclude: Vec<String>,
    /// Patterns, relative to `root` and matched as `exclude`'s are, for the
    /// files only the package itself reads, such as its testbenches: a
    /// dependent never sees them.
    pub dev: Vec<String>,
    /// What `[features]` declares, by name.
    pub features: BTreeMap<String, Feature>,
    /// The features a dependent gets without asking, `[features] default`.
    /// The package's own runs enable every feature instead.
    pub default_features: Vec<String>,
    /// The features `[dependencies]` asks of each dependency, by its name.
    /// Where a dependency is found is the resolver's.
    pub dependencies: BTreeMap<String, Vec<String>>,
    /// The features enabled: every one, until a design selects them.
    pub enabled: BTreeSet<String>,
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
            // A worktree or a submodule has a `.git` file rather than a
            // directory.
            if ancestor.join(MANIFEST).is_file() || ancestor.join(".git").exists() {
                return Package::at(ancestor).map(Some);
            }
        }
        Ok(None)
    }

    /// The package whose root is `dir`: its manifest, or the defaults
    /// without one. How a dependency is read, wherever it was found.
    pub fn at(dir: &Path) -> Result<Package, Error> {
        let manifest = dir.join(MANIFEST);
        if manifest.is_file() {
            return Package::read(&manifest);
        }
        let root = std::path::absolute(dir).map_err(|err| Error::Io(dir.to_path_buf(), err))?;
        Ok(Package::bare(clean(&root)))
    }

    /// Reads the manifest at `path`. The directory holding it is the root.
    pub fn read(path: &Path) -> Result<Package, Error> {
        let text =
            std::fs::read_to_string(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        let wrong = |what: String| Error::Manifest(path.to_path_buf(), what);
        let File {
            package,
            features,
            dependencies,
            fmt,
            lint,
        } = toml::from_str(&text).map_err(|err| wrong(err.to_string().trim_end().to_string()))?;

        let absolute =
            std::path::absolute(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        let root = clean(&absolute)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();

        let checked = |key: &str, written: &Path| -> Result<PathBuf, Error> {
            let Some(full) = inside(&root, written) else {
                return Err(wrong(format!(
                    "`{key}`: `{}` is outside the package",
                    written.display()
                )));
            };
            // Caught here, since a misspelt source would otherwise make a
            // walk that finds nothing.
            if !full.exists() {
                return Err(wrong(format!(
                    "`{key}`: `{}` does not exist",
                    written.display()
                )));
            }
            Ok(full)
        };
        let sources = (package.sources.iter())
            .map(|path| checked("sources", path))
            .collect::<Result<_, _>>()?;
        let include_dirs = match &package.include_dirs {
            Some(dirs) => (dirs.iter())
                .map(|path| checked("include-dirs", path))
                .collect::<Result<_, _>>()?,
            None => default_include_dirs(&root),
        };
        let (features, default_features) = features::read(features, &wrong, &checked)?;
        let dependencies = (dependencies.into_iter())
            .map(|(name, entry)| (name, entry.features))
            .collect();
        let package = Package {
            enabled: features.keys().cloned().collect(),
            features,
            default_features,
            dependencies,
            root,
            manifest: Some(path.to_path_buf()),
            name: package.name,
            sources,
            include_dirs,
            exclude: package.exclude,
            dev: package.dev,
            fmt,
            lint,
        };
        // Checked now, so that a wrong pattern is reported whether or not a
        // walk follows.
        patterns(&package.manifest, "exclude", &package.exclude)?;
        patterns(&package.manifest, "dev", &package.dev)?;
        for (name, feature) in &package.features {
            let key = format!("features.{name}.sources");
            patterns(&package.manifest, &key, &feature.sources)?;
        }
        Ok(package)
    }

    /// The package as a dependent sees it: without its `dev` files, which
    /// its walk then skips as it skips what `exclude` names, and with only
    /// its `default` features until a design selects more. So a dependency's
    /// testbench is neither compiled nor found by an include.
    pub fn as_dependency(mut self) -> Package {
        self.exclude.append(&mut self.dev);
        self.enabled = self.default_features.iter().cloned().collect();
        self
    }

    /// A package with no manifest, which takes every default.
    fn bare(root: PathBuf) -> Package {
        Package {
            sources: vec![root.clone()],
            include_dirs: default_include_dirs(&root),
            root,
            manifest: None,
            name: None,
            exclude: Vec::new(),
            dev: Vec::new(),
            features: BTreeMap::new(),
            default_features: Vec::new(),
            dependencies: BTreeMap::new(),
            enabled: BTreeSet::new(),
            fmt: toml::Table::new(),
            lint: toml::Table::new(),
        }
    }
}

/// `include` under `root` if it is a directory, which is where most IP keeps
/// the headers it exports.
fn default_include_dirs(root: &Path) -> Vec<PathBuf> {
    let include = root.join("include");
    match include.is_dir() {
        true => vec![include],
        false => Vec::new(),
    }
}

/// `written`, relative to `root`, as an absolute path inside it. An
/// absolute path replaces the root when joined, and `..` past it cleans to
/// something the root does not prefix.
fn inside(root: &Path, written: &Path) -> Option<PathBuf> {
    let full = clean(&root.join(written));
    full.starts_with(root).then_some(full)
}

/// The manifest as written.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct File {
    package: Section,
    /// Each entry a feature's table, but `default`, a list; told apart once
    /// read, so that a mistake in one names it.
    features: toml::Table,
    dependencies: BTreeMap<String, DependencyEntry>,
    fmt: toml::Table,
    lint: toml::Table,
}

/// The `[package]` table.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Section {
    name: Option<String>,
    sources: Vec<PathBuf>,
    /// `None` when not written, so that the default applies, where an empty
    /// list exports nothing.
    include_dirs: Option<Vec<PathBuf>>,
    exclude: Vec<String>,
    dev: Vec<String>,
}

/// An entry of `[dependencies]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DependencyEntry {
    #[serde(default)]
    features: Vec<String>,
}

impl Default for Section {
    fn default() -> Section {
        Section {
            name: None,
            sources: vec![PathBuf::from(".")],
            include_dirs: None,
            exclude: Vec::new(),
            dev: Vec::new(),
        }
    }
}
