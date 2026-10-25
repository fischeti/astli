//! What a package's features add, and which a design enables.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Dependencies, Error, Package, Requirement, mark};

/// What a feature adds to a package when enabled. Features only add, so any
/// combination is a design, and all of them together is too.
#[derive(Debug, Clone, Default)]
pub struct Feature {
    /// Patterns, relative to the root and matched as `exclude`'s are, for
    /// the files only the feature brings. A file a feature names is walked
    /// only when one naming it is enabled.
    pub sources: Vec<String>,
    /// Header roots it adds to the package's, absolute and inside the root.
    pub include_dirs: Vec<PathBuf>,
    /// The dependencies it brings into the design, each with the feature it
    /// enables there, if any: `name/feature`, or `name` alone. A dependency
    /// a feature names alone is optional, in the design only while a
    /// feature naming it is enabled.
    pub dependencies: Vec<(String, Option<String>)>,
}

/// Which features a run enables: the root's own, and its dependencies'.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// None of the root's own features but those named, rather than all.
    pub no_features: bool,
    /// More of the root's own to enable, by name. A dependency's are the
    /// root's to enable through a feature of its own, which resolution then
    /// covers.
    pub features: Vec<String>,
}

impl Package {
    /// Its include directories, then those of its enabled features: what it
    /// searches first, and what a dependent searches of it.
    pub fn search_dirs(&self) -> Vec<PathBuf> {
        let enabled = (self.enabled.iter()).filter_map(|name| self.features.get(name));
        (self.include_dirs.iter())
            .chain(enabled.flat_map(|feature| &feature.include_dirs))
            .cloned()
            .collect()
    }
}

impl Package {
    /// What it requires while the features `enabled` are: each dependency
    /// and dev-dependency, but an optional one only while an enabled feature
    /// names it.
    fn required<'a>(
        &'a self,
        enabled: &'a BTreeSet<String>,
    ) -> impl Iterator<Item = (&'a str, &'a Requirement)> {
        let optional = move |name: &str| {
            (self.features.values())
                .flat_map(|feature| &feature.dependencies)
                .any(|(dependency, feature)| dependency == name && feature.is_none())
        };
        let brought = move |name: &str| {
            (enabled.iter())
                .filter_map(|feature| self.features.get(feature))
                .flat_map(|feature| &feature.dependencies)
                .any(|(dependency, _)| dependency == name)
        };
        (self.dependencies.iter())
            .chain(&self.dev_dependencies)
            .filter(move |(name, _)| !optional(name) || brought(name))
            .map(|(name, requirement)| (name.as_str(), requirement))
    }
}

