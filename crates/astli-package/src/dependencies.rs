//! A package's dependencies, each a package of its own, and where `bender`
//! has them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Error, Package};

/// The manifest `bender` reads.
const BENDER_MANIFEST: &str = "Bender.yml";

/// A package another depends on, read as any package is, wherever it was
/// found.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// What the packages depending on it call it.
    pub name: String,
    pub package: Package,
    /// The dependencies it depends on, directly or not, by name.
    pub needs: Vec<String>,
}

/// Every package the root depends on, directly or not.
#[derive(Debug, Clone, Default)]
pub struct Dependencies {
    /// In the order located, each named once.
    pub packages: Vec<Dependency>,
}

impl Dependencies {
    /// The include directories the root searches after its own: those of
    /// every dependency, since it depends on each, directly or not.
    pub fn include_dirs(&self) -> Vec<PathBuf> {
        (self.packages.iter())
            .flat_map(|dependency| dependency.package.include_dirs.iter().cloned())
            .collect()
    }

    /// The include directories `dependency` searches after its own: those of
    /// the dependencies it needs, in the order located.
    pub fn include_dirs_of(&self, dependency: &Dependency) -> Vec<PathBuf> {
        (self.packages.iter())
            .filter(|other| dependency.needs.contains(&other.name))
            .flat_map(|other| other.package.include_dirs.iter().cloned())
            .collect()
    }
}

impl Package {
    /// The dependencies `bender` resolves for the package, or `None` if the
    /// root holds no `Bender.yml`.
    ///
    /// Only which dependencies there are, which each depends on, and where
    /// each is checked out is asked of `bender`, which may fetch them first.
    /// What files a dependency holds is its own `astli.toml`'s to say, or
    /// the defaults'.
    pub fn bender(&self) -> Result<Option<Dependencies>, Error> {
        if !self.root.join(BENDER_MANIFEST).is_file() {
            return Ok(None);
        }
        let names = bender(&self.root, &["packages", "--flat"])?;
        let names: Vec<&str> = names.split_whitespace().collect();
        let graph = bender(&self.root, &["packages", "--graph", "--flat"])?;
        let paths = match names.is_empty() {
            true => String::new(),
            false => bender(&self.root, &[&["path"], names.as_slice()].concat())?,
        };

        let packages = (located(&names, &graph, &paths)?.into_iter())
            .map(|(name, dir, needs)| {
                Ok(Dependency {
                    name,
                    package: Package::at(&dir)?,
                    needs,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Some(Dependencies { packages }))
    }
}

/// Each dependency `bender` named, the directory it is at, and what it
/// needs, from what `bender packages --graph --flat` and `bender path`
/// printed.
fn located(
    names: &[&str],
    graph: &str,
    paths: &str,
) -> Result<Vec<(String, PathBuf, Vec<String>)>, Error> {
    let paths: Vec<&str> = (paths.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if paths.len() != names.len() {
        return Err(Error::Bender(format!(
            "`bender path` gave {} paths for {} dependencies",
            paths.len(),
            names.len()
        )));
    }

    // Each line names a package and one it depends on directly.
    let mut direct: HashMap<&str, Vec<&str>> = HashMap::new();
    for line in graph.lines() {
        let mut words = line.split_whitespace();
        if let (Some(package), Some(needed)) = (words.next(), words.next()) {
            direct.entry(package).or_default().push(needed);
        }
    }
    let needs = |name: &str| -> Vec<String> {
        let mut seen = HashSet::new();
        let mut open = vec![name];
        while let Some(package) = open.pop() {
            for &needed in direct.get(package).into_iter().flatten() {
                if seen.insert(needed) {
                    open.push(needed);
                }
            }
        }
        // In the order located, so that the search order is `bender`'s.
        (names.iter())
            .filter(|name| seen.contains(**name))
            .map(|name| name.to_string())
            .collect()
    };

    Ok((names.iter().zip(paths))
        .map(|(name, path)| (name.to_string(), PathBuf::from(path), needs(name)))
        .collect())
}

/// What `bender` prints for `args` in `root`.
fn bender(root: &Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("bender")
        .arg("--dir")
        .arg(root)
        .args(args)
        .output()
        .map_err(|err| Error::Bender(format!("cannot run `bender`: {err}")))?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Bender(format!(
            "`bender {}` failed: {}",
            args.join(" "),
            said.trim()
        )));
    }
    String::from_utf8(output.stdout).map_err(|_| {
        Error::Bender(format!(
            "`bender {}` printed what is not UTF-8",
            args.join(" ")
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::located;

    const GRAPH: &str = "\
        cells               verif\n\
        cells               tech\n\
        tech                verif\n";

    #[test]
    fn each_dependency_is_at_its_path_and_needs_its_closure() {
        let names = ["verif", "tech", "cells"];
        let paths = "/deps/verif\n/deps/tech\n/deps/cells\n";

        let located = located(&names, GRAPH, paths).unwrap();

        let (name, dir, needs) = &located[2];
        assert_eq!(name, "cells");
        assert_eq!(dir, &PathBuf::from("/deps/cells"));
        // In the order located, which is `bender`'s.
        assert_eq!(needs, &["verif", "tech"]);
        assert!(located[0].2.is_empty());
    }

    #[test]
    fn a_path_missing_is_refused() {
        let err = located(&["verif", "tech"], GRAPH, "/deps/verif\n").unwrap_err();
        assert!(err.to_string().contains("gave 1 paths for 2"), "{err}");
    }
}
