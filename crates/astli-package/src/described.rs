//! What the root says of a dependency that has no manifest: the files its
//! manifest would select, until it has one of its own.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::package::inside;
use crate::walk::patterns;
use crate::{Error, Package, mark};

/// A `[described.NAME]` table: what the dependency's own `[package]` would
/// say of its files, relative to the dependency's root, so that it moves
/// there unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Description {
    /// `None` when not written, so that the default applies.
    pub sources: Option<Vec<PathBuf>>,
    pub include_dirs: Option<Vec<PathBuf>>,
    pub exclude: Vec<String>,
}

impl Package {
    /// The package as `description` describes it, which `manifest`, the
    /// root's, writes as `[described.<name>]`; or why a path in it is not
    /// in the package. Only a package without a manifest of its own is
    /// described, so what the description says replaces the defaults.
    pub fn described(
        mut self,
        name: &str,
        description: &Description,
        manifest: &Path,
    ) -> Result<Package, Error> {
        let checked = |key: &str, written: &Path| -> Result<PathBuf, Error> {
            match inside(&self.root, written) {
                Some(full) if full.exists() => Ok(full),
                _ => Err(Error::Manifest(
                    manifest.to_path_buf(),
                    format!(
                        "{}: {} is not in {}",
                        mark::code(format!("described.{name}.{key}")),
                        mark::path(written.display()),
                        mark::name(name)
                    ),
                )),
            }
        };
        if let Some(sources) = &description.sources {
            self.sources = (sources.iter())
                .map(|path| checked("sources", path))
                .collect::<Result<_, _>>()?;
        }
        if let Some(dirs) = &description.include_dirs {
            self.include_dirs = (dirs.iter())
                .map(|path| checked("include-dirs", path))
                .collect::<Result<_, _>>()?;
        }
        self.exclude.extend(description.exclude.iter().cloned());
        Ok(self)
    }
}

impl Description {
    /// Each of its `exclude` patterns that matches none of `held`, the
    /// absolute paths of every file of the package at `root`, nor any
    /// directory holding one: written for a file the package no longer has.
    pub fn unmatched(&self, root: &Path, held: &BTreeSet<PathBuf>) -> Vec<&str> {
        // A walk excludes a directory by its own path, so a pattern naming
        // one matches every file under it.
        let relative: BTreeSet<&Path> = (held.iter())
            .filter_map(|path| path.strip_prefix(root).ok())
            .flat_map(Path::ancestors)
            .filter(|path| !path.as_os_str().is_empty())
            .collect();
        (self.exclude.iter())
            .filter(|pattern| {
                // Checked when the root's manifest was read.
                let Ok(set) = patterns(&None, "", std::slice::from_ref(pattern)) else {
                    return false;
                };
                !relative.iter().any(|path| set.is_match(path))
            })
            .map(String::as_str)
            .collect()
    }
}
