//! Resolving the root's dependencies over `astli.lock`, and writing it.

use std::collections::BTreeSet;
use std::sync::Arc;

use astli_package::{Dependencies, MANIFEST, Package, mark};
use astli_resolve::{LOCKFILE, Lock, Resolution, Store, resolve};

use crate::error::{Error, Result};
use crate::term::{self, status};

/// The store the environment names, showing each clone, fetch and
/// checkout it makes.
pub fn store() -> Result<Store> {
    Ok(Store::from_env()?.reporting(Arc::new(term::Steps)))
}

/// The package the current directory is in, whose dependencies a command
/// that names no files works on.
pub fn here() -> Result<Package> {
    let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
    Package::find(&here)?.ok_or_else(|| {
        Error::failed("no package here: run inside a git repository or beside an astli.toml")
    })
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

/// Resolves the dependencies of `root`, preferring what `kept` holds of the
/// lock, and writes the lock if it changed and `write` allows. `locked`
/// refuses a lock that is missing or would change instead. Each change is
/// a status line.
///
/// While the root patches a dependency, the lock is read and never written,
/// so that it holds what the design builds on once the patch is gone; and
/// `locked`, as CI asks, and [`Write::Always`] fail.
pub fn settle(
    root: &Package,
    kept: impl FnOnce(&Lock) -> Lock,
    write: Write,
    locked: bool,
) -> Result<Resolution> {
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
    let resolution = resolve(&store()?, root, &pinned, &preferred)?;
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

/// The dependencies of `root`, each checked out and read as a dependent
/// sees it, resolved over the lock, which is written as [`Write::Manifest`]
/// says. A dependency without a manifest is read as the root describes it,
/// and the described ones are named in a note.
pub fn dependencies(root: &Package, locked: bool) -> Result<Dependencies> {
    let resolution = settle(root, Lock::clone, Write::Manifest, locked)?;
    let store = store()?;
    let mut found = Vec::new();
    let mut described = Vec::new();
    for it in resolution.packages {
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
                    described.push(mark::name(&it.name));
                }
            }
        }
        package.held = located.held;
        found.push((it.name, package.as_dependency(), it.needs));
    }
    if !described.is_empty() {
        term::note(format!(
            "described by the root, for want of their own {MANIFEST}: {}",
            described.join(", ")
        ))
        .help(format!(
            "each description belongs under {} in that package's {MANIFEST}",
            mark::code("[package]")
        ))
        .report();
    }
    Ok(Dependencies::ordered(root, found)?)
}
