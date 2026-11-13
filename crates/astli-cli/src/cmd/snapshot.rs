//! `astli snapshot` subcommand.
//!
//! Records local changes from a patched dependency into the manifests.
//!
//! Replaces the dependency's requirement in each `astli.toml` of the
//! workspace that names it with an exact Git revision (`rev = "<commit>"`)
//! pointing to the checkout's current HEAD. If a package outside the
//! workspace also requires the dependency, `astli snapshot` adds it to the
//! workspace root's `[resolve] pinned` so the exact commit takes precedence.
//! Once the manifests and lockfile are updated, the local patch in
//! `.astli/patch.toml` is removed while keeping the checkout on disk.

use std::collections::BTreeSet;
use std::path::PathBuf;

use astli_package::{MANIFEST, Package, Source, mark};
use astli_resolve::{LOCKFILE, Lock, Resolution, resolve};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::git::git;
use crate::lockfile::{self, Here, Write};
use crate::term::{self, status};

/// Pin a patched dependency to its current commit in astli.toml and remove the patch
#[derive(Args)]
pub struct Snapshot {
    /// Name of the patched dependency to snapshot
    #[usage(arg, value_name = "NAME")]
    pub name: String,
}

impl RunWith<Ctx<'_>> for Snapshot {
    type Output = Result;

    fn run_with(self, _: Ctx<'_>) -> Result {
        let here = lockfile::here()?;
        let shared = here.root();
        let name = &self.name;
        let Some(dir) = shared.patches.0.get(name) else {
            return Err(term::error(format!("{} is not patched", mark::name(name)))
                .help(format!(
                    "{} patches it",
                    mark::code(format!("astli patch {name}"))
                ))
                .into());
        };
        let changed = git(dir, ["status", "--porcelain"])?;
        if !changed.is_empty() {
            return Err(Error::failed(format!(
                "{} has changes not committed, which a snapshot would leave out:\n{changed}",
                mark::path(dir.display())
            )));
        }
        let commit = git(dir, ["rev-parse", "HEAD"])?;

        // Each package of the workspace that requires it, or else the one
        // the command runs in, which then starts to.
        let ours: Vec<&Package> = std::iter::once(shared).chain(here.members()).collect();
        let requiring: Vec<&Package> = (ours.iter().copied())
            .filter(|it| {
                it.dependencies.contains_key(name) || it.dev_dependencies.contains_key(name)
            })
            .collect();
        let mut manifests = Vec::new();
        for package in &requiring {
            manifests.push(editable(&here, package, name, &commit)?);
        }

        let url = repository(&here, &ours, name)?;
        let mut mirror = lockfile::store()?.mirror(name, &url)?;
        if !mirror.fresh {
            mirror.fetch()?;
        }
        if mirror.rev(&commit).is_none() {
            return Err(Error::failed(format!(
                "{} is on no branch or tag of {}, so nobody else can fetch it: push it first",
                mark::version(&commit),
                mark::path(&url)
            )));
        }
        drop(mirror);

        // Who else requires it, read while it is still patched, so that the
        // requirements of what the clone added are among them.
        let lock = Lock::read(&shared.root.join(LOCKFILE))?.unwrap_or_default();
        let pins = shared.pinned.iter().cloned().collect();
        let resolution = resolve(&lockfile::store()?, shared, here.members(), &pins, &lock)?;
        let targets = match requiring.is_empty() {
            false => requiring,
            true if here.workspace.is_some() && !holds(&resolution, &here.package, name) => {
                return Err(Error::failed(format!(
                    "no package of the workspace requires {}, and the design of {} does not \
                     hold it: run {} in a member whose design does",
                    mark::name(name),
                    called(&here, &here.package),
                    mark::code(format!("astli snapshot {name}"))
                )));
            }
            true => {
                manifests.push(editable(&here, &here.package, name, &commit)?);
                vec![&here.package]
            }
        };
        let names: BTreeSet<&str> = ours.iter().filter_map(|it| it.name.as_deref()).collect();
        let required_elsewhere = (resolution.packages.iter()).any(|it| {
            it.name != *name && !names.contains(it.name.as_str()) && it.needs.contains(name)
        });
        for (_, _, document) in &mut manifests {
            required(document, name, &url, &commit);
        }
        if required_elsewhere && !shared.pinned.contains(name) {
            // Outside a workspace the root is written already, and a
            // workspace root holds `[workspace]`: either way it has a manifest.
            let path = shared
                .manifest
                .clone()
                .expect("the workspace root's manifest");
            let at = match manifests.iter().position(|(it, _, _)| *it == path) {
                Some(at) => at,
                None => {
                    manifests.push(read(path)?);
                    manifests.len() - 1
                }
            };
            pinned(&mut manifests[at].2, name);
        }

        let mut unpatched = shared.patches.clone();
        unpatched.0.remove(name);
        let settled = (|| {
            for (path, _, document) in &manifests {
                std::fs::write(path, document.to_string()).map_err(|err| Error::io(path, err))?;
            }
            unpatched.write(&shared.root)?;
            lockfile::here().and_then(|here| {
                lockfile::settle(
                    &here,
                    Lock::clone,
                    Write::Manifest,
                    lockfile::Tell::Workspace,
                    false,
                )
            })
        })();
        if let Err(err) = settled {
            // As it was, so that the design still builds.
            for (path, text, _) in &manifests {
                std::fs::write(path, text).map_err(|err| Error::io(path, err))?;
            }
            shared.patches.write(&shared.root)?;
            return Err(err);
        }
        for package in &targets {
            let manifest = package.manifest.as_deref().expect("written");
            let shown = manifest.strip_prefix(&shared.root).unwrap_or(manifest);
            status!(
                "Required",
                "{name} at {} in {}",
                &commit[..12],
                shown.display()
            );
        }
        status!("Unpatched", "{name}, leaving {} as it is", dir.display());
        Ok(())
    }
}

