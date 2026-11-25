//! The package a command acts on, as `-p`, `--workspace` or the current
//! directory chooses it, and the workspace it is in, whose root holds what
//! its packages share. `cmd/workspace.rs` turns the flags into a choice, and
//! runs a command once for each package of a workspace.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

use astli_package::{MANIFEST, Package, Patches, Root, Workspace, mark};
use astli_resolve::{LOCKFILE, Overrides};

use crate::error::{Error, Result};
use crate::run::Run;
use crate::term;

/// The package the current directory is in, and its workspace, for a
/// command that acts on the workspace from any of its packages.
pub fn here(run: &Run) -> Result<Here> {
    Target::current()?.here(run)
}

/// The package a command acts on, as `-p`, `--workspace` or the current
/// directory chose it. It is found only when the command asks for it, since
/// a command reading files named may never need it, and then once.
pub struct Target {
    /// The chosen package's root, or the current directory, at or above
    /// which the package is.
    dir: PathBuf,
    /// The package, once found: `None` if there is none.
    found: OnceCell<Option<Root>>,
    /// The workspace the package is in, once found or as the choice found
    /// it: `None` if it is in none. Kept, since finding it reads every
    /// member's manifest, and a run asks for it more than once.
    workspace: OnceCell<Option<Workspace>>,
}

impl Target {
    /// The package at or above `dir`.
    pub fn new(dir: PathBuf) -> Target {
        Target {
            dir,
            found: OnceCell::new(),
            workspace: OnceCell::new(),
        }
    }

    /// `package`, as the choice found it among `workspace`'s.
    pub fn member(package: Root, workspace: Workspace) -> Target {
        Target {
            dir: package.package.root.clone(),
            found: OnceCell::from(Some(package)),
            workspace: OnceCell::from(Some(workspace)),
        }
    }

    /// The package the current directory is in.
    pub fn current() -> Result<Target> {
        let dir = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        Ok(Target::new(dir))
    }

    /// The package, if there is one.
    pub fn package(&self) -> Result<Option<Root>> {
        if let Some(found) = self.found.get() {
            return Ok(found.clone());
        }
        let found = Root::find(&self.dir)?;
        Ok(self.found.get_or_init(|| found).clone())
    }

    /// Whether the package is a virtual workspace root.
    pub fn is_virtual(&self) -> Result<bool> {
        Ok(self.package()?.is_some_and(|it| it.is_virtual()))
    }

    /// The workspace the package is in, if any.
    pub fn workspace(&self) -> Result<Option<Workspace>> {
        if let Some(found) = self.workspace.get() {
            return Ok(found.clone());
        }
        let found = match self.package()? {
            Some(package) => Workspace::find(&package)?,
            None => None,
        };
        Ok(self.workspace.get_or_init(|| found).clone())
    }

    /// The package and its workspace, for a command that works on its
    /// dependencies. Without a package it is an error.
    pub fn here(&self, run: &Run) -> Result<Here> {
        let package = self.package()?.ok_or_else(|| {
            Error::failed("no package here: run inside a git repository or beside an astli.toml")
        })?;
        self.here_of(run, package)
    }

    /// `package`, as [`Target::package`] found it, and its workspace: the
    /// one the choice found, or else the one it is in.
    pub fn here_of(&self, run: &Run, package: Root) -> Result<Here> {
        Here::new(run, package, self.workspace()?)
    }
}

/// The workspace `target`'s package is in, for `-p`, which `flag` names.
/// Outside one it is an error.
pub fn workspace(target: &Target, flag: &str) -> Result<Workspace> {
    target.workspace()?.ok_or_else(|| {
        term::error(format!(
            "{} chooses among the packages of a workspace, and this is in none",
            mark::code(flag)
        ))
        .help(format!(
            "a {} table in an {MANIFEST} above makes one",
            mark::code("[workspace]")
        ))
        .into()
    })
}

/// The package of `workspace` called `name`, the root or a member.
pub fn named<'a>(workspace: &'a Workspace, name: &str) -> Result<&'a Root> {
    let names = || (workspace.packages()).filter_map(|it| it.package.manifest.name.as_ref());
    if let Some(package) =
        (workspace.packages()).find(|it| it.package.manifest.name.as_deref() == Some(name))
    {
        return Ok(package);
    }
    let names: Vec<String> = names().map(mark::name).collect();
    Err(term::error(format!(
        "no package of the workspace at {} is called {}",
        mark::path(workspace.root.package.root.display()),
        mark::name(name)
    ))
    .help(format!("its packages are {}", names.join(", ")))
    .into())
}

