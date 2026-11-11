//! Resolving the root's dependencies over `astli.lock`, and writing it, and
//! changing what the root's `astli.toml` requires. In a workspace the lock
//! is its root's, and resolves every member.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once, OnceLock};

use astli_package::{Dependencies, MANIFEST, Package, Patches, Workspace, mark};
use astli_resolve::{LOCKFILE, Lock, Origin, Resolution, Store, resolve};
use toml_edit::DocumentMut;

use crate::error::{Error, Result};
use crate::term::{self, status};

/// The store the environment names, showing each clone, fetch and
/// checkout it makes. One for the run, so that a mirror a command fetched
/// is not fetched again by the resolution after it.
pub fn store() -> Result<Store> {
    static STORE: OnceLock<Store> = OnceLock::new();
    if let Some(store) = STORE.get() {
        return Ok(store.clone());
    }
    let store = Store::from_env()?.reporting(Arc::new(term::Steps));
    Ok(STORE.get_or_init(|| store).clone())
}

/// The package the current directory is in, or the one `-p` chose, whose
/// dependencies a command that names no files works on, and its workspace.
pub fn here() -> Result<Here> {
    let package = Package::find(&dir()?)?.ok_or_else(|| {
        Error::failed("no package here: run inside a git repository or beside an astli.toml")
    })?;
    Here::of(package)
}

/// The root of the package `-p` or `--workspace` chose, which a command
/// acts on as if run there.
static CHOSEN: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The directory a command acts on the package of: the root of the one
/// [`choose`] chose, or else the current directory.
pub fn dir() -> Result<PathBuf> {
    if let Some(chosen) = CHOSEN.lock().unwrap_or_else(|it| it.into_inner()).clone() {
        return Ok(chosen);
    }
    std::env::current_dir().map_err(|err| Error::io(".", err))
}

/// Makes the commands of the run act on the package at `root`, as if run
/// there.
pub fn choose(root: &Path) {
    *CHOSEN.lock().unwrap_or_else(|it| it.into_inner()) = Some(root.to_path_buf());
}

