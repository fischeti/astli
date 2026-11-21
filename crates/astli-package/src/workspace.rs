//! The workspace a package is in: the packages of one repository that share
//! a lock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::GlobSet;
use ignore::WalkBuilder;

use toml::de::DeTable;

use crate::walk::{named, patterns};
use crate::{Error, MANIFEST, Package, Patches, Root, mark};

/// A package whose manifest holds `[workspace]`, and every package below it
/// in its repository, which it resolves as one: they share its lock, and
/// what else only a root's manifest says.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// The package holding `[workspace]`.
    pub root: Root,
    /// Every other package in the workspace, sorted by root, each with the
    /// tables of its manifest only the workspace root's are read for as
    /// [`Root::unread`].
    pub members: Vec<Root>,
    /// The dependencies this machine reads from a directory of its own
    /// instead, as `astli patch` wrote them beside the root's manifest:
    /// they count for every package of the workspace, and a member's own
    /// are not read.
    pub patches: Patches,
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
    ///
    /// Each manifest is read once, `package`'s not again.
    pub fn find(package: &Root) -> Result<Option<Workspace>, Error> {
        let dir = &package.package.root;
        let mut roots = workspaces_above(package)?.into_iter();
        let Some(root) = roots.next() else {
            return Ok(None);
        };
        if let Some(outer) = roots.next()
            && !excluded(&outer, &root.package.root)?
        {
            return Err(nested(&outer, &root));
        }
        if excluded(&root, dir)? {
            return Ok(None);
        }
        let members = members(&root, package)?;
        // A directory the walk does not enter, such as one `.gitignore`
        // names, holds no member.
        if *dir != root.package.root && !members.iter().any(|it| it.package.root == *dir) {
            return Ok(None);
        }
        Workspace::new(root, members).map(Some)
    }

    /// The workspace of `package` alone, which holds the lock and the rest
    /// a workspace shares for a package in none.
    pub fn alone(package: Root) -> Result<Workspace, Error> {
        Workspace::new(package, Vec::new())
    }

    /// The workspace of `root` and `members`, with the patches beside the
    /// root's manifest.
    fn new(root: Root, members: Vec<Root>) -> Result<Workspace, Error> {
        let patches = Patches::read(&root.package.root)?;
        Ok(Workspace {
            root,
            members,
            patches,
        })
    }

    /// Every package in the workspace, the root first.
    pub fn packages(&self) -> impl Iterator<Item = &Root> {
        std::iter::once(&self.root).chain(&self.members)
    }
}

/// The roots at or above `package`'s whose manifests hold `[workspace]`,
/// nearest first, as far as the root of the repository it is in, which a
/// `.git` marks. Any other manifest is parsed but left unread, so that a
/// package in no workspace is read as it would be without one.
fn workspaces_above(package: &Root) -> Result<Vec<Root>, Error> {
    let mut found = Vec::new();
    for ancestor in package.package.root.ancestors() {
        let manifest = ancestor.join(MANIFEST);
        if package.package.manifest_path.as_ref() == Some(&manifest) {
            if package.workspace.is_some() {
                found.push(package.clone());
            }
        } else if manifest.is_file() {
            let text = std::fs::read_to_string(&manifest)
                .map_err(|err| Error::Io(manifest.clone(), err))?;
            let table = DeTable::parse(&text).map_err(|err| Error::unparsed(&manifest, err))?;
            if (table.get_ref().keys()).any(|key| key.get_ref() == "workspace") {
                found.push(Root::parsed(&manifest, table)?);
            }
        }
        if ancestor.join(".git").exists() {
            break;
        }
    }
    Ok(found)
}

/// Whether the workspace rooted at `root` excludes the directory `dir`.
fn excluded(root: &Root, dir: &Path) -> Result<bool, Error> {
    Ok(named(&root.package.root, &exclusions(root)?, dir))
}

/// The patterns of `root`'s `[workspace] exclude`.
fn exclusions(root: &Root) -> Result<GlobSet, Error> {
    let exclude = (root.workspace.as_ref()).map_or(&[][..], |it| &it.exclude);
    patterns(&root.package.manifest_path, "workspace.exclude", exclude)
}

/// The members of the workspace rooted at `root`: the package at each
/// directory below it holding an `astli.toml`, but in a directory that
/// `[workspace] exclude` or a `.gitignore` names, or that holds a `.git` of
/// its own, as a submodule does, and so is another repository. A member may
/// be inside another. `own`, the package the workspace was found from, is
/// taken as it was read.
fn members(root: &Root, own: &Root) -> Result<Vec<Root>, Error> {
    let exclude = exclusions(root)?;
    let top = root.package.root.clone();
    let mut builder = WalkBuilder::new(&top);
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
        let mut member = match own.package.manifest_path.as_ref() == Some(&manifest) {
            true => own.clone(),
            false => Root::read(&manifest)?,
        };
        if member.workspace.is_some() {
            return Err(nested(root, &member));
        }
        member.as_member();
        members.push(member);
    }
    members.sort_by(|a, b| a.package.root.cmp(&b.package.root));

    let mut names: BTreeMap<&str, &Path> = BTreeMap::new();
    for package in std::iter::once(&root.package).chain(members.iter().map(|it| &it.package)) {
        let Some(name) = &package.manifest.name else {
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
fn nested(outer: &Root, inner: &Root) -> Error {
    Error::Manifest(
        manifest_of(&inner.package),
        format!(
            "{} inside the workspace at {}, which does not exclude it: a workspace \
             holds no other",
            mark::code("[workspace]"),
            mark::path(outer.package.root.display())
        ),
    )
}

/// The manifest of a package found by its manifest.
fn manifest_of(package: &Package) -> PathBuf {
    (package.manifest_path.clone()).unwrap_or_else(|| package.root.join(MANIFEST))
}
