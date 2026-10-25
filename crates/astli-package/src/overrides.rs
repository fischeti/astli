//! Which of a dependency's files a package's own replace.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;

use crate::mark;
use crate::walk::patterns;
use crate::{Dependencies, Error, Package};

/// A unit as overrides see it.
#[derive(Debug, Clone)]
pub struct Declared<'a> {
    /// The package holding it: 0 for the root, then each dependency in
    /// order, as [`Dependencies::packages`] holds them.
    pub package: usize,
    pub path: &'a Path,
    /// The top-level names it declares.
    pub names: Vec<&'a str>,
}

impl Dependencies {
    /// Which of `units` an override replaces, by their place in it.
    ///
    /// A name of a dependency is replaced when a package's `[overrides]` lets
    /// it replace the name there and one of its units declares it. Every
    /// unit of that dependency declaring the name is then dropped. Each is
    /// dropped whole, so one that also declares a name nothing replaces is
    /// an error, as are two packages replacing one name, and an override of
    /// a package not in the design.
    pub fn replaced(&self, root: &Package, units: &[Declared]) -> Result<Vec<bool>, Error> {
        let (packages, names, at) = self.indexed(root);

        // What each package may replace, and where.
        let mut grants = Vec::new();
        for (by, package) in packages.iter().enumerate() {
            for (dependency, written) in &package.overrides {
                let of = match at.get(dependency.as_str()) {
                    Some(&of) if of != by => of,
                    _ => {
                        return Err(Error::Override(format!(
                            "{} overrides names of {}, which is not another package in the \
                             design",
                            names[by],
                            mark::name(dependency)
                        )));
                    }
                };
                let key = format!("overrides.{dependency}");
                grants.push((by, of, patterns(&package.manifest, &key, written)?));
            }
        }

        // Each name replaced, in the package it is replaced in, and the unit
        // replacing it.
        let mut replacing: HashMap<(usize, &str), usize> = HashMap::new();
        for (unit, declared) in units.iter().enumerate() {
            let granted = grants.iter().filter(|(by, ..)| *by == declared.package);
            for (_, of, set) in granted {
                for &name in declared.names.iter().filter(|name| set.is_match(name)) {
                    match replacing.entry((*of, name)) {
                        Entry::Vacant(entry) => {
                            entry.insert(unit);
                        }
                        Entry::Occupied(entry) => {
                            let other = &units[*entry.get()];
                            if other.package != declared.package {
                                return Err(Error::Override(format!(
                                    "{} of {} is replaced twice: by {} in {} and by {} in {}",
                                    mark::name(name),
                                    names[*of],
                                    names[other.package],
                                    mark::path(other.path.display()),
                                    names[declared.package],
                                    mark::path(declared.path.display())
                                )));
                            }
                        }
                    }
                }
            }
        }

        let mut dropped = vec![false; units.len()];
        for (unit, declared) in units.iter().enumerate() {
            let replaced = |name: &str| replacing.contains_key(&(declared.package, name));
            let Some(&name) = declared.names.iter().find(|name| replaced(name)) else {
                continue;
            };
            if let Some(kept) = declared.names.iter().find(|name| !replaced(name)) {
                let by = &units[replacing[&(declared.package, name)]];
                return Err(Error::Override(format!(
                    "{} declares {}, which {} replaces, and {}, which nothing does, but a \
                     file is kept or dropped whole",
                    mark::path(declared.path.display()),
                    mark::name(name),
                    names[by.package],
                    mark::name(kept)
                )));
            }
            dropped[unit] = true;
        }
        Ok(dropped)
    }
}
