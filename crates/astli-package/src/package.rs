//! Finding a package's root, and reading its manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use astli_text::clean;
use serde::Deserialize;
use serde::de::IgnoredAny;
use toml::Spanned;
use toml::de::{DeTable, Deserializer};

use crate::build::{BuildSection, ProfileSection};
use crate::features::{self, Feature};
use crate::requirement::{AstliEntry, Requirement, Source, bender_dependencies};
use crate::walk::{literal, patterns};
use crate::{Description, Error, Profiles, Simulator, Tests, Top, mark};

/// The manifest's file name.
pub const MANIFEST: &str = "astli.toml";

/// The manifest bender reads, whose dependencies a package without an
/// `astli.toml` takes.
pub const BENDER_MANIFEST: &str = "Bender.yml";

/// A package's root and what its manifest says to a dependent: where its
/// files are, what it requires, and how its tools are configured.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// The directory every path in the manifest is relative to, absolute.
    pub root: PathBuf,
    /// Where its manifest is, if the root has one.
    pub manifest_path: Option<PathBuf>,
    /// What its manifest says, or the defaults without one. A description
    /// adds to its `exclude`, and a dependent sees its `dev` files there
    /// and none of its dev-dependencies.
    pub manifest: Manifest,
    /// The header roots a dependent searches, absolute and inside `root`:
    /// the manifest's `include-dirs`, or what a description says, or else
    /// `include`, where it exists.
    pub include_dirs: Vec<PathBuf>,
}

/// What a package's manifest says to a dependent, as written. A path in it
/// is relative to the directory holding it.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    /// What a dependent calls the package; needed once something depends on
    /// it.
    pub name: Option<String>,
    /// Patterns, relative to the root and matched as `exclude`'s are, for
    /// the design's files: those that neither `dev` nor a feature names.
    /// The root itself by default, or what a description says, and nothing
    /// in a virtual workspace root's.
    pub sources: Vec<String>,
    /// The header roots a dependent searches; `None` if not written, which
    /// leaves the default.
    pub include_dirs: Option<Vec<PathBuf>>,
    /// Patterns, relative to the root, for files the walk skips on top of
    /// those `.gitignore` names. `*` does not cross a `/`, and `**` does.
    pub exclude: Vec<String>,
    /// Patterns, relative to the root and matched as `exclude`'s are, for
    /// the files only the package itself reads, such as its testbenches: a
    /// dependent never sees them.
    pub dev: Vec<String>,
    /// What `[features]` declares, by name.
    pub features: BTreeMap<String, Feature>,
    /// The features a dependent gets without asking, `[features] default`.
    /// The package's own runs enable every feature instead.
    pub default_features: Vec<String>,
    /// What it requires of each package it depends on, by name: its
    /// `[dependencies]`, or without that table, what its `Bender.yml` lists.
    pub dependencies: BTreeMap<String, Requirement>,
    /// What only its own runs depend on, `[dev-dependencies]`: a dependent
    /// never resolves them.
    pub dev_dependencies: BTreeMap<String, Requirement>,
    /// What `[overrides]` lets the package replace in each dependency, by
    /// its name: patterns over the top-level names it declares, where `*`
    /// matches any run of characters.
    pub overrides: BTreeMap<String, Vec<String>>,
    /// The `[fmt]` table, for the formatter's driver to read.
    pub fmt: toml::Table,
    /// The `[lint]` table, for the linter's driver to read.
    pub lint: toml::Table,
    /// Which manifest lists its dependencies, which a mistake in one then
    /// names, and which a command editing them must change.
    pub listed_by: ListedBy,
}

/// Which manifest lists a package's dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListedBy {
    /// Its `astli.toml`, which has a `[dependencies]` table.
    Astli,
    /// Its `Bender.yml`, since it has no `[dependencies]` table.
    Bender,
    /// Neither: it has no `[dependencies]` table and no `Bender.yml`, or is a
    /// virtual workspace root, which depends on nothing.
    None,
}