impl Dependencies {
    /// Settles, once for the design, which dependencies are in it and which
    /// features `root` and each enable, and drops those outside it.
    ///
    /// A dependency is in the design when one in it requires it, an
    /// optional one only while a feature bringing it in is enabled. The root
    /// enables all its own features unless `selection` says otherwise; a
    /// dependency, its defaults, unless every requirement of it turns them
    /// off. Each then gets every feature anything asks of it: a requirement
    /// or an enabled feature. Two packages asking for different features of
    /// one get both, since features only add. A feature nobody declares, or
    /// a package not in the design, is an error naming who asked.
    pub fn select(&mut self, root: &mut Package, selection: &Selection) -> Result<(), Error> {
        if let Some(asked) = selection.features.iter().find(|it| it.contains('/')) {
            return Err(Error::Selection(format!(
                "{} enables the package's own features, and {} is a dependency's: give the \
                 package a feature that enables it",
                mark::code("--features"),
                mark::name(asked)
            )));
        }
        let (in_design, enabled, direct) = {
            let (packages, names, at) = self.indexed(root);
            let find = |name: &str, asker: &str| {
                at.get(name).copied().ok_or_else(|| {
                    Error::Selection(format!(
                        "{asker} asks for features of {}, which is not in the design",
                        mark::name(name)
                    ))
                })
            };

            // Which are in the design and which features each enables grow
            // together, an enabled feature bringing in a dependency and a
            // dependency asking for features, until neither changes.
            let mut in_design = vec![false; packages.len()];
            let mut enabled = vec![BTreeSet::new(); packages.len()];
            loop {
                let direct = (packages.iter().zip(&enabled).enumerate())
                    .map(|(asker, (package, on))| {
                        (package.required(on))
                            .map(|(name, requirement)| {
                                Ok((find(name, &names[asker])?, requirement))
                            })
                            .collect::<Result<Vec<_>, Error>>()
                    })
                    .collect::<Result<Vec<_>, Error>>()?;

                let mut reached = vec![false; packages.len()];
                reached[0] = true;
                let mut open = vec![0];
                while let Some(package) = open.pop() {
                    for &(to, _) in &direct[package] {
                        if !reached[to] {
                            reached[to] = true;
                            open.push(to);
                        }
                    }
                }

                // What is asked, of which package, and by whom.
                let mut open: Vec<(usize, String, String)> = Vec::new();
                if !selection.no_features {
                    for name in root.features.keys() {
                        open.push((0, name.clone(), names[0].clone()));
                    }
                }
                for name in &selection.features {
                    open.push((0, name.clone(), mark::code("--features")));
                }
                let required = (direct.iter().enumerate())
                    .filter(|(asker, _)| reached[*asker])
                    .flat_map(|(asker, required)| required.iter().map(move |it| (asker, it)));
                let mut defaults = vec![false; packages.len()];
                for (asker, &(to, requirement)) in required {
                    defaults[to] |= requirement.default_features;
                    for name in &requirement.features {
                        open.push((to, name.clone(), names[asker].clone()));
                    }
                }
                for (at, package) in packages.iter().enumerate().filter(|(at, _)| defaults[*at]) {
                    for name in &package.default_features {
                        open.push((at, name.clone(), names[at].clone()));
                    }
                }

                let mut next = vec![BTreeSet::new(); packages.len()];
                while let Some((at, name, asker)) = open.pop() {
                    let Some(feature) = packages[at].features.get(&name) else {
                        return Err(Error::Selection(format!(
                            "{asker} enables {} of {}, which declares no such feature",
                            mark::name(&name),
                            names[at]
                        )));
                    };
                    if !next[at].insert(name) {
                        continue;
                    }
                    for (dependency, name) in &feature.dependencies {
                        if let Some(name) = name {
                            let to = find(dependency, &names[at])?;
                            open.push((to, name.clone(), names[at].clone()));
                        }
                    }
                }

                if reached == in_design && next == enabled {
                    let direct: Vec<Vec<usize>> = (direct.iter())
                        .map(|required| required.iter().map(|&(to, _)| to).collect())
                        .collect();
                    break (in_design, enabled, direct);
                }
                in_design = reached;
                enabled = next;
            }
        };

        // What each needs, directly or not, in the order located.
        let needs = |start: usize| -> Vec<String> {
            let mut seen = vec![false; direct.len()];
            let mut open = vec![start];
            while let Some(package) = open.pop() {
                for &to in &direct[package] {
                    if !seen[to] {
                        seen[to] = true;
                        open.push(to);
                    }
                }
            }
            (self.packages.iter().enumerate())
                .filter(|(at, _)| seen[at + 1])
                .map(|(_, dependency)| dependency.name.clone())
                .collect()
        };
        let needs: Vec<Vec<String>> = (1..direct.len()).map(needs).collect();

        let mut enabled = enabled.into_iter();
        root.enabled = enabled.next().unwrap_or_default();
        let packages = std::mem::take(&mut self.packages);
        self.packages = (packages.into_iter().zip(enabled).zip(needs).enumerate())
            .filter(|(at, _)| in_design[at + 1])
            .map(|(_, ((mut dependency, enabled), needs))| {
                dependency.package.enabled = enabled;
                dependency.needs = needs;
                dependency
            })
            .collect();
        Ok(())
    }
}

/// The `[features]` table, each entry checked: the features by name, and
/// `default`.
pub(crate) fn read(
    table: toml::Table,
    wrong: &dyn Fn(String) -> Error,
    checked: &dyn Fn(&str, &Path) -> Result<PathBuf, Error>,
) -> Result<(BTreeMap<String, Feature>, Vec<String>), Error> {
    let mut features = BTreeMap::new();
    let mut default = None;
    for (name, value) in table {
        let message = |err: toml::de::Error| {
            format!(
                "{}: {}",
                mark::code(format!("features.{name}")),
                err.message()
            )
        };
        if name == "default" {
            let names: Vec<String> = value.try_into().map_err(|err| wrong(message(err)))?;
            default = Some(names);
            continue;
        }
        if name.is_empty() || name.contains('/') {
            return Err(wrong(format!(
                "{}: {} is not a feature's name, which holds no {}",
                mark::code("features"),
                mark::name(&name),
                mark::code("/")
            )));
        }
        let written: Written = value.try_into().map_err(|err| wrong(message(err)))?;
        let include_dirs = (written.include_dirs.iter())
            .map(|dir| checked(&format!("features.{name}.include-dirs"), dir))
            .collect::<Result<_, _>>()?;
        let dependencies = (written.dependencies.iter())
            .map(|asked| match asked.split_once('/') {
                Some((package, feature)) if !package.is_empty() && !feature.is_empty() => {
                    Ok((package.to_string(), Some(feature.to_string())))
                }
                None if !asked.is_empty() => Ok((asked.clone(), None)),
                _ => Err(wrong(format!(
                    "{}: {} is neither a dependency nor one's feature",
                    mark::code(format!("features.{name}.dependencies")),
                    mark::name(asked)
                ))),
            })
            .collect::<Result<_, _>>()?;
        let feature = Feature {
            sources: written.sources,
            include_dirs,
            dependencies,
        };
        features.insert(name, feature);
    }
    let default = default.unwrap_or_default();
    if let Some(name) = default.iter().find(|name| !features.contains_key(*name)) {
        return Err(wrong(format!(
            "{}: {} is not a feature of the package",
            mark::code("features.default"),
            mark::name(name)
        )));
    }
    Ok((features, default))
}

/// A feature's table, as written.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Written {
    sources: Vec<String>,
    include_dirs: Vec<PathBuf>,
    dependencies: Vec<String>,
}