/// `package`'s manifest, its text and that parsed, to require `name` in;
/// refused if its dependencies come from a `Bender.yml`.
fn editable(
    here: &Here,
    package: &Package,
    name: &str,
    commit: &str,
) -> Result<(PathBuf, String, DocumentMut)> {
    let bender = |why: &str| {
        Error::failed(format!(
            "{}'s dependencies come from its Bender.yml{why}, which is the bender build's to \
             edit: require {} at {} there, and {}",
            called(here, package),
            mark::name(name),
            mark::version(commit),
            mark::code(format!("astli patch --remove {name}"))
        ))
    };
    let Some(manifest) = package.manifest.clone() else {
        return Err(bender(""));
    };
    let read = read(manifest)?;
    if read.2.get("dependencies").is_none() {
        let why = format!(
            ", since its {MANIFEST} has no {} table",
            mark::code("[dependencies]")
        );
        return Err(bender(&why));
    }
    Ok(read)
}

/// The manifest at `path`, its text and that parsed.
fn read(path: PathBuf) -> Result<(PathBuf, String, DocumentMut)> {
    let text = std::fs::read_to_string(&path).map_err(|err| Error::io(&path, err))?;
    let document =
        (text.parse()).map_err(|err| Error::failed(format!("{}: {err}", path.display())))?;
    Ok((path, text, document))
}

/// What a message calls `package`: the root outside a workspace, and in
/// one, its name or else its directory.
fn called(here: &Here, package: &Package) -> String {
    match (&here.workspace, &package.name) {
        (None, _) => "the root".to_string(),
        (Some(_), Some(name)) => mark::name(name),
        (Some(_), None) => mark::path(package.root.display()),
    }
}

/// Whether `package`'s design holds `name`: some chain of dependencies
/// from it reaches it.
fn holds(resolution: &Resolution, package: &Package, name: &str) -> bool {
    let mut todo: Vec<&str> = (package.dependencies.keys())
        .chain(package.dev_dependencies.keys())
        .map(String::as_str)
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(next) = todo.pop() {
        if next == name {
            return true;
        }
        if !seen.insert(next) {
            continue;
        }
        if let Some(resolved) = resolution.packages.iter().find(|it| it.name == next) {
            todo.extend(resolved.needs.iter().map(String::as_str));
        }
    }
    false
}

/// The repository the package `name` comes from when it is not patched:
/// a requirement of it in the workspace says, or else the lock, which a
/// patch leaves as it was.
fn repository(here: &Here, ours: &[&Package], name: &str) -> Result<String> {
    let requirements = (ours.iter())
        .flat_map(|it| [it.dependencies.get(name), it.dev_dependencies.get(name)])
        .flatten();
    for requirement in requirements {
        if let Some(Source::Git { url, .. }) = &requirement.source {
            return Ok(url.clone());
        }
    }
    let lock = Lock::read(&here.root().root.join(LOCKFILE))?.unwrap_or_default();
    match lock.get(name) {
        Some(locked) => Ok(locked.source.clone()),
        None => Err(Error::failed(format!(
            "{} comes from no repository that the workspace requires or {LOCKFILE} names, so \
             there is no repository to require a commit of",
            mark::name(name)
        ))),
    }
}

/// Makes the manifest `document` require `name` from `url` at `commit`,
/// in the table that already names it, keeping the rest of what it asks,
/// such as features.
fn required(document: &mut DocumentMut, name: &str, url: &str, commit: &str) {
    let table = match document.get("dev-dependencies") {
        Some(dev) if dev.get(name).is_some() => "dev-dependencies",
        _ => "dependencies",
    };
    let entries = &mut document[table];
    let mut entry = match entries.get(name).and_then(Item::as_value) {
        Some(Value::InlineTable(entry)) => entry.clone(),
        _ => match entries.get(name).and_then(Item::as_table) {
            Some(entry) => entry.clone().into_inline_table(),
            None => InlineTable::new(),
        },
    };
    for key in ["path", "version", "rev"] {
        entry.remove(key);
    }
    if !entry.contains_key("git") {
        entry.insert("git", url.into());
    }
    entry.insert("rev", commit.into());
    entries[name] = Item::Value(Value::InlineTable(entry));
}

/// Adds `name` to the `[resolve] pinned` of the manifest `document`.
fn pinned(document: &mut DocumentMut, name: &str) {
    if document.get("resolve").is_none() {
        document["resolve"] = Item::Table(Table::new());
    }
    let resolve = &mut document["resolve"];
    if resolve.get("pinned").is_none() {
        resolve["pinned"] = toml_edit::value(Array::new());
    }
    if let Some(pinned) = resolve["pinned"].as_array_mut() {
        pinned.push(name);
    }
}