/// What an empty manifest says, as does a package without one.
impl Default for Manifest {
    fn default() -> Manifest {
        Manifest {
            name: None,
            sources: vec![".".to_string()],
            include_dirs: None,
            exclude: Vec::new(),
            dev: Vec::new(),
            features: BTreeMap::new(),
            default_features: Vec::new(),
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            overrides: BTreeMap::new(),
            fmt: toml::Table::new(),
            lint: toml::Table::new(),
            listed_by: ListedBy::None,
        }
    }
}

/// Why the text of a package's manifests is not one, [marked](crate::mark).
#[derive(Debug)]
pub enum Malformed {
    /// The `astli.toml` does not parse, or not into a manifest's shape.
    Unparsed(toml::de::Error),
    /// The `astli.toml` names what it may not.
    Astli(String),
    /// The dependencies of the `Bender.yml` cannot be read.
    Bender(String),
}

impl Malformed {
    /// The error, for manifests read from `astli` and `bender`.
    fn at(self, astli: &Path, bender: &Path) -> Error {
        match self {
            Malformed::Unparsed(err) => Error::unparsed(astli, err),
            Malformed::Astli(what) => Error::Manifest(astli.to_path_buf(), what),
            Malformed::Bender(what) => Error::Manifest(bender.to_path_buf(), what),
        }
    }
}

impl Manifest {
    /// What the text `astli` of an `astli.toml` says to a dependent, with
    /// `bender`, the `Bender.yml` beside it if there is one, listing its
    /// dependencies if it has no `[dependencies]`. A path stays as written.
    /// Resolution reads a mirror this way, where there is no directory to
    /// read a [`Package`] from; an empty manifest is a package without one.
    pub fn parse(astli: &str, bender: Option<&str>) -> Result<Manifest, Malformed> {
        let file: DependencyFile = toml::from_str(astli).map_err(Malformed::Unparsed)?;
        Manifest::checked(file.design(), bender)
    }

    /// What `design` says, checked, or a virtual workspace root's if `None`.
    fn checked(design: Option<Design>, bender: Option<&str>) -> Result<Manifest, Malformed> {
        let Some(design) = design else {
            return Ok(Manifest {
                sources: Vec::new(),
                ..Manifest::default()
            });
        };
        let (features, default_features) =
            features::read(design.features).map_err(Malformed::Astli)?;
        let (listed_by, dependencies) = match (design.dependencies, bender) {
            (Some(entries), _) => (
                ListedBy::Astli,
                entries_read("dependencies", entries).map_err(Malformed::Astli)?,
            ),
            (None, Some(text)) => (
                ListedBy::Bender,
                bender_dependencies(text).map_err(Malformed::Bender)?,
            ),
            (None, None) => (ListedBy::None, BTreeMap::new()),
        };
        let dev_dependencies =
            entries_read("dev-dependencies", design.dev_dependencies).map_err(Malformed::Astli)?;
        if let Some(name) = (dev_dependencies.keys()).find(|it| dependencies.contains_key(*it)) {
            return Err(Malformed::Astli(format!(
                "{} is in both {} and {}",
                mark::name(name),
                mark::code("dependencies"),
                mark::code("dev-dependencies")
            )));
        }
        brought_checked(&features, &dependencies).map_err(Malformed::Astli)?;
        let section = design.section;
        Ok(Manifest {
            name: section.name,
            sources: section.sources,
            include_dirs: section.include_dirs,
            exclude: section.exclude,
            dev: section.dev,
            features,
            default_features,
            dependencies,
            dev_dependencies,
            overrides: design.overrides,
            fmt: design.fmt,
            lint: design.lint,
            listed_by,
        })
    }
}

