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

use astli_package::{BENDER_MANIFEST, Package, Source, mark};
use astli_resolve::{LOCKFILE, Lock};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::git::git;
use crate::lockfile::{self, Here, Tell};
use crate::manifest::{self, Editable};
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

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let run = ctx.run;
        let here = lockfile::here(run)?;
        let shared = &here.workspace.root.package;
        let own = &here.package.package;
        let name = &self.name;
        let patches = &here.workspace.patches;
        let Some(dir) = patches.0.get(name) else {
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
        let members: Vec<Package> = (here.workspace.members.iter())
            .map(|it| it.package.clone())
            .collect();
        let ours: Vec<&Package> = std::iter::once(shared).chain(&members).collect();
        let requiring: Vec<&Package> = (ours.iter().copied())
            .filter(|it| {
                it.manifest.dependencies.contains_key(name)
                    || it.manifest.dev_dependencies.contains_key(name)
            })
            .collect();
        let mut manifests = Vec::new();
        for package in &requiring {
            manifests.push(editable(&here, package, name, &commit)?);
        }

        let url = repository(&here, &ours, name)?;
        let mirror = run.store()?.fetched(name, &url)?;
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
        let (_, resolution) = lockfile::resolved(run, &here, &here.overrides(), Lock::clone)?;
        let targets = match requiring.is_empty() {
            false => requiring,
            true if here.in_workspace && !resolution.reached(own).contains(name.as_str()) => {
                return Err(Error::failed(format!(
                    "no package of the workspace requires {}, and the design of {} does not \
                     hold it: run {} in a member whose design does",
                    mark::name(name),
                    here.called(own),
                    mark::code(format!("astli snapshot {name}"))
                )));
            }
            true => {
                manifests.push(editable(&here, own, name, &commit)?);
                vec![own]
            }
        };
        let names: BTreeSet<&str> = ours
            .iter()
            .filter_map(|it| it.manifest.name.as_deref())
            .collect();
        let required_elsewhere = (resolution.packages.iter()).any(|it| {
            it.name != *name && !names.contains(it.name.as_str()) && it.requires.contains(name)
        });
        for manifest in &mut manifests {
            manifest.require_at(name, &url, &commit);
        }
        if required_elsewhere && !here.workspace.root.tables.pinned.contains(name) {
            // Outside a workspace the root is written already, and a
            // workspace root holds `[workspace]`: either way it has a manifest.
            let path = shared
                .manifest_path
                .clone()
                .expect("the workspace root's manifest");
            let at = match manifests.iter().position(|it| it.path() == path) {
                Some(at) => at,
                None => {
                    manifests.push(Editable::open(shared)?);
                    manifests.len() - 1
                }
            };
            manifests[at].pin(name);
        }

        let mut unpatched = patches.clone();
        unpatched.0.remove(name);
        manifest::commit(
            run,
            &here,
            &manifests,
            Some(&unpatched),
            Tell::Workspace,
            false,
        )?;
        for package in &targets {
            let manifest = package.manifest_path.as_deref().expect("written");
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

/// `package`'s manifest, to require `name` in; refused if its dependencies
/// come from a `Bender.yml` and `name` would be written among them. One
/// without a `[dependencies]` table or a `Bender.yml` has none, and gets the
/// table.
fn editable(here: &Here, package: &Package, name: &str, commit: &str) -> Result<Editable> {
    // A dev-dependency is the astli.toml's to list, whatever lists the rest.
    if !package.manifest.dev_dependencies.contains_key(name)
        && let Some(problem) = manifest::from_bender(here, package)
    {
        return Err(problem
            .help(format!(
                "or require {} at {} in the {BENDER_MANIFEST} by hand, and drop the patch with {}",
                mark::name(name),
                mark::version(commit),
                mark::code(format!("astli patch --remove {name}"))
            ))
            .into());
    }
    Editable::open(package)
}

/// The repository the package `name` comes from when it is not patched:
/// a requirement of it in the workspace says, or else the lock, which a
/// patch leaves as it was.
fn repository(here: &Here, ours: &[&Package], name: &str) -> Result<String> {
    let requirements = (ours.iter())
        .flat_map(|it| {
            [
                it.manifest.dependencies.get(name),
                it.manifest.dev_dependencies.get(name),
            ]
        })
        .flatten();
    for requirement in requirements {
        if let Some(Source::Git { url, .. }) = &requirement.source {
            return Ok(url.clone());
        }
    }
    let lock = Lock::read(&here.lockfile())?.unwrap_or_default();
    match lock.get(name) {
        Some(locked) => Ok(locked.source.clone()),
        None => Err(Error::failed(format!(
            "{} comes from no repository that the workspace requires or {LOCKFILE} names, so \
             there is no repository to require a commit of",
            mark::name(name)
        ))),
    }
}
