//! The lockfile: the commit each repository's package resolved to, which
//! the next resolution prefers.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use gix::ObjectId;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{Error, Origin, Resolution, canonical};

/// The lockfile's name, beside the root's manifest.
pub const LOCKFILE: &str = "astli.lock";

/// The layout of the file, raised when a change to it would be misread by
/// an astli that reads the one before.
const FORMAT: u32 = 1;

/// The commit chosen for each package from a repository. A package on disk,
/// or in a directory of another's repository, follows from the manifests,
/// so it is not locked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lock {
    /// By name.
    pub packages: Vec<Locked>,
}

/// A package from a repository, at the commit chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    pub name: String,
    /// The repository's URL.
    pub source: String,
    pub commit: ObjectId,
    /// The highest version a tag on the commit gives it, if any.
    pub version: Option<Version>,
    /// The `rev`s requirements named the commit by, other than its id: a
    /// branch's commit stays locked only while it is required by that
    /// branch, whatever the branch holds now.
    pub revs: Vec<String>,
    /// The packages it depends on directly, by name.
    pub dependencies: Vec<String>,
}

impl Lock {
    /// The lock `resolution` makes.
    pub fn of(resolution: &Resolution) -> Lock {
        let packages = (resolution.packages.iter())
            .filter_map(|it| match (&it.origin, it.commit) {
                (Origin::Git { url }, Some(commit)) => Some(Locked {
                    name: it.name.clone(),
                    source: url.clone(),
                    commit,
                    version: it.version.clone(),
                    revs: it.revs.clone(),
                    dependencies: it.needs.clone(),
                }),
                _ => None,
            })
            .collect();
        Lock { packages }
    }

    /// The lock at `path`, or `None` if there is no file.
    pub fn read(path: &Path) -> Result<Option<Lock>, Error> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(Error::Io(path.to_path_buf(), err)),
        };
        Lock::parse(&text)
            .map(Some)
            .map_err(|what| Error::Lock(path.to_path_buf(), what))
    }

    /// Reads a lockfile's text. Another format than this astli's is refused,
    /// an older one too, rather than read as something it never said.
    pub fn parse(text: &str) -> Result<Lock, String> {
        let table: toml::Table = toml::from_str(text).map_err(|err| err.to_string())?;
        match table.get("version").and_then(toml::Value::as_integer) {
            Some(format) if format == i64::from(FORMAT) => {}
            Some(format) if format > i64::from(FORMAT) => {
                return Err(format!(
                    "written in format {format} by a newer astli; this one reads format {FORMAT}"
                ));
            }
            Some(format) => {
                return Err(format!(
                    "written in format {format} by an older astli; this one reads format \
                     {FORMAT}, and `astli update` writes it again"
                ));
            }
            None => return Err("no format `version`".to_string()),
        }
        let file: File = table.try_into().map_err(|err| err.to_string())?;
        let mut packages = Vec::new();
        for entry in file.package {
            let commit = ObjectId::from_hex(entry.commit.as_bytes()).map_err(|_| {
                format!(
                    "`{}` is at `{}`, not a commit's id",
                    entry.name, entry.commit
                )
            })?;
            let version = (entry.version.as_deref())
                .map(Version::parse)
                .transpose()
                .map_err(|err| format!("`{}`'s version: {err}", entry.name))?;
            packages.push(Locked {
                name: entry.name,
                source: entry.source,
                commit,
                version,
                revs: entry.revs,
                dependencies: entry.dependencies,
            });
        }
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Lock { packages })
    }

    /// The package locked as `name`.
    pub fn get(&self, name: &str) -> Option<&Locked> {
        (self
            .packages
            .binary_search_by(|it| it.name.as_str().cmp(name))
            .ok())
        .map(|at| &self.packages[at])
    }

    /// What changed from `self` to `newer`, one line a package, by name.
    pub fn changes(&self, newer: &Lock) -> Vec<String> {
        let mut names: BTreeMap<&str, (Option<&Locked>, Option<&Locked>)> = BTreeMap::new();
        for old in &self.packages {
            names.entry(&old.name).or_default().0 = Some(old);
        }
        for new in &newer.packages {
            names.entry(&new.name).or_default().1 = Some(new);
        }
        let mut changes = Vec::new();
        for (name, pair) in names {
            match pair {
                (None, Some(new)) => changes.push(format!("Adding {name} {}", new.at())),
                (Some(old), None) => changes.push(format!("Removing {name} {}", old.at())),
                (Some(old), Some(new)) if canonical(&old.source) != canonical(&new.source) => {
                    changes.push(format!(
                        "Updating {name} {} of {} -> {} of {}",
                        old.at(),
                        old.source,
                        new.at(),
                        new.source
                    ));
                }
                (Some(old), Some(new)) if old.commit != new.commit => {
                    let (mut was, mut is) = (old.at(), new.at());
                    // A tag moved, so the version alone does not tell them
                    // apart.
                    if was == is {
                        (was, is) = (short(old.commit), short(new.commit));
                    }
                    changes.push(format!("Updating {name} {was} -> {is}"));
                }
                _ => {}
            }
        }
        changes
    }
}

impl Locked {
    /// Where it is locked, said for a person: its version, or its commit.
    fn at(&self) -> String {
        match &self.version {
            Some(version) => version.to_string(),
            None => short(self.commit),
        }
    }
}

/// A commit's id, as short as a person reads it.
fn short(commit: ObjectId) -> String {
    commit.to_hex_with_len(10).to_string()
}

impl fmt::Display for Lock {
    /// The file's text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = File {
            version: FORMAT,
            package: (self.packages.iter())
                .map(|it| Entry {
                    name: it.name.clone(),
                    source: it.source.clone(),
                    commit: it.commit.to_string(),
                    version: it.version.as_ref().map(Version::to_string),
                    revs: it.revs.clone(),
                    dependencies: it.dependencies.clone(),
                })
                .collect(),
        };
        let text = toml::to_string(&file).map_err(|_| fmt::Error)?;
        writeln!(
            f,
            "# Written by astli. `astli update` changes it; edit the manifest instead."
        )?;
        write!(f, "{text}")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    #[serde(default)]
    package: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: String,
    source: String,
    commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    revs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::Lock;

    const ENTRY: &str = "[[package]]\nname = 'axi'\nsource = 'https://github.com/pulp-platform/axi'\ncommit = '0d337aa3cb51d34f149e3666ab574f6ccda193ea'\n";

    #[test]
    fn another_format_is_refused_whichever_way_it_differs() {
        let newer = Lock::parse(&format!("version = 2\n{ENTRY}")).unwrap_err();
        let older = Lock::parse(&format!("version = 0\n{ENTRY}")).unwrap_err();
        let none = Lock::parse(ENTRY).unwrap_err();

        assert!(newer.contains("newer astli"), "{newer}");
        assert!(older.contains("`astli update`"), "{older}");
        assert!(none.contains("`version`"), "{none}");
    }

    #[test]
    fn a_lock_reads_back_as_it_was_written() {
        let lock = Lock::parse(&format!(
            "version = 1\n{ENTRY}version = '0.39.1'\nrevs = ['main']\ndependencies = ['common_cells']\n"
        ))
        .unwrap();

        let text = lock.to_string();

        assert_eq!(Lock::parse(&text), Ok(lock));
        assert!(text.contains("[[package]]\nname = \"axi\""), "{text}");
    }

    #[test]
    fn a_field_it_does_not_know_is_refused() {
        let err = Lock::parse(&format!("version = 1\n{ENTRY}features = []\n")).unwrap_err();

        assert!(err.contains("features"), "{err}");
    }
}