/// What a manifest says that only a run at its package reads, never a
/// dependent: a dependency's would change the design of a package that
/// never chose it. In a workspace, `pinned`, `described`, `simulator` and
/// `profiles` are the workspace root's for every package of it; the rest
/// are each package's own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RootTables {
    /// The dependencies whose requirement, as this package writes it, is the
    /// only one, `[resolve] pinned`: what anything else asks of them is
    /// replaced.
    pub pinned: Vec<String>,
    /// What it says of each dependency without a manifest, by name.
    pub described: BTreeMap<String, Description>,
    /// What each `[top.NAME]` builds, by name.
    pub tops: BTreeMap<String, Top>,
    /// The top a run elaborates from when it names none, `[package]
    /// default-top`.
    pub default_top: Option<String>,
    /// The simulator a build drives, `[build] simulator`.
    pub simulator: Simulator,
    /// What a build under each profile asks of the simulator.
    pub profiles: Profiles,
    /// What applies to every test, `[test]`.
    pub tests: Tests,
}

/// The `[workspace]` table, which makes its package the root of a
/// [workspace](crate::Workspace).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceTable {
    /// Patterns, relative to the root and matched as `exclude`'s are, for
    /// the directories whose manifests are not its members.
    pub exclude: Vec<String>,
    /// Whether the manifest holds no `[package]` beside it, which makes the
    /// root virtual: it holds no files of its own, depends on nothing, and
    /// stands for its members.
    pub is_virtual: bool,
}

/// A package read as the root of a run: what its manifest says to a
/// dependent, and what it says to the run alone.
#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    /// What a dependent would read of it.
    pub package: Package,
    /// What only a run at the package reads.
    pub tables: RootTables,
    /// `[workspace]`, if the manifest holds it.
    pub workspace: Option<WorkspaceTable>,
    /// The tables the manifest holds that nothing reads, by name: in a
    /// virtual root's, those of a design, and in a member's, as
    /// [`Workspace::find`](crate::Workspace::find) finds it, those only the
    /// workspace root's are read for.
    pub unread: Vec<&'static str>,
    /// The tables the manifest holds, by name.
    written: Vec<String>,
}

/// The tables of a virtual root's manifest that nothing reads, since it is
/// no design: rather than resolved or applied to files named there alone,
/// they are left out, a `Bender.yml`'s dependencies too.
const DESIGN_TABLES: [&str; 8] = [
    "dependencies",
    "dev-dependencies",
    "features",
    "overrides",
    "top",
    "test",
    "fmt",
    "lint",
];

/// The tables of a member's manifest that nothing reads, since the
/// workspace root's are read for every package of it.
const SHARED_TABLES: [&str; 4] = ["resolve", "described", "build", "profile"];

impl Root {
    /// The package `dir` is in: the nearest directory at or above it that
    /// holds an `astli.toml`, or else the root of the git repository it is
    /// in. `None` if it is in neither.
    ///
    /// The search stops at the repository's root, since a manifest above it
    /// belongs to something the repository is not part of. A dependency in
    /// a shared store has no `.git`, which is why a manifest is looked for
    /// first.
    pub fn find(dir: &Path) -> Result<Option<Root>, Error> {
        let dir = std::path::absolute(dir).map_err(|err| Error::Io(dir.to_path_buf(), err))?;
        // Cleaned, so that the ancestors of `a/b/..` are those of `a`.
        let dir = clean(&dir);
        for ancestor in dir.ancestors() {
            // A worktree or a submodule has a `.git` file rather than a
            // directory.
            if ancestor.join(MANIFEST).is_file() || ancestor.join(".git").exists() {
                return Root::at(ancestor).map(Some);
            }
        }
        Ok(None)
    }

    /// The root package at `dir`: its manifest, or the defaults without
    /// one.
    pub fn at(dir: &Path) -> Result<Root, Error> {
        let manifest = dir.join(MANIFEST);
        if manifest.is_file() {
            return Root::read(&manifest);
        }
        let root = std::path::absolute(dir).map_err(|err| Error::Io(dir.to_path_buf(), err))?;
        Ok(Root {
            package: Package::bare(clean(&root))?,
            tables: RootTables::default(),
            workspace: None,
            unread: Vec::new(),
            written: Vec::new(),
        })
    }

