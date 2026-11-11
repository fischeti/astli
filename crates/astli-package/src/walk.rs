//! The files a package holds.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::{Error, MANIFEST, Package, Source, mark};

/// The extensions of the files a walk yields: units, and headers written to
/// be included. A header of any other extension is found by being included.
const EXTENSIONS: [&str; 4] = ["sv", "svh", "v", "vh"];

impl Package {
    /// The SystemVerilog files the package holds, absolute and sorted: those
    /// under its `sources`, less what a `.gitignore` or `exclude` names and
    /// what a package nested in it, one it depends on by `path`, or one
    /// patched to a directory inside it holds.
    ///
    /// Only the `.gitignore` files at or below the root apply, whether or not
    /// the root is in a repository, so that a dependency reads the same from
    /// a shared store as from its own checkout, and a package checked out
    /// where an enclosing repository ignores it is still walked. A global
    /// ignore file and `.git/info/exclude` do not apply, since they differ
    /// between machines. A virtual workspace root holds none.
    pub fn walk(&self) -> Result<Vec<PathBuf>, Error> {
        if self.is_virtual {
            return Ok(Vec::new());
        }
        let filter = Arc::new(Filter {
            root: self.root.clone(),
            sources: self.sources.clone(),
            exclude: patterns(&self.manifest, "exclude", &self.exclude)?,
            enabled: self.feature_sources(true)?,
            disabled: self.feature_sources(false)?,
            dependencies: (self.dependencies.values())
                .filter_map(|requirement| match &requirement.source {
                    Some(Source::Path(dir)) => Some(dir.clone()),
                    _ => None,
                })
                .chain(self.patches.0.values().cloned())
                .collect(),
        });
        let entries = Arc::clone(&filter);
        let mut builder = WalkBuilder::new(&self.root);
        builder
            .standard_filters(false)
            .git_ignore(true)
            .require_git(false)
            .filter_entry(move |entry| {
                let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
                entries.keeps(entry.path(), is_dir)
            });

        // On every core, since a large package is thousands of directories
        // to read, and sorted after, so that the order is the paths'. Of
        // several problems, the first by its message is told, the same one
        // every run.
        let found = Mutex::new((Vec::new(), Vec::new()));
        builder.build_parallel().run(|| {
            Box::new(|entry| {
                let kept = (entry.map_err(|err| err.to_string()))
                    .and_then(|entry| self.kept(entry, &filter));
                let mut found = found.lock().expect("a walk thread panicked");
                match kept {
                    Ok(Some(path)) => found.0.push(path),
                    Ok(None) => {}
                    Err(problem) => found.1.push(problem),
                }
                WalkState::Continue
            })
        });
        let (mut files, problems) = found.into_inner().expect("a walk thread panicked");
        if let Some(problem) = problems.into_iter().min() {
            return Err(Error::Walk(problem));
        }
        files.sort();
        Ok(files)
    }

    /// The file `entry` is, if the walk yields it: a SystemVerilog file the
    /// features select. A problem if it is a file of a checkout no commit
    /// holds.
    fn kept(&self, entry: DirEntry, filter: &Filter) -> Result<Option<PathBuf>, String> {
        let path = entry.path();
        if let Some(held) = &self.held
            && entry.file_type().is_some_and(|kind| !kind.is_dir())
            && !held.contains(path)
        {
            return Err(format!(
                "{}: not a file of the commit checked out here, so put there by hand, \
                 which changes the package for every design that uses it. Generate \
                 into the package that needs the file, and override what it replaces",
                path.display()
            ));
        }
        let wanted = (path.extension())
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| EXTENSIONS.contains(&extension));
        // Follows a link, which the walk itself does not.
        Ok((wanted && path.is_file() && filter.selected(path)).then(|| entry.into_path()))
    }

    /// The `sources` of the features enabled, or of those not.
    fn feature_sources(&self, enabled: bool) -> Result<GlobSet, Error> {
        let sources: Vec<String> = (self.features.iter())
            .filter(|(name, _)| self.enabled.contains(*name) == enabled)
            .flat_map(|(_, feature)| feature.sources.iter().cloned())
            .collect();
        patterns(&self.manifest, "features", &sources)
    }
}

impl Package {
    /// Which of its own patterns name each file, for a manifest that has to
    /// say under which conditions a file is built.
    pub fn naming(&self) -> Result<Naming, Error> {
        let features = (self.features.iter())
            .map(|(name, feature)| {
                let key = format!("features.{name}.sources");
                Ok((
                    name.clone(),
                    patterns(&self.manifest, &key, &feature.sources)?,
                ))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Naming {
            root: self.root.clone(),
            features,
            dev: patterns(&self.manifest, "dev", &self.dev)?,
        })
    }
}

/// Which of a package's patterns name a file: each feature's `sources`, and
/// `dev`.
pub struct Naming {
    root: PathBuf,
    features: Vec<(String, GlobSet)>,
    dev: GlobSet,
}

impl Naming {
    /// The features whose `sources` name `path`, by name, sorted.
    pub fn features(&self, path: &Path) -> Vec<&str> {
        (self.features.iter())
            .filter(|(_, set)| named(&self.root, set, path))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Whether `dev` names `path`.
    pub fn dev(&self, path: &Path) -> bool {
        named(&self.root, &self.dev, path)
    }
}

/// Whether `set` names `path`, or a directory holding it, relative to `root`.
pub(crate) fn named(root: &Path, set: &GlobSet, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    (relative.ancestors())
        .take_while(|it| !it.as_os_str().is_empty())
        .any(|it| set.is_match(it))
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
            .map_err(|err| {
                wrong(format!(
                    "{}: {}: {err}",
                    mark::code(key),
                    mark::code(pattern)
                ))
            })?;
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
    /// The sources of the enabled features, and of the others.
    enabled: GlobSet,
    disabled: GlobSet,
    /// The directories of the packages it depends on by `path` or reads
    /// from a patch, absolute.
    dependencies: Vec<PathBuf>,
}

impl Filter {
    /// Whether a file the walk found is in the features selected: one that
    /// only disabled features name is not. Decided per file rather than per
    /// directory, so that a file two features name is kept when either is
    /// enabled.
    fn selected(&self, path: &Path) -> bool {
        !named(&self.root, &self.disabled, path) || named(&self.root, &self.enabled, path)
    }

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
        // A dependency, which has a manifest only if it was written for
        // astli, holds its own files, as does a clone patched in.
        if is_dir && self.dependencies.iter().any(|dir| dir == path) {
            return false;
        }
        // Under a source, or a directory on the way to one.
        (self.sources.iter())
            .any(|source| path.starts_with(source) || (is_dir && source.starts_with(path)))
    }
}
