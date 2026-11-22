//! When a run resolves the dependencies of the package it acts on over
//! `astli.lock`, and when it writes the lock: in a workspace the lock is its
//! root's, and resolves every member. And the dependencies a design reads,
//! loaded from what was resolved, with what loading found told; or, for a
//! run that loads what it can, the package without them.

use std::collections::BTreeSet;

use astli_package::{Dependencies, MANIFEST, mark};
use astli_resolve::{Loaded, Lock, Note, Overrides, Resolution, resolve};

use crate::error::{Error, Result};
use crate::run::{Resolving, Run};
use crate::target::Here;
use crate::term::{self, status};

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
/// workspace as `overrides` says, preferring what `kept` holds of the lock,
/// and writes the lock if it changed and `write` allows, telling the
/// warnings `tell` asks for. `locked` refuses a lock that is missing or
/// would change instead. Each change is a status line.
///
/// While a dependency is patched, the lock is read and never written, so
/// that it holds what the design builds on once the patch is gone; and
/// `locked`, as CI asks, and [`Write::Always`] fail.
pub fn settle(
    run: &Run,
    here: &Here,
    overrides: &Overrides,
    kept: impl FnOnce(&Lock) -> Lock,
    write: Write,
    tell: Tell,
    locked: bool,
) -> Result<Resolution> {
    let root = &here.workspace.root;
    let path = here.lockfile();
    let patched = overrides.patches.0.keys().map(mark::name);
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
    let (old, resolution) = resolved(run, here, overrides, kept)?;
    let held = resolution.reached(&here.package.package);
    for warning in &resolution.warnings {
        let told = match (&warning.of, tell) {
            (Some(of), Tell::Design) => held.contains(of.as_str()),
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
    if write == Write::Manifest && root.package.manifest_path.is_none() {
        return Ok(resolution);
    }
    for (verb, what) in old.unwrap_or_default().changes(&new) {
        status!(verb, "{what}");
    }
    std::fs::write(&path, new.to_string()).map_err(|err| Error::io(&path, err))?;
    // Every commit locked is one the requirements admit, so a resolution
    // preferring the lock written chooses them all again: the next
    // package's run, reading it, needs no resolution of its own.
    run.resolved(resolving(here, overrides, new), &resolution);
    Ok(resolution)
}

/// What [`settle`] resolves, and the lock as it read it, without telling
/// a warning or writing the lock.
pub fn resolved(
    run: &Run,
    here: &Here,
    overrides: &Overrides,
    kept: impl FnOnce(&Lock) -> Lock,
) -> Result<(Option<Lock>, Resolution)> {
    let old = Lock::read(&here.lockfile())?;
    let preferred = old.as_ref().map(kept).unwrap_or_default();
    let resolution = run.resolution(resolving(here, overrides, preferred), |it| {
        Ok(resolve(
            &run.store()?,
            &it.roots,
            &it.overrides,
            &it.preferred,
        )?)
    })?;
    Ok((old, resolution))
}

/// What resolving `here`'s workspace as `overrides` says, preferring
/// `preferred`, reads.
fn resolving(here: &Here, overrides: &Overrides, preferred: Lock) -> Resolving {
    Resolving {
        roots: (here.workspace.packages())
            .map(|it| it.package.clone())
            .collect(),
        overrides: overrides.clone(),
        preferred,
    }
}

/// The dependencies of `here`'s package, resolved over the lock, which is
/// written as [`Write::Manifest`] says. In a workspace the whole workspace
/// is resolved, so that the lock is the same wherever it is written, and
/// the package's design is the part of it the package reaches.
pub fn dependencies(run: &Run, here: &Here, locked: bool) -> Result<Loaded> {
    let resolution = settle(
        run,
        here,
        &here.overrides(),
        Lock::clone,
        Write::Manifest,
        Tell::Design,
        locked,
    )?;
    let store = run.store()?;
    let package = &here.package.package;
    let told = |note| warning(note).report(run);
    Ok(resolution.load(&store, &here.workspace.root, package, told)?)
}

/// The dependencies of `here`'s package, as [`dependencies`] loads them,
/// with the package they are loaded for: `here`'s own.
///
/// A run that [loads what it can](Run::loads_what_it_can) is not failed by
/// a dependency it cannot settle or read: why is a problem it keeps, and
/// the package is read without any dependency, as if it required none. An
/// error that names one dependency still loses the rest, since a
/// resolution that fails holds none of them.
pub fn loaded(run: &Run, here: Here) -> Result<(Here, Loaded)> {
    let err = match dependencies(run, &here, run.args.locked) {
        Ok(loaded) => return Ok((here, loaded)),
        Err(err) if run.loads_what_it_can() => err,
        Err(err) => return Err(err),
    };
    (err.problem())
        .note("the package is read without its dependencies")
        .report(run);
    run.read_without_dependencies();
    let mut here = here;
    let manifest = &mut here.package.package.manifest;
    manifest.dependencies.clear();
    manifest.dev_dependencies.clear();
    manifest.dev_features.clear();
    for feature in manifest.features.values_mut() {
        feature.dependencies.clear();
    }
    let loaded = Loaded {
        dependencies: Dependencies::default(),
        described: Vec::new(),
        on_disk: BTreeSet::new(),
    };
    Ok((here, loaded))
}

/// What a run says of `note`.
fn warning(note: Note) -> term::Problem {
    match note {
        Note::OwnManifest(name) => term::warning(format!(
            "{} has its own {MANIFEST}, which is read instead of {}",
            mark::name(&name),
            mark::code(format!("[described.{name}]"))
        ))
        .help("remove the description"),
        Note::Unmatched { name, pattern } => term::warning(format!(
            "{}: {} matches none of its files",
            mark::code(format!("described.{name}.exclude")),
            mark::code(pattern)
        )),
    }
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
