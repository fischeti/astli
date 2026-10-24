//! What a package's features add, and which a design enables.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Dependencies, Error, Package};

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
    /// The features of dependencies it enables, each a dependency's name and
    /// one of its features, written `name/feature`.
    pub dependencies: Vec<(String, String)>,
}

/// Which features a run enables: the root's own, and its dependencies'.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// None of the root's own features but those named, rather than all.
    pub no_features: bool,
    /// More to enable: the root's own by name, a dependency's as
    /// `name/feature`.
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

impl Dependencies {
    /// Settles, once for the design, which features `root` and each
    /// dependency enable.
    ///
    /// The root enables all its own unless `selection` says otherwise; a
    /// dependency, its defaults. Each then gets every feature anything asks
    /// of it: `[dependencies]`, an enabled feature, or the command line.
    /// Two packages asking for different features of one get both, since
    /// features only add. A feature nobody declares, or a package not in the
    /// design, is an error naming who asked.
    pub fn select(&mut self, root: &mut Package, selection: &Selection) -> Result<(), Error> {
        let enabled = {
            let (packages, names, at) = self.indexed(root);
            let find = |name: &str, asker: &str| {
                at.get(name).copied().ok_or_else(|| {
                    Error::Selection(format!(
                        "{asker} asks for features of `{name}`, which is not in the design"
                    ))
                })
            };

            // What is asked, of which package, and by whom.
            let mut open: Vec<(usize, String, String)> = Vec::new();
            if !selection.no_features {
                for name in root.features.keys() {
                    open.push((0, name.clone(), names[0].clone()));
                }
            }
            for asked in &selection.features {
                let (at, name) = match asked.split_once('/') {
                    Some((package, name)) => (find(package, "`--features`")?, name),
                    None => (0, asked.as_str()),
                };
                open.push((at, name.to_string(), "`--features`".to_string()));
            }
            for (at, package) in packages.iter().enumerate().skip(1) {
                for name in &package.default_features {
                    open.push((at, name.clone(), names[at].clone()));
                }
            }
            for (asker, package) in packages.iter().enumerate() {
                for (dependency, requirement) in &package.dependencies {
                    let at = find(dependency, &names[asker])?;
                    for name in &requirement.features {
                        open.push((at, name.clone(), names[asker].clone()));
                    }
                }
            }

            let mut enabled = vec![BTreeSet::new(); packages.len()];
            while let Some((at, name, asker)) = open.pop() {
                let Some(feature) = packages[at].features.get(&name) else {
                    return Err(Error::Selection(format!(
                        "{asker} enables `{name}` of {}, which declares no such feature",
                        names[at]
                    )));
                };
                if !enabled[at].insert(name) {
                    continue;
                }
                for (dependency, name) in &feature.dependencies {
                    let to = find(dependency, &names[at])?;
                    open.push((to, name.clone(), names[at].clone()));
                }
            }
            enabled
        };

        let mut enabled = enabled.into_iter();
        root.enabled = enabled.next().unwrap_or_default();
        for (dependency, enabled) in self.packages.iter_mut().zip(enabled) {
            dependency.package.enabled = enabled;
        }
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
        let message = |err: toml::de::Error| format!("`features.{name}`: {}", err.message());
        if name == "default" {
            let names: Vec<String> = value.try_into().map_err(|err| wrong(message(err)))?;
            default = Some(names);
            continue;
        }
        if name.is_empty() || name.contains('/') {
            return Err(wrong(format!(
                "`features`: `{name}` is not a feature's name, which holds no `/`"
            )));
        }
        let written: Written = value.try_into().map_err(|err| wrong(message(err)))?;
        let include_dirs = (written.include_dirs.iter())
            .map(|dir| checked(&format!("features.{name}.include-dirs"), dir))
            .collect::<Result<_, _>>()?;
        let dependencies = (written.dependencies.iter())
            .map(|asked| match asked.split_once('/') {
                Some((package, feature)) if !package.is_empty() && !feature.is_empty() => {
                    Ok((package.to_string(), feature.to_string()))
                }
                _ => Err(wrong(format!(
                    "`features.{name}.dependencies`: `{asked}` names no feature, as \
                     `{asked}/<feature>` would"
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
            "`features.default`: `{name}` is not a feature of the package"
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
