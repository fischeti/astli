//! `astli snapshot` subcommand.
//!
//! Records local changes from a patched dependency into the root manifest.
//!
//! Replaces the dependency's requirement in `astli.toml` with an exact Git
//! revision (`rev = "<commit>"`) pointing to the checkout's current HEAD.
//! If another transitive package also requires the dependency, `astli snapshot`
//! adds it to `[resolve] pinned` so the exact commit takes precedence.
//! Once the manifest and lockfile are updated, the local patch in
//! `.astli/patch.toml` is removed while keeping the checkout on disk.

use astli_package::{MANIFEST, Package, Source};
use astli_resolve::{LOCKFILE, Lock, Store, resolve};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::git::git;
use crate::lockfile::{self, Write};
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
        let root = lockfile::here()?;
        let name = &self.name;
        let Some(dir) = root.patches.0.get(name) else {
            return Err(term::error(format!("`{name}` is not patched"))
                .help(format!("`astli patch {name}` patches it"))
                .into());
        };
        let changed = git(dir, ["status", "--porcelain"])?;
        if !changed.is_empty() {
            return Err(Error::failed(format!(
                "{} has changes not committed, which a snapshot would leave out:\n{changed}",
                dir.display()
            )));
        }
        let commit = git(dir, ["rev-parse", "HEAD"])?;
        let Some(manifest) = root.manifest.clone() else {
            return Err(Error::failed(format!(
                "the root's dependencies come from its Bender.yml, which is the bender build's \
                 to edit: require `{name}` at `{commit}` there, and `astli patch --remove {name}`"
            )));
        };
        let text = std::fs::read_to_string(&manifest).map_err(|err| Error::io(&manifest, err))?;
        let mut document: DocumentMut = (text.parse())
            .map_err(|err| Error::failed(format!("{}: {err}", manifest.display())))?;
        if document.get("dependencies").is_none() {
            return Err(Error::failed(format!(
                "the root's dependencies come from its Bender.yml, since its {MANIFEST} has no \
                 `[dependencies]` table: require `{name}` at `{commit}` there, and \
                 `astli patch --remove {name}`"
            )));
        }
        let url = repository(&root, name)?;
        let mut mirror = Store::from_env()?.mirror(&url)?;
        if !mirror.fresh {
            mirror.fetch()?;
        }
        if mirror.rev(&commit).is_none() {
            return Err(Error::failed(format!(
                "{commit} is on no branch or tag of {url}, so nobody else can fetch it: push it first"
            )));
        }
        drop(mirror);

        // Who else requires it, read while it is still patched, so that the
        // requirements of what the clone added are among them.
        let lock = Lock::read(&root.root.join(LOCKFILE))?.unwrap_or_default();
        let pins = root.pinned.iter().cloned().collect();
        let resolution = resolve(&Store::from_env()?, &root, &pins, &lock)?;
        let required_elsewhere =
            (resolution.packages.iter()).any(|it| it.name != *name && it.needs.contains(name));
        required(&mut document, name, &url, &commit);
        if required_elsewhere && !root.pinned.contains(name) {
            pinned(&mut document, name);
        }

        let mut unpatched = root.patches.clone();
        unpatched.0.remove(name);
        std::fs::write(&manifest, document.to_string()).map_err(|err| Error::io(&manifest, err))?;
        unpatched.write(&root.root)?;
        let settled = lockfile::here()
            .and_then(|root| lockfile::settle(&root, Lock::clone, Write::Manifest, false));
        if let Err(err) = settled {
            // As it was, so that the design still builds.
            std::fs::write(&manifest, &text).map_err(|err| Error::io(&manifest, err))?;
            root.patches.write(&root.root)?;
            return Err(err);
        }
        status!("Required", "{name} at {} in {MANIFEST}", &commit[..12]);
        status!("Unpatched", "{name}, leaving {} as it is", dir.display());
        Ok(())
    }
}

/// The repository the package `name` comes from when it is not patched:
/// the root's own requirement says, or else the lock, which a patch leaves
/// as it was.
fn repository(root: &Package, name: &str) -> Result<String> {
    let requirement = (root.dependencies.get(name)).or_else(|| root.dev_dependencies.get(name));
    if let Some(Source::Git { url, .. }) = requirement.and_then(|it| it.source.as_ref()) {
        return Ok(url.clone());
    }
    let lock = Lock::read(&root.root.join(LOCKFILE))?.unwrap_or_default();
    match lock.get(name) {
        Some(locked) => Ok(locked.source.clone()),
        None => Err(Error::failed(format!(
            "`{name}` comes from no repository that the root requires or {LOCKFILE} names, so \
             there is no repository to require a commit of"
        ))),
    }
}

/// Makes the root require `name` from `url` at `commit`, in the table that
/// already names it, keeping the rest of what it asks, such as features.
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

/// Adds `name` to the root's `[resolve] pinned`.
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
