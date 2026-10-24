//! Resolving the root's dependencies over `astli.lock`, and writing it.

use std::collections::BTreeSet;

use astli_package::{Dependencies, Package};
use astli_resolve::{LOCKFILE, Lock, Resolution, Store, resolve};

use crate::error::{Error, Result};

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
/// refuses a lock that is missing or would change instead. Changes are
/// said unless `quiet`.
pub fn settle(
    root: &Package,
    kept: impl FnOnce(&Lock) -> Lock,
    write: Write,
    locked: bool,
    quiet: bool,
) -> Result<Resolution> {
    let path = root.root.join(LOCKFILE);
    let old = Lock::read(&path)?;
    let preferred = old.as_ref().map(kept).unwrap_or_default();
    let pinned: BTreeSet<String> = root.pinned.iter().cloned().collect();
    let resolution = resolve(&Store::from_env()?, root, &pinned, &preferred)?;
    for warning in &resolution.warnings {
        eprintln!("astli: {warning}");
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
        return Err(Error::failed(format!(
            "{} {why}, and `--locked` keeps it as it is; `astli update` writes it",
            path.display()
        )));
    }
    if write == Write::Manifest && root.manifest.is_none() {
        return Ok(resolution);
    }
    if !quiet {
        for change in old.unwrap_or_default().changes(&new) {
            eprintln!("{change}");
        }
    }
    std::fs::write(&path, new.to_string()).map_err(|err| Error::io(&path, err))?;
    Ok(resolution)
}

/// The dependencies of `root`, each read as a dependent sees it, resolved
/// over the lock, which is written as [`Write::Manifest`] says.
pub fn dependencies(root: &Package, locked: bool, quiet: bool) -> Result<Dependencies> {
    let resolution = settle(root, Lock::clone, Write::Manifest, locked, quiet)?;
    let found = (resolution.packages.into_iter())
        .map(|it| {
            let package = Package::at(&it.dir)
                .map_err(|err| Error::failed(format!("`{}`: {err}", it.name)))?
                .as_dependency();
            Ok((it.name, package, it.needs))
        })
        .collect::<Result<_>>()?;
    Ok(Dependencies::ordered(root, found)?)
}
