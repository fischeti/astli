//! A package's dependencies, each a package of its own, in the order they
//! search each other's headers.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::{Error, Package};

/// A package another depends on, read as any package is, wherever it was
/// found.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// What the packages depending on it call it.
    pub name: String,
    /// As a dependent sees it, without its `dev` files, and with the
    /// features [`Dependencies::select`] settles.
    pub package: Package,
    /// The dependencies it depends on, directly or not, by name.
    pub needs: Vec<String>,
}

/// Every package the root depends on, directly or not.
#[derive(Debug, Clone, Default)]
pub struct Dependencies {
    /// Each after those it depends on, each named once.
    pub packages: Vec<Dependency>,
}

impl Dependencies {
    /// The include directories the root searches after its own: those of
    /// every dependency, since it depends on each, directly or not.
    pub fn include_dirs(&self) -> Vec<PathBuf> {
        (self.packages.iter())
            .flat_map(|dependency| dependency.package.search_dirs())
            .collect()
    }

    /// The root and then each dependency, the name an error calls each by,
    /// and each dependency's place among them by its name.
    pub(crate) fn indexed<'a>(
        &'a self,
        root: &'a Package,
    ) -> (Vec<&'a Package>, Vec<String>, HashMap<&'a str, usize>) {
        let packages = std::iter::once(root)
            .chain(self.packages.iter().map(|dependency| &dependency.package))
            .collect();
        let root_name = match &root.name {
            Some(name) => format!("`{name}`"),
            None => "the root package".to_string(),
        };
        let names = std::iter::once(root_name)
            .chain(self.packages.iter().map(|it| format!("`{}`", it.name)))
            .collect();
        let at = (self.packages.iter().enumerate())
            .map(|(at, dependency)| (dependency.name.as_str(), at + 1))
            .collect();
        (packages, names, at)
    }

    /// The include directories `dependency` searches after its own: those of
    /// the dependencies it needs, in the order located.
    pub fn include_dirs_of(&self, dependency: &Dependency) -> Vec<PathBuf> {
        (self.packages.iter())
            .filter(|other| dependency.needs.contains(&other.name))
            .flat_map(|other| other.package.search_dirs())
            .collect()
    }
}

impl Dependencies {
    /// The dependencies resolution found, each with the names of those it
    /// depends on directly, ordered as `bender` orders them: the farthest
    /// from the root first, ties by name, where a package is as far as the
    /// longest chain of requirements from the root to it. Each then comes
    /// after those it requires, and searches their include directories in
    /// that order. A cycle is an error naming the packages in it.
    pub fn ordered(
        root: &Package,
        found: Vec<(String, Package, Vec<String>)>,
    ) -> Result<Dependencies, Error> {
        let at: HashMap<&str, usize> = (found.iter().enumerate())
            .map(|(at, (name, _, _))| (name.as_str(), at))
            .collect();
        let index = |names: &mut dyn Iterator<Item = &String>| -> Vec<usize> {
            names
                .filter_map(|name| at.get(name.as_str()).copied())
                .collect()
        };
        let names: Vec<String> = found.iter().map(|(name, _, _)| name.clone()).collect();
        let required: Vec<Vec<usize>> = (found.iter())
            .map(|(_, _, needs)| index(&mut needs.iter()))
            .collect();
        let root_requires =
            index(&mut root.dependencies.keys().chain(root.dev_dependencies.keys()));
        let order = ordered(&names, &root_requires, &required)?;

        let place: HashMap<usize, usize> = (order.iter().enumerate())
            .map(|(place, &package)| (package, place))
            .collect();
        let mut found: Vec<Option<(String, Package, Vec<String>)>> =
            found.into_iter().map(Some).collect();
        let packages = (order.iter())
            .map(|&package| {
                let (name, package_of, _) = found[package].take().expect("each once");
                let mut seen = vec![false; names.len()];
                let mut open = required[package].clone();
                while let Some(needed) = open.pop() {
                    if !std::mem::replace(&mut seen[needed], true) {
                        open.extend(&required[needed]);
                    }
                }
                let mut needs: Vec<usize> = (0..names.len()).filter(|&it| seen[it]).collect();
                needs.sort_by_key(|it| place[it]);
                Dependency {
                    name,
                    package: package_of,
                    needs: needs.into_iter().map(|it| names[it].clone()).collect(),
                }
            })
            .collect();
        Ok(Dependencies { packages })
    }
}

/// The packages named `names` in the order [`Dependencies::ordered`] gives,
/// `root` those the root requires and `required` what each requires.
fn ordered(names: &[String], root: &[usize], required: &[Vec<usize>]) -> Result<Vec<usize>, Error> {
    // Each package's depth is settled once every package requiring it is.
    let mut waiting = vec![0; names.len()];
    for &to in root.iter().chain(required.iter().flatten()) {
        waiting[to] += 1;
    }
    let mut depth = vec![0; names.len()];
    let mut settled = 0;
    // How far each requirer is, and what it requires.
    let mut open = vec![(0, root.to_vec())];
    while let Some((from, requires)) = open.pop() {
        for to in requires {
            depth[to] = depth[to].max(from + 1);
            waiting[to] -= 1;
            if waiting[to] == 0 {
                settled += 1;
                open.push((depth[to], required[to].clone()));
            }
        }
    }

    if settled < names.len() {
        // In a cycle, rather than only below one: it reaches itself.
        let reaches_itself = |start: usize| {
            let mut seen = vec![false; names.len()];
            let mut open = required[start].clone();
            while let Some(package) = open.pop() {
                if !std::mem::replace(&mut seen[package], true) {
                    open.extend(&required[package]);
                }
            }
            seen[start]
        };
        let mut cycle: Vec<&str> = (0..names.len())
            .filter(|&package| reaches_itself(package))
            .map(|package| names[package].as_str())
            .collect();
        cycle.sort();
        return Err(Error::Dependency(format!(
            "`{}` depend on each other",
            cycle.join("`, `")
        )));
    }
    let mut order: Vec<usize> = (0..names.len()).collect();
    order.sort_by(|&a, &b| depth[b].cmp(&depth[a]).then(names[a].cmp(&names[b])));
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::ordered;

    fn named(names: &[&str]) -> Vec<String> {
        names.iter().map(|it| it.to_string()).collect()
    }

    #[test]
    fn the_farthest_from_the_root_come_first_ties_by_name() {
        // axi needs cells and verif; cells needs verif and tech.
        let names = named(&["axi", "cells", "tech", "verif", "zz"]);
        let required = [vec![1, 3], vec![3, 2], vec![], vec![], vec![]];

        let order = ordered(&names, &[1, 4, 0], &required).unwrap();

        assert_eq!(order, [2, 3, 1, 0, 4]);
    }

    #[test]
    fn a_cycle_is_refused() {
        let names = named(&["a", "b", "c"]);
        let required = [vec![1], vec![0], vec![]];

        let err = ordered(&names, &[0, 2], &required).unwrap_err();

        assert!(
            err.to_string().contains("`a`, `b` depend on each other"),
            "{err}"
        );
    }
}
