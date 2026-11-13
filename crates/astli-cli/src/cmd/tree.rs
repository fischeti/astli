//! `astli tree` subcommand.
//!
//! Displays the resolved dependency tree of the active package as an ASCII tree.
//!
//! Each dependency is listed under the packages that require it, alongside its resolved
//! version (or git commit / local checkout path). If a package appears multiple times in
//! the tree, its own dependencies are printed only on first occurrence and marked with
//! `(*)` thereafter. Dependencies configured via `[described.NAME]` are annotated with
//! `(described)`. With `--workspace`, each package of the workspace has a
//! tree, a blank line apart.

use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;
use std::path::Path;

use astli_package::MANIFEST;
use astli_resolve::{Lock, Origin, Resolved};
use usage_rs::{Args, RunWith};

use crate::cli::{Members, RunArgs};
use crate::cmd::{self, Ctx};
use crate::error::Result;
use crate::lockfile::{self, Write};
use crate::render::Out;

/// Display the resolved dependency tree and package hierarchy
#[derive(Args)]
pub struct Tree {
    #[usage(flatten)]
    pub members: Members,
}

impl RunWith<Ctx<'_>> for Tree {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let mut first = true;
        cmd::members(&self.members, false, out, |out| {
            // A tree for each package, a line apart.
            if !std::mem::take(&mut first) {
                writeln!(out)?;
            }
            tree(out, run)
        })
    }
}

/// Prints the tree of the package the run acts on.
fn tree(out: &mut Out, run: &RunArgs) -> Result {
    let here = lockfile::here()?;
    let resolution = lockfile::settle(
        &here,
        Lock::clone,
        Write::Manifest,
        lockfile::Tell::Design,
        run.locked,
    )?;
    let root = &here.package;
    let store = lockfile::store()?;
    let mut nodes = BTreeMap::new();
    let mut described = Vec::new();
    let held = lockfile::held(&here.package, &resolution);
    for package in &resolution.packages {
        let mut label = label(package, &here.root().root);
        // A description is read only while the package has no manifest.
        if here.root().described.contains_key(&package.name)
            && !package.checkout(&store)?.dir.join(MANIFEST).is_file()
        {
            label += " (described)";
            // Only what the tree shows, a member's run resolving the
            // whole workspace; but run for each package, the workspace's
            // whole, so that the note is the same and told once.
            if held.contains(&package.name) || cmd::several() {
                described.push(package.name.clone());
            }
        }
        let needs = &package.needs[..];
        nodes.insert(package.name.as_str(), Node { label, needs });
    }
    // A dependency no enabled feature brings in is not resolved, and so
    // not needed.
    let mut needs: Vec<String> = (root.dependencies.keys())
        .chain(root.dev_dependencies.keys())
        .filter(|it| nodes.contains_key(it.as_str()))
        .cloned()
        .collect();
    needs.sort();
    lockfile::described(&described, here.workspace.is_some());

    let name = match &root.name {
        Some(name) => name.clone(),
        None => (root.root.file_name())
            .map(|it| it.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.root.display().to_string()),
    };
    writeln!(out, "{name}")?;
    branches(out, &nodes, &needs, "", &mut HashSet::new())
}

/// A package as the tree prints it.
struct Node<'a> {
    label: String,
    /// The packages it depends on directly, by name.
    needs: &'a [String],
}

/// Prints `needs` below a package, each line led by `indent`, and below
/// each what it needs in turn unless `shown` already holds it.
fn branches<'a>(
    out: &mut Out,
    nodes: &BTreeMap<&str, Node<'a>>,
    needs: &'a [String],
    indent: &str,
    shown: &mut HashSet<&'a str>,
) -> Result {
    for (at, name) in needs.iter().enumerate() {
        let node = &nodes[name.as_str()];
        let (branch, under) = match at + 1 == needs.len() {
            true => ("└── ", "    "),
            false => ("├── ", "│   "),
        };
        let first = shown.insert(name.as_str());
        let elided = match !first && !node.needs.is_empty() {
            true => " (*)",
            false => "",
        };
        writeln!(out, "{indent}{branch}{}{elided}", node.label)?;
        if first {
            let indent = format!("{indent}{under}");
            branches(out, nodes, node.needs, &indent, shown)?;
        }
    }
    Ok(())
}

/// A package said for a person: its name and version, or its commit when no
/// tag gives it one, and where it is unless it is its repository's own. A
/// directory under `base`, the workspace root or else the package's, is
/// said from there, the same on every machine.
fn label(package: &Resolved, base: &Path) -> String {
    let name = &package.name;
    let label = match (&package.version, package.commit) {
        (Some(version), _) => format!("{name} {version}"),
        (None, Some(commit)) => format!("{name} {}", commit.to_hex_with_len(10)),
        (None, None) => name.clone(),
    };
    let from = |dir: &Path| match dir.strip_prefix(base) {
        Ok(under) => under.display().to_string(),
        Err(_) => dir.display().to_string(),
    };
    let place = match &package.origin {
        Origin::Git { .. } => return label,
        Origin::Dir(dir) => from(dir),
        Origin::Patched(dir) => format!("{}, patched", from(dir)),
        origin @ Origin::Within { .. } => origin.to_string(),
    };
    label + &format!(" ({place})")
}
