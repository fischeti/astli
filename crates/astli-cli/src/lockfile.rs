//! Resolving the root's dependencies over `astli.lock`, and writing it, and
//! changing what the root's `astli.toml` requires. In a workspace the lock
//! is its root's, and resolves every member.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use astli_package::{
    Dependencies, Dependency, MANIFEST, Package, Patches, Root, Walk, Workspace, mark,
};
use astli_resolve::{LOCKFILE, Lock, Origin, Resolution, resolve};
use toml_edit::DocumentMut;

use crate::error::{Error, Result};
use crate::run::{Resolving, Run};
use crate::term::{self, status};

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
    /// The workspace the choice found the package in, if it looked.
    workspace: Option<Workspace>,
}

impl Target {
    /// The package at or above `dir`.
    pub fn new(dir: PathBuf) -> Target {
        Target {
            dir,
            found: OnceCell::new(),
            workspace: None,
        }
    }

    /// `package`, as the choice found it among `workspace`'s.
    pub fn member(package: Root, workspace: Workspace) -> Target {
        Target {
            dir: package.package.root.clone(),
            found: OnceCell::from(Some(package)),
            workspace: Some(workspace),
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
        if let Some(workspace) = &self.workspace {
            return Ok(Some(workspace.clone()));
        }
        Ok(match self.package()? {
            Some(package) => Workspace::find(&package)?,
            None => None,
        })
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
        let workspace = match &self.workspace {
            Some(workspace) => Some(workspace.clone()),
            None => Workspace::find(&package)?,
        };
        Here::new(run, package, workspace)
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
    let names = || (workspace.packages()).filter_map(|it| it.package.name.as_ref());
    if let Some(package) =
        (workspace.packages()).find(|it| it.package.name.as_deref() == Some(name))
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
        Ok(Here {
            package,
            workspace,
            in_workspace,
        })
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
    if let Some(manifest) = &root.package.manifest
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
        if let Some(manifest) = &member.package.manifest
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

/// Which of a resolution's warnings a run tells.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tell {
    /// Those of the packages the design of the package the run acts on
    /// holds, and every patch's: a member's run resolves the whole
    /// workspace, but builds only what its requirements reach.
    Design,
    /// Every one, for a command that acts on the whole workspace.
    Workspace,
}

/// When a resolution may write the lock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Write {
    /// Only where the root has an `astli.toml`, so that a run in a project
    /// only bender manages leaves no file behind. What a package command
    /// does.
    Manifest,
    /// Always, as `astli update` was asked to.
    Always,
}

/// Resolves the dependencies of `here`'s root and of each member of its
/// workspace, preferring what `kept` holds of the lock, and writes the lock
/// if it changed and `write` allows, telling the warnings `tell` asks for.
/// `locked` refuses a lock that is missing or would change instead. Each
/// change is a status line.
///
/// While the root patches a dependency, the lock is read and never written,
/// so that it holds what the design builds on once the patch is gone; and
/// `locked`, as CI asks, and [`Write::Always`] fail.
pub fn settle(
    run: &Run,
    here: &Here,
    kept: impl FnOnce(&Lock) -> Lock,
    write: Write,
    tell: Tell,
    locked: bool,
) -> Result<Resolution> {
    let root = &here.workspace.root;
    let path = root.package.root.join(LOCKFILE);
    let patched = here.workspace.patches.0.keys().map(mark::name);
    let patched = patched.collect::<Vec<_>>().join(", ");
    if !patched.is_empty() && (locked || write == Write::Always) {
        let asked = match locked {
            true => mark::code("--locked"),
            false => mark::code("astli update"),
        };
        return Err(term::error(format!(
            "{asked} needs a design without patches, and the root patches {patched}"
        ))
        .help(format!(
            "{} drops a patch",
            mark::code("astli patch --remove")
        ))
        .into());
    }
    let old = Lock::read(&path)?;
    let preferred = old.as_ref().map(kept).unwrap_or_default();
    let resolving = |preferred| Resolving {
        root: root.package.clone(),
        members: (here.workspace.members.iter())
            .map(|it| it.package.clone())
            .collect(),
        pinned: root.tables.pinned.iter().cloned().collect(),
        patches: here.workspace.patches.clone(),
        preferred,
    };
    let resolution = run.resolution(resolving(preferred), |it| {
        Ok(resolve(
            &run.store()?,
            &it.root,
            &it.members,
            &it.pinned,
            &it.patches,
            &it.preferred,
        )?)
    })?;
    let held = held(&here.package.package, &resolution);
    for warning in &resolution.warnings {
        let told = match (&warning.of, tell) {
            (Some(of), Tell::Design) => held.contains(of),
            _ => true,
        };
        if told {
            term::warning(&warning.message).report(run);
        }
    }
    for name in root.tables.described.keys() {
        if !resolution.packages.iter().any(|it| it.name == *name) {
            term::warning(format!(
                "{} describes a package the design does not hold",
                mark::code(format!("[described.{name}]"))
            ))
            .report(run);
        }
    }

    if !patched.is_empty() {
        return Ok(resolution);
    }
    let new = Lock::of(&resolution);
    // Nothing to lock is the same as no lock, so a package without
    // dependencies gets no file.
    let changed = match &old {
        Some(old) => *old != new,
        None => !new.packages.is_empty(),
    };
    if !changed {
        return Ok(resolution);
    }
    if locked {
        let why = match old {
            Some(_) => "would change",
            None => "is missing",
        };
        let mut error = term::error(format!(
            "{} {why}, and {} keeps it as it is",
            mark::path(path.display()),
            mark::code("--locked")
        ));
        for uncached in &resolution.uncached {
            error = error.note(uncached);
        }
        let help = match resolution.uncached.is_empty() {
            true => format!("{} writes it", mark::code("astli update")),
            false => format!(
                "{} without {} puts what it lacks in the cache",
                mark::code("astli fetch"),
                mark::code("--offline")
            ),
        };
        return Err(error.help(help).into());
    }
    for uncached in &resolution.uncached {
        term::warning(format!("{uncached}, so another version is locked")).report(run);
    }
    if write == Write::Manifest && root.package.manifest.is_none() {
        return Ok(resolution);
    }
    for (verb, what) in old.unwrap_or_default().changes(&new) {
        status!(verb, "{what}");
    }
    std::fs::write(&path, new.to_string()).map_err(|err| Error::io(&path, err))?;
    // Every commit locked is one the requirements admit, so a resolution
    // preferring the lock written chooses them all again: the next
    // package's run, reading it, needs no resolution of its own.
    run.resolved(resolving(new), &resolution);
    Ok(resolution)
}

/// The dependencies of `here`'s package, each checked out and read as a
/// dependent sees it, resolved over the lock, which is written as
/// [`Write::Manifest`] says. In a workspace the whole workspace is
/// resolved, so that the lock is the same wherever it is written, and the
/// package's design is the part of it the package reaches. A dependency
/// without a manifest is read as the root describes it, and the described
/// ones are returned by name, for [`described`]. So are those a `path`
/// requirement reads from a directory on disk, directly or through another,
/// a sibling member among them: not a patched one, nor a directory of a
/// fetched repository, nor one only a patched one reaches, which is that
/// repository's too.
pub fn dependencies(
    run: &Run,
    here: &Here,
    locked: bool,
) -> Result<(Dependencies, Vec<String>, BTreeSet<String>)> {
    let resolution = settle(
        run,
        here,
        Lock::clone,
        Write::Manifest,
        Tell::Design,
        locked,
    )?;
    let package = &here.package.package;
    let root = &here.workspace.root;
    let reached = held(package, &resolution);
    let mut on_disk = BTreeSet::new();
    let mut from_disk: Vec<&String> = (package.dependencies.keys())
        .chain(package.dev_dependencies.keys())
        .collect();
    while let Some(name) = from_disk.pop() {
        if let Some(it) = resolution.packages.iter().find(|it| it.name == *name)
            && matches!(it.origin, Origin::Dir(_))
            && on_disk.insert(name.clone())
        {
            from_disk.extend(&it.needs);
        }
    }
    let store = run.store()?;
    let mut found = Vec::new();
    let mut described = Vec::new();
    for it in resolution.packages {
        if !reached.contains(&it.name) {
            continue;
        }
        let named = |err: String| Error::failed(format!("{}: {err}", mark::name(&it.name)));
        let located = it.checkout(&store).map_err(|err| named(err.message()))?;
        let mut package = Package::at(&located.dir).map_err(|err| named(err.message()))?;
        let description = root.tables.described.get(&it.name);
        if let (Some(description), Some(manifest)) = (description, &root.package.manifest) {
            match package.manifest {
                Some(_) => term::warning(format!(
                    "{} has its own {MANIFEST}, which is read instead of {}",
                    mark::name(&it.name),
                    mark::code(format!("[described.{}]", it.name))
                ))
                .help("remove the description")
                .report(run),
                None => {
                    if let Some(held) = &located.held {
                        for pattern in description.unmatched(&package.root, held) {
                            term::warning(format!(
                                "{}: {} matches none of its files",
                                mark::code(format!("described.{}.exclude", it.name)),
                                mark::code(pattern)
                            ))
                            .report(run);
                        }
                    }
                    package = package.described(&it.name, description, manifest)?;
                    described.push(it.name.clone());
                }
            }
        }
        found.push(Dependency {
            name: it.name,
            package: package.as_dependency(),
            needs: it.needs,
            walk: Walk {
                held: located.held,
                ..Walk::default()
            },
        });
    }
    Ok((Dependencies::ordered(package, found)?, described, on_disk))
}

/// The names of the packages of `resolution` that `package`'s design holds:
/// what its requirements reach.
pub fn held(package: &Package, resolution: &Resolution) -> BTreeSet<String> {
    let mut open: Vec<&String> = (package.dependencies.keys())
        .chain(package.dev_dependencies.keys())
        .collect();
    let mut reached = BTreeSet::new();
    while let Some(name) = open.pop() {
        if reached.insert(name.clone())
            && let Some(it) = resolution.packages.iter().find(|it| it.name == *name)
        {
            open.extend(&it.needs);
        }
    }
    reached
}

/// Notes the dependencies read as the root describes them, if any, the
/// workspace's root if `in_workspace`, which a member's run would
/// otherwise read as itself. Only the commands about dependencies say so:
/// elsewhere it would read as a finding about the design.
pub fn described(run: &Run, names: &[String], in_workspace: bool) {
    let named: Vec<String> = names.iter().map(mark::name).collect();
    let (what, sections) = match names {
        [] => return,
        [name] => (
            format!("dependency {} has", named[0]),
            mark::code(format!("[described.{name}]")),
        ),
        _ => (
            format!("dependencies {} have", named.join(", ")),
            format!("their {} sections", mark::code("[described]")),
        ),
    };
    let from = match in_workspace {
        true => format!("the workspace root's {MANIFEST}"),
        false => "this one".to_string(),
    };
    term::note(format!(
        "{what} no {MANIFEST}, so astli uses {sections} from {from} instead"
    ))
    .help(format!(
        "the same keys under {} in a dependency's own {MANIFEST} make its description unnecessary",
        mark::code("[package]")
    ))
    .report(run);
}

/// The root's `astli.toml`, its text and a document of it to edit, for a
/// command that changes what the root requires. A root without one is
/// refused.
pub fn manifest(root: &Package) -> Result<(PathBuf, String, DocumentMut)> {
    let Some(path) = root.manifest.clone() else {
        return Err(term::error(format!("the package has no {MANIFEST}"))
            .help(format!("{} writes one", mark::code("astli init")))
            .into());
    };
    let text = std::fs::read_to_string(&path).map_err(|err| Error::io(&path, err))?;
    let document =
        (text.parse()).map_err(|err| Error::failed(format!("{}: {err}", path.display())))?;
    Ok((path, text, document))
}

/// Writes `document` over the manifest at `path` of the package `target`
/// chose and resolves again, writing the lock. If either fails, the
/// manifest is put back as `text` was, so that the design still builds.
pub fn rewrite(
    run: &Run,
    target: &Target,
    path: &Path,
    text: &str,
    document: &DocumentMut,
    locked: bool,
) -> Result {
    std::fs::write(path, document.to_string()).map_err(|err| Error::io(path, err))?;
    // Found again, workspace and all, as the manifest now reads.
    let settled = (Target::new(target.dir.clone()).here(run)).and_then(|here| {
        settle(
            run,
            &here,
            Lock::clone,
            Write::Manifest,
            Tell::Design,
            locked,
        )
    });
    if let Err(err) = settled {
        std::fs::write(path, text).map_err(|err| Error::io(path, err))?;
        return Err(err);
    }
    Ok(())
}
