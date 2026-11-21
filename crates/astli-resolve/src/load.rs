//! Reading the packages a resolution chose as the dependencies of a design.

use std::collections::BTreeSet;

use astli_package::{Dependencies, Dependency, MANIFEST, Package, Root, Walk};

use crate::{Error, Located, Origin, Resolution, Store};

/// The dependencies of a design, each checked out and read as a dependent
/// sees it.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub dependencies: Dependencies,
    /// Those read as the root describes them, having no manifest, by name.
    pub described: Vec<String>,
    /// Those a `path` requirement reads from a directory on disk, directly
    /// or through another, a sibling member among them: not a patched one,
    /// nor a directory of a fetched repository, nor one only a patched one
    /// reaches, which is that repository's too.
    pub on_disk: BTreeSet<String>,
}

/// Something loading found that a run tells as a warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// The root describes the package of this name, which has a manifest of
    /// its own, read instead.
    OwnManifest(String),
    /// A pattern of the package's `exclude` in its description matches none
    /// of its files.
    Unmatched { name: String, pattern: String },
}

impl Located {
    /// Whether the root's description of the package applies to its files:
    /// only while they hold no manifest of their own, which is read instead.
    pub fn describable(&self) -> bool {
        !self.dir.join(MANIFEST).is_file()
    }
}

impl Resolution {
    /// The dependencies of `package`'s design, checked out into `store`: the
    /// part of the resolution it reaches. `root` is the root of its
    /// workspace, or the package itself outside one, as a dependency without
    /// a manifest is read as it describes it. Each note is given to `note`
    /// as found, between the checkouts' own steps and before a failure.
    pub fn load(
        &self,
        store: &Store,
        root: &Root,
        package: &Package,
        mut note: impl FnMut(Note),
    ) -> Result<Loaded, Error> {
        let reached = self.reached(package);
        let on_disk = (self.reached_through(package, |it| matches!(it.origin, Origin::Dir(_))))
            .into_iter()
            .map(String::from)
            .collect();
        let mut found = Vec::new();
        let mut described = Vec::new();
        for it in &self.packages {
            if !reached.contains(it.name.as_str()) {
                continue;
            }
            let named = |err: Error| Error::Dependency(it.name.clone(), Box::new(err));
            let located = it.checkout(store).map_err(named)?;
            let mut read = Package::at(&located.dir).map_err(|err| named(err.into()))?;
            let description = root.tables.described.get(&it.name);
            if let (Some(description), Some(manifest)) = (description, &root.package.manifest_path)
            {
                match located.describable() {
                    false => note(Note::OwnManifest(it.name.clone())),
                    true => {
                        if let Some(held) = &located.held {
                            for pattern in description.unmatched(&read.root, held) {
                                note(Note::Unmatched {
                                    name: it.name.clone(),
                                    pattern: pattern.to_string(),
                                });
                            }
                        }
                        read = read.described(&it.name, description, manifest)?;
                        described.push(it.name.clone());
                    }
                }
            }
            found.push(Dependency {
                name: it.name.clone(),
                package: read.as_dependency(),
                needs: Vec::new(),
                walk: Walk {
                    held: located.held,
                    ..Walk::default()
                },
            });
        }
        let requires = |name: &str| match self.get(name) {
            Some(it) => &it.requires[..],
            None => &[],
        };
        Ok(Loaded {
            dependencies: Dependencies::ordered(package, found, requires)?,
            described,
            on_disk,
        })
    }
}
