//! Resolving the root's dependencies over `astli.lock`, and writing it.

use std::collections::BTreeSet;

use astli_package::{Dependencies, MANIFEST, Package};
use astli_resolve::{LOCKFILE, Lock, Resolution, Store, resolve};

use crate::error::{Error, Result};
use crate::term::say;

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
/// refuses a lock that is missing or would change instead. Changes are
/// said unless `quiet`.
///
/// While the root patches a dependency, the lock is read and never written,
/// so that it holds what the design builds on once the patch is gone; and
/// `locked`, as CI asks, and [`Write::Always`] fail.
pub fn settle(
    root: &Package,
    kept: impl FnOnce(&Lock) -> Lock,
    write: Write,
    locked: bool,
    quiet: bool,
) -> Result<Resolution> {
    let path = root.root.join(LOCKFILE);
    let patched = root.patches.0.keys().map(|it| format!("`{it}`"));
    let patched = patched.collect::<Vec<_>>().join(", ");
    if !patched.is_empty() && (locked || write == Write::Always) {
        let asked = match locked {
            true => "`--locked`",
            false => "`astli update`",
        };
        return Err(Error::failed(format!(
            "{asked} needs a design without patches, and the root patches {patched}; \
             `astli patch --remove` drops a patch"
        )));
    }
    let old = Lock::read(&path)?;
    let preferred = old.as_ref().map(kept).unwrap_or_default();
    let pinned: BTreeSet<String> = root.pinned.iter().cloned().collect();
    let resolution = resolve(&Store::from_env()?, root, &pinned, &preferred)?;
    for warning in &resolution.warnings {
        say!("astli: {warning}");
    }
    for name in root.described.keys() {
        if !resolution.packages.iter().any(|it| it.name == *name) {
            say!("astli: `[described.{name}]` describes a package the design does not hold");
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
            say!("{change}");
        }
    }
    std::fs::write(&path, new.to_string()).map_err(|err| Error::io(&path, err))?;
    Ok(resolution)
}

/// The dependencies of `root`, each checked out and read as a dependent
/// sees it, resolved over the lock, which is written as [`Write::Manifest`]
/// says. A dependency without a manifest is read as the root describes it,
/// and the described ones are named unless `quiet`.
pub fn dependencies(root: &Package, locked: bool, quiet: bool) -> Result<Dependencies> {
    let resolution = settle(root, Lock::clone, Write::Manifest, locked, quiet)?;
    let store = Store::from_env()?;
    let mut found = Vec::new();
    let mut described = Vec::new();
    for it in resolution.packages {
        let named = |err: &dyn std::fmt::Display| Error::failed(format!("`{}`: {err}", it.name));
        let located = it.checkout(&store).map_err(|err| named(&err))?;
        let mut package = Package::at(&located.dir).map_err(|err| named(&err))?;
        if let (Some(description), Some(manifest)) = (root.described.get(&it.name), &root.manifest)
        {
            match package.manifest {
                Some(_) => say!(
                    "astli: `{}` has its own {MANIFEST}, which is read instead of \
                     `[described.{}]`: remove the description",
                    it.name,
                    it.name
                ),
                None => {
                    if let Some(held) = &located.held {
                        for pattern in description.unmatched(&package.root, held) {
                            say!(
                                "astli: `described.{}.exclude`: `{pattern}` matches none of its files",
                                it.name
                            );
                        }
                    }
                    package = package.described(&it.name, description, manifest)?;
                    described.push(format!("`{}`", it.name));
                }
            }
        }
        package.held = located.held;
        found.push((it.name, package.as_dependency(), it.needs));
    }
    if !quiet && !described.is_empty() {
        say!(
            "astli: described by the root, for want of their own {MANIFEST}: {}; each \
             description belongs under `[package]` in that package's {MANIFEST}",
            described.join(", ")
        );
    }
    Ok(Dependencies::ordered(root, found)?)
}
