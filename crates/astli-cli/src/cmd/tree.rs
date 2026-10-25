//! `astli tree` subcommand.
//!
//! Prints the dependencies of the package the current directory is in as a
//! tree, each package under those that need it, resolved as any command
//! reading them does. A package needed in more than one place lists its own
//! dependencies once, where it first appears, and is marked `(*)` wherever
//! else, so that a design whose dependencies share their own prints one line
//! per need rather than one per path to it.

use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;

use astli_resolve::{Lock, Origin, Resolved};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::Result;
use crate::lockfile::{self, Write};
use crate::render::Out;

/// Print the dependency tree of the package, with the version each resolved to
#[derive(Args)]
pub struct Tree {}

impl RunWith<Ctx<'_>> for Tree {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let root = lockfile::here()?;
        let run = ctx.run;
        let resolution =
            lockfile::settle(&root, Lock::clone, Write::Manifest, run.locked, run.quiet)?;
        let packages: BTreeMap<&str, &Resolved> = (resolution.packages.iter())
            .map(|it| (it.name.as_str(), it))
            .collect();
        // A dependency no enabled feature brings in is not resolved, and so
        // not needed.
        let mut needs: Vec<String> = (root.dependencies.keys())
            .chain(root.dev_dependencies.keys())
            .filter(|it| packages.contains_key(it.as_str()))
            .cloned()
            .collect();
        needs.sort();

        let name = match &root.name {
            Some(name) => name.clone(),
            None => (root.root.file_name())
                .map(|it| it.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.root.display().to_string()),
        };
        writeln!(ctx.out, "{name}")?;
        branches(ctx.out, &packages, &needs, "", &mut HashSet::new())
    }
}

/// Prints `needs` below a package, each line led by `indent`, and below
/// each what it needs in turn unless `shown` already holds it.
fn branches<'a>(
    out: &mut Out,
    packages: &BTreeMap<&str, &'a Resolved>,
    needs: &'a [String],
    indent: &str,
    shown: &mut HashSet<&'a str>,
) -> Result {
    for (at, name) in needs.iter().enumerate() {
        let package = packages[name.as_str()];
        let (branch, under) = match at + 1 == needs.len() {
            true => ("└── ", "    "),
            false => ("├── ", "│   "),
        };
        let first = shown.insert(name.as_str());
        let elided = match !first && !package.needs.is_empty() {
            true => " (*)",
            false => "",
        };
        writeln!(out, "{indent}{branch}{}{elided}", label(package))?;
        if first {
            let indent = format!("{indent}{under}");
            branches(out, packages, &package.needs, &indent, shown)?;
        }
    }
    Ok(())
}

/// A package said for a person: its name and version, or its commit when no
/// tag gives it one, and where it is unless it is its repository's own.
fn label(package: &Resolved) -> String {
    let name = &package.name;
    let mut label = match (&package.version, package.commit) {
        (Some(version), _) => format!("{name} {version}"),
        (None, Some(commit)) => format!("{name} {}", commit.to_hex_with_len(10)),
        (None, None) => name.clone(),
    };
    if !matches!(package.origin, Origin::Git { .. }) {
        label += &format!(" ({})", package.origin);
    }
    label
}
