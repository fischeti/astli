//! Finding a package's root, and reading its manifest.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use astli_text::clean;
use serde::Deserialize;

use crate::Error;
use crate::features::{self, Feature};
use crate::requirement::{AstliEntry, Requirement, Source, bender_dependencies};
use crate::walk::patterns;

/// The manifest's file name.
pub const MANIFEST: &str = "astli.toml";

/// The manifest bender reads, whose dependencies a package without an
/// `astli.toml` takes.
pub const BENDER_MANIFEST: &str = "Bender.yml";

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
    /// What it requires of each package it depends on, by name: its
    /// `[dependencies]`, or without that table, what its `Bender.yml` lists.
    /// A path is absolute.
    pub dependencies: BTreeMap<String, Requirement>,
    /// What only its own runs depend on, `[dev-dependencies]`: a dependent
    /// never resolves them. A path is absolute.
    pub dev_dependencies: BTreeMap<String, Requirement>,
    /// What `[overrides]` lets the package replace in each dependency, by
    /// its name: patterns over the top-level names it declares, where `*`
    /// matches any run of characters.
    pub overrides: BTreeMap<String, Vec<String>>,
    /// The features enabled: every one, until a design selects them.
    pub enabled: BTreeSet<String>,
    /// The `[fmt]` table, for the formatter's driver to read.
    pub fmt: toml::Table,
    /// The `[lint]` table, for the linter's driver to read.
    pub lint: toml::Table,
    /// The dependencies whose requirement, as this package writes it, is the
    /// only one, `[resolve] pinned`: what anything else asks of them is
    /// replaced. Only the root's count.
    pub pinned: Vec<String>,
    /// For a checkout in the shared store, every file its commit holds,
    /// absolute: a walk refuses any other, since it was put there by hand
    /// and would change the package for every design. `None` elsewhere.
    pub held: Option<BTreeSet<PathBuf>>,
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
        Package::bare(clean(&root))
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
            dev_dependencies,
            overrides,
            fmt,
            lint,
            resolve,
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
        let table = |table: &str, entries: BTreeMap<String, AstliEntry>| {
            let read = entries_read(table, entries).map_err(wrong)?;
            located(&root, table, read).map_err(wrong)
        };
        let dependencies = match dependencies {
            Some(entries) => table("dependencies", entries)?,
            None => listed_by_bender(&root)?,
        };
        let dev_dependencies = table("dev-dependencies", dev_dependencies)?;
        if let Some(name) = (dev_dependencies.keys()).find(|it| dependencies.contains_key(*it)) {
            return Err(wrong(format!(
                "`{name}` is in both `dependencies` and `dev-dependencies`"
            )));
        }
        brought_checked(&features, &dependencies).map_err(wrong)?;
        let package = Package {
            enabled: features.keys().cloned().collect(),
            features,
            default_features,
            dependencies,
            dev_dependencies,
            overrides,
            root,
            manifest: Some(path.to_path_buf()),
            name: package.name,
            sources,
            include_dirs,
            exclude: package.exclude,
            dev: package.dev,
            fmt,
            lint,
            pinned: resolve.pinned,
            held: None,
        };
        // Checked now, so that a wrong pattern is reported whether or not a
        // walk follows.
        patterns(&package.manifest, "exclude", &package.exclude)?;
        patterns(&package.manifest, "dev", &package.dev)?;
        for (dependency, names) in &package.overrides {
            let key = format!("overrides.{dependency}");
            patterns(&package.manifest, &key, names)?;
        }
        for (name, feature) in &package.features {
            let key = format!("features.{name}.sources");
            patterns(&package.manifest, &key, &feature.sources)?;
        }
        Ok(package)
    }

    /// The package as a dependent sees it: without its `dev` files, which
    /// its walk then skips as it skips what `exclude` names, without its
    /// dev-dependencies, and with only its `default` features until a design
    /// selects more. So a dependency's testbench is neither compiled nor
    /// found by an include.
    pub fn as_dependency(mut self) -> Package {
        self.exclude.append(&mut self.dev);
        self.dev_dependencies.clear();
        self.enabled = self.default_features.iter().cloned().collect();
        self
    }

    /// A package with no manifest, which takes every default, and its
    /// dependencies from its `Bender.yml`, if it has one.
    fn bare(root: PathBuf) -> Result<Package, Error> {
        let dependencies = listed_by_bender(&root)?;
        Ok(Package {
            sources: vec![root.clone()],
            include_dirs: default_include_dirs(&root),
            root,
            manifest: None,
            name: None,
            exclude: Vec::new(),
            dev: Vec::new(),
            features: BTreeMap::new(),
            default_features: Vec::new(),
            dependencies,
            dev_dependencies: BTreeMap::new(),
            overrides: BTreeMap::new(),
            enabled: BTreeSet::new(),
            fmt: toml::Table::new(),
            lint: toml::Table::new(),
            pinned: Vec::new(),
            held: None,
        })
    }
}