/// The workspace the current directory is in, for `-p` or `--workspace`,
/// which `flag` names. Outside one it is an error.
pub fn workspace(flag: &str) -> Result<Workspace> {
    let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
    let workspace = match Package::find(&here)? {
        Some(package) => Workspace::find(&package)?,
        None => None,
    };
    workspace.ok_or_else(|| {
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
pub fn named<'a>(workspace: &'a Workspace, name: &str) -> Result<&'a Package> {
    if let Some(package) = (workspace.packages()).find(|it| it.name.as_deref() == Some(name)) {
        return Ok(package);
    }
    let names: Vec<String> = (workspace.packages())
        .filter_map(|it| it.name.as_ref().map(mark::name))
        .collect();
    Err(term::error(format!(
        "no package of the workspace at {} is called {}",
        mark::path(workspace.root.root.display()),
        mark::name(name)
    ))
    .help(format!("its packages are {}", names.join(", ")))
    .into())
}

/// The package a command acts on, and the workspace it is in, whose root
/// holds what its packages share: the lock, the pins, the patches, the
/// descriptions and the build directory.
pub struct Here {
    /// The package itself, with the patches that count for it.
    pub package: Package,
    /// The workspace it is in, if any.
    pub workspace: Option<Workspace>,
}

impl Here {
    /// `package` and the workspace it is in. A member's manifest holding
    /// what only the workspace root's may is warned about, once a run.
    pub fn of(mut package: Package) -> Result<Here> {
        let workspace = Workspace::find(&package)?;
        if let Some(workspace) = &workspace {
            static WARNED: Once = Once::new();
            let mut unread = Ok(());
            WARNED.call_once(|| unread = warn_unread(workspace));
            unread?;
            // Only the workspace root's count, so the walk stops at them.
            package.patches = workspace.root.patches.clone();
        }
        Ok(Here { package, workspace })
    }

    /// What holds the lock and the rest a workspace shares: its root, or
    /// without one the package itself.
    pub fn root(&self) -> &Package {
        match &self.workspace {
            Some(workspace) => &workspace.root,
            None => &self.package,
        }
    }

    /// [`Here::root`], to change before resolving, as if a patch were not
    /// there.
    pub fn root_mut(&mut self) -> &mut Package {
        match &mut self.workspace {
            Some(workspace) => &mut workspace.root,
            None => &mut self.package,
        }
    }

    /// The packages resolved beside [`Here::root`]: the workspace's
    /// members, or none without one.
    pub fn members(&self) -> &[Package] {
        match &self.workspace {
            Some(workspace) => &workspace.members,
            None => &[],
        }
    }
}

/// Warns of each table a member's manifest holds that only the workspace
/// root's is read for, and of a member's own patches.
fn warn_unread(workspace: &Workspace) -> Result {
    for member in &workspace.members {
        let Some(manifest) = &member.manifest else {
            continue;
        };
        let text = std::fs::read_to_string(manifest).map_err(|err| Error::io(manifest, err))?;
        let table: toml::Table = (text.parse())
            .map_err(|err| Error::failed(format!("{}: {err}", manifest.display())))?;
        let unread: Vec<String> = ["resolve", "described", "build", "profile"]
            .into_iter()
            .filter(|key| table.contains_key(*key))
            .map(|key| mark::code(format!("[{key}]")))
            .collect();
        if !unread.is_empty() {
            term::warning(format!(
                "{}: astli reads {} only from the workspace root's {MANIFEST}",
                mark::path(manifest.display()),
                unread.join(", "),
            ))
            .help(format!(
                "move what is to count to {}",
                mark::path(workspace.root.root.join(MANIFEST).display())
            ))
            .report();
        }
        let patches = Patches::path(&member.root);
        if patches.is_file() {
            term::warning(format!(
                "{} is not read, since only the workspace root's patches count",
                mark::path(patches.display())
            ))
            .help(format!(
                "{} in the workspace patches its root",
                mark::code("astli patch")
            ))
            .report();
        }
    }
    Ok(())
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
/// if it changed and `write` allows. `locked` refuses a lock that is
/// missing or would change instead. Each change is a status line.
///
/// While the root patches a dependency, the lock is read and never written,
/// so that it holds what the design builds on once the patch is gone; and
/// `locked`, as CI asks, and [`Write::Always`] fail.
pub fn settle(
    here: &Here,
    kept: impl FnOnce(&Lock) -> Lock,
    write: Write,
    locked: bool,
) -> Result<Resolution> {
    let root = here.root();
    let path = root.root.join(LOCKFILE);
    let patched = root.patches.0.keys().map(mark::name);
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
    let pinned: BTreeSet<String> = root.pinned.iter().cloned().collect();
    let resolution = resolve(&store()?, root, here.members(), &pinned, &preferred)?;
    for warning in &resolution.warnings {
        term::warning(warning).report();
    }
    for name in root.described.keys() {
        if !resolution.packages.iter().any(|it| it.name == *name) {
            term::warning(format!(
                "{} describes a package the design does not hold",
                mark::code(format!("[described.{name}]"))
            ))
            .report();
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
        return Err(term::error(format!(
            "{} {why}, and {} keeps it as it is",
            mark::path(path.display()),
            mark::code("--locked")
        ))
        .help(format!("{} writes it", mark::code("astli update")))
        .into());
    }
    if write == Write::Manifest && root.manifest.is_none() {
        return Ok(resolution);
    }
    for (verb, what) in old.unwrap_or_default().changes(&new) {
        status!(verb, "{what}");
    }
    std::fs::write(&path, new.to_string()).map_err(|err| Error::io(&path, err))?;
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
/// fetched repository.
pub fn dependencies(here: &Here, locked: bool) -> Result<(Dependencies, Vec<String>, Vec<String>)> {
    let resolution = settle(here, Lock::clone, Write::Manifest, locked)?;
    let package = &here.package;
    let root = here.root();
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
    let store = store()?;
    let mut found = Vec::new();
    let mut described = Vec::new();
    let mut on_disk = Vec::new();
    for it in resolution.packages {
        if !reached.contains(&it.name) {
            continue;
        }
        if matches!(it.origin, Origin::Dir(_)) {
            on_disk.push(it.name.clone());
        }
        let named = |err: String| Error::failed(format!("{}: {err}", mark::name(&it.name)));
        let located = it.checkout(&store).map_err(|err| named(err.message()))?;
        let mut package = Package::at(&located.dir).map_err(|err| named(err.message()))?;
        if let (Some(description), Some(manifest)) = (root.described.get(&it.name), &root.manifest)
        {
            match package.manifest {
                Some(_) => term::warning(format!(
                    "{} has its own {MANIFEST}, which is read instead of {}",
                    mark::name(&it.name),
                    mark::code(format!("[described.{}]", it.name))
                ))
                .help("remove the description")
                .report(),
                None => {
                    if let Some(held) = &located.held {
                        for pattern in description.unmatched(&package.root, held) {
                            term::warning(format!(
                                "{}: {} matches none of its files",
                                mark::code(format!("described.{}.exclude", it.name)),
                                mark::code(pattern)
                            ))
                            .report();
                        }
                    }
                    package = package.described(&it.name, description, manifest)?;
                    described.push(it.name.clone());
                }
            }
        }
        package.held = located.held;
        found.push((it.name, package.as_dependency(), it.needs));
    }
    Ok((Dependencies::ordered(package, found)?, described, on_disk))
}

/// Notes the dependencies read as the root describes them, if any. Only the
/// commands about dependencies say so: elsewhere it would read as a finding
/// about the design.
pub fn described(names: &[String]) {
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
    term::note(format!(
        "{what} no {MANIFEST}, so astli uses {sections} from this one instead"
    ))
    .help(format!(
        "the same keys under {} in a dependency's own {MANIFEST} make its description unnecessary",
        mark::code("[package]")
    ))
    .report();
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

/// Writes `document` over the root's manifest at `path` and resolves again,
/// writing the lock. If either fails, the manifest is put back as `text`
/// was, so that the design still builds.
pub fn rewrite(path: &Path, text: &str, document: &DocumentMut, locked: bool) -> Result {
    std::fs::write(path, document.to_string()).map_err(|err| Error::io(path, err))?;
    let settled = here().and_then(|here| settle(&here, Lock::clone, Write::Manifest, locked));
    if let Err(err) = settled {
        std::fs::write(path, text).map_err(|err| Error::io(path, err))?;
        return Err(err);
    }
    Ok(())
}
