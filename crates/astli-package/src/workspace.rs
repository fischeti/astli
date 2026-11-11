//! The workspace a package is in: the packages of one repository that share
//! a lock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::GlobSet;
use ignore::WalkBuilder;

use crate::walk::{named, patterns};
use crate::{Error, MANIFEST, Package, Patches, mark};

/// A package whose manifest holds `[workspace]`, and every package below it
/// in its repository, which it resolves as one: they share its lock, and
/// what else only a root's manifest says.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// The package holding `[workspace]`, read as a root is, its patches
    /// too.
    pub root: Package,
    /// Every other package in the workspace, sorted by root. Each is read
    /// as a root is, but for its patches, since only the workspace root's
    /// count.
    pub members: Vec<Package>,
}

impl Workspace {
    /// The workspace `package` is in, with its members: the nearest
    /// manifest at or above `package`'s root that holds `[workspace]`,
    /// looked for as far as the root of the repository `package` is in.
    /// `None` without one, or if it does not count `package` a member.
    ///
    /// A `[workspace]` above that one, in the same repository, is an error
    /// unless it excludes it: a workspace has one level. So is a
    /// `[workspace]` among the members, and two members of one name.
    pub fn find(package: &Package) -> Result<Option<Workspace>, Error> {
        let roots = workspaces_above(&package.root)?;
        let Some(nearest) = roots.first() else {
            return Ok(None);
        };
        let mut root = Package::read(nearest)?;
        if let Some(outer) = roots.get(1) {
            let outer = Package::read(outer)?;
            if !excluded(&outer, &root.root)? {
                return Err(nested(&outer, &root));
            }
        }
        if excluded(&root, &package.root)? {
            return Ok(None);
        }
        let members = members(&root)?;
        // A directory the walk does not enter, such as one `.gitignore`
        // names, holds no member.
        if package.root != root.root && !members.iter().any(|it| it.root == package.root) {
            return Ok(None);
        }
        root.patches = Patches::read(&root.root)?;
        Ok(Some(Workspace { root, members }))
    }

    /// Every package in the workspace, the root first.
    pub fn packages(&self) -> impl Iterator<Item = &Package> {
        std::iter::once(&self.root).chain(&self.members)
    }
}

/// The manifests at or above `dir` that hold `[workspace]`, nearest first,
/// as far as the root of the repository `dir` is in, which a `.git` marks.
/// Any other manifest is left unread, so that a package in no workspace is
/// read as it would be without one.
fn workspaces_above(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut found = Vec::new();
    for ancestor in dir.ancestors() {
        let manifest = ancestor.join(MANIFEST);
        if manifest.is_file() {
            let text = std::fs::read_to_string(&manifest)
                .map_err(|err| Error::Io(manifest.clone(), err))?;
            let table: toml::Table =
                toml::from_str(&text).map_err(|err| Error::unparsed(&manifest, err))?;
            if table.contains_key("workspace") {
                found.push(manifest);
            }
        }
        if ancestor.join(".git").exists() {
            break;
        }
    }
    Ok(found)
}

/// Whether the workspace rooted at `root` excludes the directory `dir`.
fn excluded(root: &Package, dir: &Path) -> Result<bool, Error> {
    Ok(named(&root.root, &exclusions(root)?, dir))
}

/// The patterns of `root`'s `[workspace] exclude`.
fn exclusions(root: &Package) -> Result<GlobSet, Error> {
    let exclude = root.workspace.as_deref().unwrap_or_default();
    patterns(&root.manifest, "workspace.exclude", exclude)
}

/// The members of the workspace rooted at `root`: the package at each
/// directory below it holding an `astli.toml`, but in a directory that
/// `[workspace] exclude` or a `.gitignore` names, or that holds a `.git` of
/// its own, as a submodule does, and so is another repository. A member may
/// be inside another.
fn members(root: &Package) -> Result<Vec<Package>, Error> {
    let exclude = exclusions(root)?;
    let top = root.root.clone();
    let mut builder = WalkBuilder::new(&root.root);
    // As the walk for a package's files reads `.gitignore`, which is also
    // what keeps a patched clone in `.astli` and a bender checkout out.
    builder
        .standard_filters(false)
        .git_ignore(true)
        .require_git(false)
        .filter_entry(move |entry| {
            let path = entry.path();
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            let relative = path.strip_prefix(&top).unwrap_or(path);
            path == top
                || (is_dir
                    && path.file_name().is_none_or(|name| name != ".git")
                    && !path.join(".git").exists()
                    && !exclude.is_match(relative))
        });
    let mut members = Vec::new();
    for entry in builder.build() {
        let entry = entry.map_err(|err| Error::Walk(err.to_string()))?;
        let manifest = entry.path().join(MANIFEST);
        if entry.depth() == 0 || !manifest.is_file() {
            continue;
        }
        let member = Package::read(&manifest)?;
        if member.workspace.is_some() {
            return Err(nested(root, &member));
        }
        members.push(member);
    }
    members.sort_by(|a, b| a.root.cmp(&b.root));

    let mut names: BTreeMap<&str, &Path> = BTreeMap::new();
    for package in std::iter::once(root).chain(&members) {
        let Some(name) = &package.name else {
            continue;
        };
        if let Some(first) = names.insert(name, &package.root) {
            return Err(Error::Manifest(
                manifest_of(package),
                format!(
                    "{} is also the name of the package at {}, and a workspace holds one \
                     package of a name",
                    mark::name(name),
                    mark::path(first.display())
                ),
            ));
        }
    }
    Ok(members)
}

/// Why the workspace rooted at `inner` may not be where it is.
fn nested(outer: &Package, inner: &Package) -> Error {
    Error::Manifest(
        manifest_of(inner),
        format!(
            "{} inside the workspace at {}, which does not exclude it: a workspace \
             holds no other",
            mark::code("[workspace]"),
            mark::path(outer.root.display())
        ),
    )
}

/// The manifest of a package found by its manifest.
fn manifest_of(package: &Package) -> PathBuf {
    (package.manifest.clone()).unwrap_or_else(|| package.root.join(MANIFEST))
}