/// What the `Bender.yml` at `root` lists, or nothing without one.
fn listed_by_bender(root: &Path) -> Result<BTreeMap<String, Requirement>, Error> {
    let bender = root.join(BENDER_MANIFEST);
    if !bender.is_file() {
        return Ok(BTreeMap::new());
    }
    let text = std::fs::read_to_string(&bender).map_err(|err| Error::Io(bender.clone(), err))?;
    let wrong = |what| Error::Manifest(bender.clone(), what);
    located(
        root,
        "dependencies",
        bender_dependencies(&text).map_err(wrong)?,
    )
    .map_err(wrong)
}

/// The requirements of a table of `astli.toml` entries, named `table`.
fn entries_read(
    table: &str,
    entries: BTreeMap<String, AstliEntry>,
) -> Result<BTreeMap<String, Requirement>, String> {
    (entries.into_iter())
        .map(|(name, entry)| Ok((name.clone(), entry.read(table, &name)?)))
        .collect()
}

/// Refuses a feature that brings in what `dependencies` does not hold:
/// a feature only turns on a dependency the package lists.
fn brought_checked(
    features: &BTreeMap<String, Feature>,
    dependencies: &BTreeMap<String, Requirement>,
) -> Result<(), String> {
    for (name, feature) in features {
        if let Some((asked, _)) =
            (feature.dependencies.iter()).find(|(asked, _)| !dependencies.contains_key(asked))
        {
            return Err(format!(
                "`features.{name}.dependencies`: `{asked}` is not one of `dependencies`"
            ));
        }
    }
    Ok(())
}

/// `dependencies`, from the table named `table`, with each path made
/// absolute against `root`, the directory of the manifest that wrote it;
/// or why one is not a directory.
fn located(
    root: &Path,
    table: &str,
    mut dependencies: BTreeMap<String, Requirement>,
) -> Result<BTreeMap<String, Requirement>, String> {
    for (name, requirement) in &mut dependencies {
        if let Some(Source::Path(path)) = &mut requirement.source {
            let full = clean(&root.join(&*path));
            if !full.is_dir() {
                return Err(format!(
                    "`{table}.{name}.path`: `{}` is not a directory",
                    path.display()
                ));
            }
            *path = full;
        }
    }
    Ok(dependencies)
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
    /// `None` when not written, so that the `Bender.yml` beside it says,
    /// where an empty table depends on nothing.
    dependencies: Option<BTreeMap<String, AstliEntry>>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, AstliEntry>,
    overrides: BTreeMap<String, Vec<String>>,
    fmt: toml::Table,
    lint: toml::Table,
    resolve: Resolve,
}

/// The `[resolve]` table.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Resolve {
    pinned: Vec<String>,
}

/// What resolution reads of a package's manifests: what it depends on, and
/// what its features bring in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// By name, as [`Package::dependencies`] has them.
    pub dependencies: BTreeMap<String, Requirement>,
    /// What each feature brings in, by its name, as [`Feature::dependencies`]
    /// has it.
    pub features: BTreeMap<String, Vec<(String, Option<String>)>>,
    /// `[features] default`.
    pub default_features: Vec<String>,
}

impl Manifest {
    /// Whether the dependency `name` is optional: in the design only while
    /// a feature naming it alone, or another naming it, is enabled.
    pub fn optional(&self, name: &str) -> bool {
        (self.features.values().flatten())
            .any(|(asked, feature)| asked == name && feature.is_none())
    }
}

impl Package {
    /// What resolution reads of the package's manifest. Its
    /// dev-dependencies are not in it, since only the root's are resolved.
    pub fn manifest(&self) -> Manifest {
        Manifest {
            dependencies: self.dependencies.clone(),
            features: (self.features.iter())
                .map(|(name, feature)| (name.clone(), feature.dependencies.clone()))
                .collect(),
            default_features: self.default_features.clone(),
        }
    }
}

/// What a package at a commit depends on, from the text of its manifests
/// there: its `astli.toml`'s `[dependencies]`, or without that table, its
/// `Bender.yml`'s, and what its features bring in. A path stays as written.
/// Resolution reads a mirror this way, where there is no directory to read
/// a [`Package`] from; the message names the file at fault.
pub fn manifest(astli: Option<&str>, bender: Option<&str>) -> Result<Manifest, String> {
    let in_astli = |err: String| format!("{MANIFEST}: {err}");
    let mut file: File = match astli {
        Some(text) => {
            toml::from_str(text).map_err(|err| in_astli(err.to_string().trim_end().to_string()))?
        }
        None => File::default(),
    };
    let written = file.dependencies.take();
    let dependencies = match written {
        Some(entries) => entries_read("dependencies", entries).map_err(in_astli)?,
        None => match bender {
            Some(text) => {
                bender_dependencies(text).map_err(|err| format!("{BENDER_MANIFEST}: {err}"))?
            }
            None => BTreeMap::new(),
        },
    };
    // Its include directories are never looked at, so they are not checked.
    let wrong = |what: String| Error::Manifest(PathBuf::from(MANIFEST), what);
    let (features, default_features) =
        features::read(file.features, &wrong, &|_, path| Ok(path.to_path_buf()))
            .map_err(|err| err.to_string())?;
    brought_checked(&features, &dependencies).map_err(in_astli)?;
    Ok(Manifest {
        dependencies,
        features: (features.into_iter())
            .map(|(name, feature)| (name, feature.dependencies))
            .collect(),
        default_features,
    })
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
