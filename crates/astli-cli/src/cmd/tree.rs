//! `astli tree` subcommand.
//!
//! Displays the resolved dependency tree of the active package as an ASCII tree.
//!
//! Each dependency is listed under the packages that require it, alongside its resolved
//! version (or git commit / local checkout path). If a package appears multiple times in
//! the tree, its own dependencies are printed only on first occurrence and marked with
//! `(*)` thereafter. Dependencies configured via `[described.NAME]` are annotated with
//! `(described)`.

use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;

use astli_package::MANIFEST;
use astli_resolve::{Lock, Origin, Resolved};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::Result;
use crate::lockfile::{self, Write};
use crate::render::Out;

/// Display the resolved dependency tree and package hierarchy
#[derive(Args)]
pub struct Tree {}

impl RunWith<Ctx<'_>> for Tree {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let root = lockfile::here()?;
        let run = ctx.run;
        let resolution = lockfile::settle(&root, Lock::clone, Write::Manifest, run.locked)?;
        let store = lockfile::store()?;
        let mut nodes = BTreeMap::new();
        for package in &resolution.packages {
            let mut label = label(package);
            // A description is read only while the package has no manifest.
            if root.described.contains_key(&package.name)
                && !package.checkout(&store)?.dir.join(MANIFEST).is_file()
            {
                label += " (described)";
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

        let name = match &root.name {
            Some(name) => name.clone(),
            None => (root.root.file_name())
                .map(|it| it.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.root.display().to_string()),
        };
        writeln!(ctx.out, "{name}")?;
        branches(ctx.out, &nodes, &needs, "", &mut HashSet::new())
    }
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