    /// Reads the manifest at `path` as a root's. The directory holding it is
    /// the root.
    pub fn read(path: &Path) -> Result<Root, Error> {
        let text =
            std::fs::read_to_string(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        let table = DeTable::parse(&text).map_err(|err| Error::unparsed(path, err))?;
        Root::parsed(path, table)
    }

    /// The root whose manifest at `path` parses to `table`.
    pub(crate) fn parsed(path: &Path, table: Spanned<DeTable<'_>>) -> Result<Root, Error> {
        let written: Vec<String> = (table.get_ref().keys())
            .map(|key| key.get_ref().to_string())
            .collect();
        let File {
            package,
            features,
            dependencies,
            dev_dependencies,
            overrides,
            fmt,
            lint,
            resolve,
            described,
            top,
            test,
            build,
            profile,
            workspace,
        } = File::deserialize(Deserializer::from(table))
            .map_err(|err| Error::unparsed(path, err))?;
        let workspace = workspace.map(|table| WorkspaceTable {
            exclude: table.exclude,
            is_virtual: package.is_none(),
        });
        let is_virtual = workspace.as_ref().is_some_and(|it| it.is_virtual);
        let section = package.unwrap_or_default();
        let default_top = section.default_top.clone();
        let design = (!is_virtual).then_some(Design {
            section,
            features,
            dependencies,
            dev_dependencies,
            overrides,
            fmt,
            lint,
        });
        let package = Package::read_design(path, design)?;
        let tables = RootTables {
            pinned: resolve.pinned,
            described,
            tops: match is_virtual {
                true => BTreeMap::new(),
                false => top,
            },
            default_top,
            simulator: build.simulator,
            profiles: profile.read(),
            tests: match is_virtual {
                true => Tests::default(),
                false => test,
            },
        };
        for (name, top) in &tables.tops {
            let unknown = (top.features.iter().flatten())
                .find(|feature| !package.manifest.features.contains_key(*feature));
            if let Some(feature) = unknown {
                return Err(Error::Manifest(
                    path.to_path_buf(),
                    format!(
                        "{}: {} is not a feature of the package",
                        mark::code(format!("top.{name}.features")),
                        mark::name(feature)
                    ),
                ));
            }
        }
        // Checked now, as the package's own patterns are.
        for (name, description) in &tables.described {
            let key = format!("described.{name}.exclude");
            patterns(&package.manifest_path, &key, &description.exclude)?;
        }
        if let Some(workspace) = &workspace {
            patterns(
                &package.manifest_path,
                "workspace.exclude",
                &workspace.exclude,
            )?;
        }
        let unread = match is_virtual {
            true => tables_of(&written, &DESIGN_TABLES),
            false => Vec::new(),
        };
        Ok(Root {
            package,
            tables,
            workspace,
            unread,
            written,
        })
    }

    /// Whether it is a virtual workspace root.
    pub fn is_virtual(&self) -> bool {
        self.workspace.as_ref().is_some_and(|it| it.is_virtual)
    }

    /// Notes, for a member of a workspace, which of its tables only the
    /// workspace root's are read for.
    pub(crate) fn as_member(&mut self) {
        self.unread = tables_of(&self.written, &SHARED_TABLES);
    }
}

/// Which of `names` `written` holds.
fn tables_of(written: &[String], names: &[&'static str]) -> Vec<&'static str> {
    (names.iter().copied())
        .filter(|name| written.iter().any(|it| it == name))
        .collect()
}

/// The tables of a manifest that make a design, as written.
struct Design {
    section: Section,
    features: toml::Table,
    dependencies: Option<BTreeMap<String, AstliEntry>>,
    dev_dependencies: BTreeMap<String, AstliEntry>,
    overrides: BTreeMap<String, Vec<String>>,
    fmt: toml::Table,
    lint: toml::Table,
}

impl Package {
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

    /// Reads the manifest at `path` as a dependent does. The directory
    /// holding it is the root.
    pub fn read(path: &Path) -> Result<Package, Error> {
        let text =
            std::fs::read_to_string(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        let file: DependencyFile =
            toml::from_str(&text).map_err(|err| Error::unparsed(path, err))?;
        Package::read_design(path, file.design())
    }

    /// The directory holding the manifest at `path`, absolute.
    fn root_of(path: &Path) -> Result<PathBuf, Error> {
        let absolute =
            std::path::absolute(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        Ok(clean(&absolute)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default())
    }

    /// The package the manifest at `path` describes with `design`, or a
    /// virtual workspace root without one: its manifest checked, and the
    /// paths it names checked against the directory holding it.
    fn read_design(path: &Path, design: Option<Design>) -> Result<Package, Error> {
        let root = Package::root_of(path)?;
        let bender_path = root.join(BENDER_MANIFEST);
        let bender = match &design {
            Some(design) if design.dependencies.is_none() => bender_text(&bender_path)?,
            _ => None,
        };
        let manifest = Manifest::checked(design, bender.as_deref())
            .map_err(|err| err.at(path, &bender_path))?;
        let wrong = |what: String| Error::Manifest(path.to_path_buf(), what);
        let checked = |key: &str, written: &Path| -> Result<PathBuf, Error> {
            let Some(full) = inside(&root, written) else {
                return Err(wrong(format!(
                    "{}: {} is outside the package",
                    mark::code(key),
                    mark::path(written.display())
                )));
            };
            if let Some(why) = unfetched(&root, &full) {
                return Err(wrong(format!(
                    "{}: {} is in {why}",
                    mark::code(key),
                    mark::path(written.display())
                )));
            }
            // Caught here, since a misspelt source would otherwise make a
            // walk that finds nothing.
            if !full.exists() {
                return Err(wrong(format!(
                    "{}: {} does not exist",
                    mark::code(key),
                    mark::path(written.display())
                )));
            }
            Ok(full)
        };
        for pattern in &manifest.sources {
            checked_source(&root, pattern).map_err(|why| {
                let why = match why {
                    Unsourced::Outside => "is outside the package".to_string(),
                    Unsourced::Unfetched(why) => format!("is in {why}"),
                    Unsourced::Missing => "does not exist".to_string(),
                };
                wrong(format!(
                    "{}: {} {why}",
                    mark::code("sources"),
                    mark::code(pattern)
                ))
            })?;
        }
        let include_dirs = match &manifest.include_dirs {
            Some(dirs) => (dirs.iter())
                .map(|path| checked("include-dirs", path))
                .collect::<Result<_, _>>()?,
            None => default_include_dirs(&root),
        };
        for (name, feature) in &manifest.features {
            for dir in &feature.include_dirs {
                checked(&format!("features.{name}.include-dirs"), dir)?;
            }
        }
        let package = Package {
            root,
            manifest_path: Some(path.to_path_buf()),
            manifest,
            include_dirs,
        };
        package.located()?;
        // Checked now, so that a wrong pattern is reported whether or not a
        // walk follows.
        let (manifest, at) = (&package.manifest, &package.manifest_path);
        patterns(at, "sources", &manifest.sources)?;
        patterns(at, "exclude", &manifest.exclude)?;
        patterns(at, "dev", &manifest.dev)?;
        for (dependency, names) in &manifest.overrides {
            patterns(at, &format!("overrides.{dependency}"), names)?;
        }
        for (name, feature) in &manifest.features {
            patterns(at, &format!("features.{name}.sources"), &feature.sources)?;
        }
        Ok(package)
    }

    /// A package with no manifest, which takes every default, and its
    /// dependencies from its `Bender.yml`, if it has one.
    fn bare(root: PathBuf) -> Result<Package, Error> {
        let bender_path = root.join(BENDER_MANIFEST);
        let bender = bender_text(&bender_path)?;
        let manifest = Manifest::parse("", bender.as_deref())
            .map_err(|err| err.at(&root.join(MANIFEST), &bender_path))?;
        let package = Package {
            include_dirs: default_include_dirs(&root),
            root,
            manifest_path: None,
            manifest,
        };
        package.located()?;
        Ok(package)
    }

    /// Refuses a requirement whose path, relative to the root, is not a
    /// directory.
    fn located(&self) -> Result<(), Error> {
        let at = match self.manifest.listed_by {
            ListedBy::Bender => self.root.join(BENDER_MANIFEST),
            _ => self.manifest_path.clone().unwrap_or_default(),
        };
        let tables = [
            ("dependencies", &self.manifest.dependencies),
            ("dev-dependencies", &self.manifest.dev_dependencies),
        ];
        for (table, dependencies) in tables {
            for (name, requirement) in dependencies {
                if let Some(Source::Path(path)) = &requirement.source
                    && !clean(&self.root.join(path)).is_dir()
                {
                    return Err(Error::Manifest(
                        at,
                        format!(
                            "{}: {} is not a directory",
                            mark::code(format!("{table}.{name}.path")),
                            mark::path(path.display())
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The package as a dependent sees it: without its `dev` files, which
    /// its walk then skips as it skips what `exclude` names, and without its
    /// dev-dependencies. So a dependency's testbench is neither compiled nor
    /// found by an include.
    pub fn as_dependency(mut self) -> Package {
        let manifest = &mut self.manifest;
        manifest.exclude.append(&mut manifest.dev);
        manifest.dev_dependencies.clear();
        self
    }
}

/// The text of the `Bender.yml` at `path`, or `None` without one.
fn bender_text(path: &Path) -> Result<Option<String>, Error> {
    if !path.is_file() {
        return Ok(None);
    }
    (std::fs::read_to_string(path).map(Some)).map_err(|err| Error::Io(path.to_path_buf(), err))
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
                "{}: {} is not one of {}",
                mark::code(format!("features.{name}.dependencies")),
                mark::name(asked),
                mark::code("dependencies")
            ));
        }
    }
    Ok(())
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
/// Why a source names nothing: the fixed part of its pattern, before any
/// wildcard, is outside the package, in a submodule not checked out, which
/// the message says, or missing.
pub(crate) enum Unsourced {
    Outside,
    Unfetched(String),
    Missing,
}

/// Whether the source `pattern` of the package at `root` can name a file,
/// checked when the manifest is read, since a misspelt source would
/// otherwise make a walk that finds nothing.
pub(crate) fn checked_source(root: &Path, pattern: &str) -> Result<(), Unsourced> {
    let Some(full) = inside(root, &literal(pattern)) else {
        return Err(Unsourced::Outside);
    };
    if let Some(why) = unfetched(root, &full) {
        return Err(Unsourced::Unfetched(why));
    }
    if !full.exists() {
        return Err(Unsourced::Missing);
    }
    Ok(())
}

pub(crate) fn inside(root: &Path, written: &Path) -> Option<PathBuf> {
    let full = clean(&root.join(written));
    full.starts_with(root).then_some(full)
}

/// The manifest as a root reads it.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct File {
    /// `None` when not written, which with `[workspace]` makes a virtual
    /// root.
    package: Option<Section>,
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
    described: BTreeMap<String, Description>,
    top: BTreeMap<String, Top>,
    test: Tests,
    build: BuildSection,
    profile: ProfileSection,
    /// `None` when not written, since an empty table still makes a
    /// workspace.
    workspace: Option<WorkspaceSection>,
}

/// The manifest as a dependent reads it: what only a root reads is skipped
/// unread, since no dependent could fix a mistake in it.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct DependencyFile {
    package: Option<Section>,
    features: toml::Table,
    dependencies: Option<BTreeMap<String, AstliEntry>>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, AstliEntry>,
    overrides: BTreeMap<String, Vec<String>>,
    fmt: toml::Table,
    lint: toml::Table,
    resolve: IgnoredAny,
    described: IgnoredAny,
    top: IgnoredAny,
    test: IgnoredAny,
    build: IgnoredAny,
    profile: IgnoredAny,
    /// Whether it is written, which without `[package]` makes a virtual
    /// root.
    workspace: Option<IgnoredAny>,
}

impl DependencyFile {
    /// Its design, or `None` for a virtual workspace root, which has none.
    fn design(self) -> Option<Design> {
        if self.package.is_none() && self.workspace.is_some() {
            return None;
        }
        Some(Design {
            section: self.package.unwrap_or_default(),
            features: self.features,
            dependencies: self.dependencies,
            dev_dependencies: self.dev_dependencies,
            overrides: self.overrides,
            fmt: self.fmt,
            lint: self.lint,
        })
    }
}

/// The `[workspace]` table.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct WorkspaceSection {
    exclude: Vec<String>,
}

/// The `[resolve]` table.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Resolve {
    pinned: Vec<String>,
}

/// The `[package]` table.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Section {
    name: Option<String>,
    sources: Vec<String>,
    /// `None` when not written, so that the default applies, where an empty
    /// list exports nothing.
    include_dirs: Option<Vec<PathBuf>>,
    exclude: Vec<String>,
    dev: Vec<String>,
    default_top: Option<String>,
}

impl Default for Section {
    fn default() -> Section {
        Section {
            name: None,
            sources: vec![".".to_string()],
            include_dirs: None,
            exclude: Vec::new(),
            dev: Vec::new(),
            default_top: None,
        }
    }
}

/// Why `full`, a path the package at `root` names, is missing or an empty
/// directory, if it is in a submodule that is not checked out: which one,
/// and what checks it out. Read from the nearest `.gitmodules` at or above
/// `root`, as far as a `.git`: a git working copy has one, and a
/// dependency's checkout holds none, the root of its repository holding
/// its `.gitmodules`.
pub(crate) fn unfetched(root: &Path, full: &Path) -> Option<String> {
    if full.is_file() || full.is_dir() && !is_empty(full) {
        return None;
    }
    let top = (root.ancestors())
        .find(|dir| dir.join(".gitmodules").is_file() || dir.join(".git").exists())?;
    let text = std::fs::read_to_string(top.join(".gitmodules")).ok()?;
    for path in submodule_paths(&text) {
        let dir = top.join(path);
        if !full.starts_with(&dir) || dir.is_dir() && !is_empty(&dir) {
            continue;
        }
        let fetch = match top.join(".git").exists() {
            true => format!(
                "{} checks it out",
                mark::code("git submodule update --init")
            ),
            false => format!(
                "a requirement of the package with {} checks it out",
                mark::code(format!("submodules = [\"{path}\"]"))
            ),
        };
        return Some(format!(
            "the submodule {}, which is not checked out: {fetch}",
            mark::path(path)
        ));
    }
    None
}

/// The `path` of each submodule a `.gitmodules` file's `text` declares.
/// Read by hand rather than as git's configuration, since it only
/// explains an error: a path written with a quote or an escape is taken
/// as written, and at worst the error goes without the hint.
fn submodule_paths(text: &str) -> Vec<&str> {
    let mut paths = Vec::new();
    let mut in_submodule = false;
    for line in text.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[') {
            in_submodule = header.trim_start().starts_with("submodule");
        } else if in_submodule
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "path"
        {
            paths.push(value.trim().trim_matches('"'));
        }
    }
    paths
}

/// Whether the directory `dir` holds nothing, or cannot be read.
fn is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir).map_or(true, |mut it| it.next().is_none())
}