/// The package a command acts on, and the workspace it is in, whose root
/// holds what its packages share: the lock, the pins, the patches, the
/// descriptions and the build directory.
#[derive(Clone)]
pub struct Here {
    /// The package itself, with its own tables.
    pub package: Root,
    /// The workspace it is in, or outside one a workspace of it alone, so
    /// that its root is what holds the lock and the rest a workspace shares.
    pub workspace: Workspace,
    /// Whether the package is in a workspace rather than one of its own,
    /// which a message about what the root holds says.
    pub in_workspace: bool,
    /// The lock: the one the run names, or else the workspace root's.
    lockfile: PathBuf,
}

impl Here {
    /// `package` and the workspace it is in.
    pub fn of(run: &Run, package: Root) -> Result<Here> {
        let workspace = Workspace::find(&package)?;
        Here::new(run, package, workspace)
    }

    /// `package` and `workspace`, which it is in, if any. A member's
    /// manifest holding what only the workspace root's may is warned about,
    /// once a run.
    fn new(run: &Run, package: Root, workspace: Option<Workspace>) -> Result<Here> {
        let in_workspace = workspace.is_some();
        let workspace = match workspace {
            Some(workspace) => {
                run.warn_once(|| warn_unread(run, &workspace));
                workspace
            }
            None => Workspace::alone(package.clone())?,
        };
        let lockfile = (run.args.lockfile.clone())
            .unwrap_or_else(|| workspace.root.package.root.join(LOCKFILE));
        Ok(Here {
            package,
            workspace,
            in_workspace,
            lockfile,
        })
    }

    /// The lock the package resolves over.
    pub fn lockfile(&self) -> &Path {
        &self.lockfile
    }

    /// What a message calls `package`, the package or one of its
    /// workspace: the root outside a workspace, and in one, its name or
    /// else its directory.
    pub fn called(&self, package: &Package) -> String {
        match (self.in_workspace, &package.manifest.name) {
            (false, _) => "the root".to_string(),
            (true, Some(name)) => mark::name(name),
            (true, None) => mark::path(package.root.display()),
        }
    }

    /// What the workspace root says of how to resolve: its pins, and the
    /// patches beside its manifest.
    pub fn overrides(&self) -> Overrides {
        Overrides {
            pinned: self.workspace.root.tables.pinned.iter().cloned().collect(),
            patches: self.workspace.patches.clone(),
        }
    }
}

/// Warns of each table a member's manifest holds that only the workspace
/// root's is read for, and of a member's own patches; and of each table a
/// virtual root's holds that only a design is read for.
fn warn_unread(run: &Run, workspace: &Workspace) {
    let tables = |names: &[&str]| -> String {
        let marked: Vec<String> = (names.iter())
            .map(|name| mark::code(format!("[{name}]")))
            .collect();
        marked.join(", ")
    };
    let root = &workspace.root;
    if let Some(manifest) = &root.package.manifest_path
        && !root.unread.is_empty()
    {
        term::warning(format!(
            "{}: a virtual workspace root is no design, so astli does not read its {}",
            mark::path(manifest.display()),
            tables(&root.unread),
        ))
        .help(format!(
            "move them to a member, or make the root a package with {}",
            mark::code("[package]")
        ))
        .report(run);
    }
    for member in &workspace.members {
        if let Some(manifest) = &member.package.manifest_path
            && !member.unread.is_empty()
        {
            term::warning(format!(
                "{}: astli reads {} only from the workspace root's {MANIFEST}",
                mark::path(manifest.display()),
                tables(&member.unread),
            ))
            .help(format!(
                "move what is to count to {}",
                mark::path(root.package.root.join(MANIFEST).display())
            ))
            .report(run);
        }
        let patches = Patches::path(&member.package.root);
        if patches.is_file() {
            term::warning(format!(
                "{} is not read, since only the workspace root's patches count",
                mark::path(patches.display())
            ))
            .help(format!(
                "{} in the workspace patches its root",
                mark::code("astli patch")
            ))
            .report(run);
        }
    }
}
