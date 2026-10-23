//! What a package requires of another, from either manifest: the
//! `[dependencies]` of its `astli.toml`, or without that table, the
//! `dependencies` of its `Bender.yml`.
//!
//! Both are read from text alone, so that resolution can read a manifest at
//! any commit of a mirror. A path stays as written, relative to the
//! manifest, until a package read from a directory makes it absolute.

use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::VersionReq;
use serde::Deserialize;
use serde::de::IgnoredAny;

/// What a package asks of one it depends on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Requirement {
    /// Where it comes from; `None` while `bender` locates it.
    pub source: Option<Source>,
    /// The features it enables there.
    pub features: Vec<String>,
}

/// Where a dependency comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A repository, at what `at` picks of it.
    Git { url: String, at: At },
    /// A directory.
    Path(PathBuf),
}

/// Which commit of a repository a requirement wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum At {
    /// The highest version its tags give that the requirement admits.
    Version(VersionReq),
    /// A branch, a tag or a commit, as git reads it.
    Rev(String),
}

/// An entry of `[dependencies]` in an `astli.toml`, as written.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct AstliEntry {
    git: Option<String>,
    version: Option<String>,
    rev: Option<String>,
    path: Option<PathBuf>,
    #[serde(default)]
    features: Vec<String>,
}

impl AstliEntry {
    /// The requirement, or why the entry is not one; the message names the
    /// key it is about, under `dependencies.<name>`.
    pub(crate) fn read(self, name: &str) -> Result<Requirement, String> {
        let key = format!("dependencies.{name}");
        let source = match (self.git, self.path) {
            (Some(_), Some(_)) => return Err(format!("`{key}`: both `git` and `path`")),
            (Some(url), None) => Some(Source::Git {
                url,
                at: at(&key, self.version, self.rev)?,
            }),
            (None, path) => {
                if self.version.is_some() || self.rev.is_some() {
                    return Err(format!("`{key}`: `version` and `rev` need `git`"));
                }
                path.map(Source::Path)
            }
        };
        Ok(Requirement {
            source,
            features: self.features,
        })
    }
}

/// The dependencies the `Bender.yml` `text` lists, by name, lowercased as
/// bender names them; or why it cannot be read. Nothing else of the file is
/// read.
///
/// An entry is a repository with `git` and a `version` or a `rev`, a `path`,
/// or a bare version of the repository a remote names. A dependency with a
/// `target` is left out, since astli has no targets: it belongs to some
/// builds, not to the design.
pub fn bender_dependencies(text: &str) -> Result<BTreeMap<String, Requirement>, String> {
    let file: BenderFile = serde_saphyr::from_str(text).map_err(|err| err.to_string())?;
    let mut dependencies = BTreeMap::new();
    for (name, entry) in file.dependencies.unwrap_or_default() {
        let key = format!("dependencies.{name}");
        let source = match entry {
            BenderEntry::Version(version) => {
                let url = remote(&file.remotes, None, &name, &key)?;
                let at = at(&key, Some(version), None)?;
                Source::Git { url, at }
            }
            BenderEntry::Table(table) => {
                if table.target.is_some() {
                    continue;
                }
                match (table.git, table.path) {
                    (Some(_), Some(_)) => return Err(format!("`{key}`: both `git` and `path`")),
                    (Some(url), None) => Source::Git {
                        url,
                        at: at(&key, table.version, table.rev)?,
                    },
                    (None, Some(path)) => Source::Path(path),
                    (None, None) => {
                        let url = remote(&file.remotes, table.remote.as_deref(), &name, &key)?;
                        Source::Git {
                            url,
                            at: at(&key, table.version, table.rev)?,
                        }
                    }
                }
            }
        };
        let requirement = Requirement {
            source: Some(source),
            features: Vec::new(),
        };
        dependencies.insert(name.to_lowercase(), requirement);
    }
    Ok(dependencies)
}

/// A `version` or a `rev`, exactly one.
fn at(key: &str, version: Option<String>, rev: Option<String>) -> Result<At, String> {
    match (version, rev) {
        (Some(version), None) => match VersionReq::parse(&version) {
            Ok(required) => Ok(At::Version(required)),
            Err(err) => Err(format!(
                "`{key}.version`: `{version}` is not a version requirement: {err}"
            )),
        },
        (None, Some(rev)) => Ok(At::Rev(rev)),
        (Some(_), Some(_)) => Err(format!("`{key}`: both `version` and `rev`")),
        (None, None) => Err(format!("`{key}`: a repository needs `version` or `rev`")),
    }
}

/// The URL of `name` in the remote `named`, or in the default one: the only
/// remote, or the one marked `default`. A remote's URL holds `{}` where the
/// name goes, or else the name follows it as a path component.
fn remote(
    remotes: &BTreeMap<String, Remote>,
    named: Option<&str>,
    name: &str,
    key: &str,
) -> Result<String, String> {
    let chosen = match named {
        Some(named) => remotes
            .get(named)
            .ok_or_else(|| format!("`{key}.remote`: `{named}` is not one of `remotes`"))?,
        None => {
            let defaults: Vec<&Remote> = match remotes.len() {
                1 => remotes.values().collect(),
                _ => remotes.values().filter(|it| it.is_default()).collect(),
            };
            match defaults[..] {
                [remote] => remote,
                _ => {
                    return Err(format!(
                        "`{key}`: a bare version needs a default remote, the only one in \
                         `remotes` or the one marked `default`"
                    ));
                }
            }
        }
    };
    let url = chosen.url().trim_end_matches('/');
    Ok(match url.contains("{}") {
        true => url.replace("{}", name),
        false => format!("{url}/{name}.git"),
    })
}

/// A `Bender.yml`, as far as dependencies go.
#[derive(Deserialize)]
struct BenderFile {
    #[serde(default)]
    remotes: BTreeMap<String, Remote>,
    /// `None` where the key is written with nothing after it.
    dependencies: Option<BTreeMap<String, BenderEntry>>,
}

/// An entry of `remotes`: its URL, or a table that may mark it the default.
#[derive(Deserialize)]
#[serde(untagged)]
enum Remote {
    Url(String),
    Table {
        url: String,
        #[serde(default)]
        default: bool,
    },
}

impl Remote {
    fn url(&self) -> &str {
        match self {
            Remote::Url(url) | Remote::Table { url, .. } => url,
        }
    }

    fn is_default(&self) -> bool {
        matches!(self, Remote::Table { default: true, .. })
    }
}

/// An entry of `dependencies`: a bare version, or a table.
enum BenderEntry {
    Version(String),
    Table(BenderTable),
}

/// Told apart by hand rather than by trying each in turn, so that a
/// mistake is reported as what it is, not as matching neither.
impl<'de> Deserialize<'de> for BenderEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = BenderEntry;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a version, or a table with `git`, `path` or `version`")
            }

            fn visit_str<E: serde::de::Error>(self, version: &str) -> Result<BenderEntry, E> {
                Ok(BenderEntry::Version(version.to_string()))
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<BenderEntry, M::Error> {
                let table =
                    BenderTable::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(BenderEntry::Table(table))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// A dependency's table. bender's keys that do not say where a dependency
/// is, such as `pass_targets`, are ignored.
#[derive(Deserialize)]
struct BenderTable {
    git: Option<String>,
    version: Option<String>,
    rev: Option<String>,
    path: Option<PathBuf>,
    remote: Option<String>,
    target: Option<IgnoredAny>,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use semver::VersionReq;

    use super::{At, Requirement, Source, bender_dependencies};

    fn git(url: &str, at: At) -> Requirement {
        Requirement {
            source: Some(Source::Git {
                url: url.to_string(),
                at,
            }),
            features: Vec::new(),
        }
    }

    fn version(required: &str) -> At {
        At::Version(VersionReq::parse(required).unwrap())
    }

    #[test]
    fn each_form_of_entry_is_read() {
        let text = "\
package:
  name: soc
remotes:
  pulp: https://github.com/pulp-platform
dependencies:
  axi: { git: \"https://github.com/pulp-platform/axi.git\", version: 0.39.8 }
  iDMA:
    git: https://github.com/pulp-platform/iDMA.git
    rev: 78ab25f13a88658be3f7b4d1d7b1074effaf42eb
    upstream_name: iDMA
  common_verification: 0.2.5
  pd: { path: ./pd }
  synth_only: { path: ./synth, target: floo_synth }
sources:
  - target: rtl
    files: [src/soc.sv]
";
        let read = bender_dependencies(text).unwrap();

        assert!(read.keys().eq(["axi", "common_verification", "idma", "pd"]));
        assert_eq!(
            read["axi"],
            git(
                "https://github.com/pulp-platform/axi.git",
                version("0.39.8")
            )
        );
        assert_eq!(
            read["idma"],
            git(
                "https://github.com/pulp-platform/iDMA.git",
                At::Rev("78ab25f13a88658be3f7b4d1d7b1074effaf42eb".to_string())
            )
        );
        assert_eq!(
            read["common_verification"],
            git(
                "https://github.com/pulp-platform/common_verification.git",
                version("0.2.5")
            )
        );
        assert_eq!(read["pd"].source, Some(Source::Path(PathBuf::from("./pd"))));
    }

    #[test]
    fn a_remote_is_a_template_or_a_default() {
        let text = "\
remotes:
  pulp: { url: \"https://github.com/pulp-platform\", default: true }
  mine: \"git@example.com:{}.git\"
dependencies:
  cells: 1.38.0
  tech: { version: 0.2.14, remote: mine }
";
        let read = bender_dependencies(text).unwrap();

        assert_eq!(
            read["cells"],
            git(
                "https://github.com/pulp-platform/cells.git",
                version("1.38.0")
            )
        );
        assert_eq!(
            read["tech"],
            git("git@example.com:tech.git", version("0.2.14"))
        );
    }

    #[test]
    fn no_dependencies_is_none() {
        assert!(
            bender_dependencies("package:\n  name: leaf\n")
                .unwrap()
                .is_empty()
        );
        assert!(bender_dependencies("dependencies:\n").unwrap().is_empty());
    }

    #[test]
    fn what_bender_would_refuse_is_refused() {
        let refused = |text: &str| bender_dependencies(text).unwrap_err();

        let no_remote = refused("dependencies:\n  axi: 0.39.8\n");
        let neither = refused("dependencies:\n  axi: { git: \"https://x/axi\" }\n");
        let both = refused(
            "dependencies:\n  axi: { git: \"https://x/axi\", version: 1.0.0, rev: main }\n",
        );
        let bad = refused("dependencies:\n  axi: { git: \"https://x/axi\", version: \"one\" }\n");

        assert!(
            no_remote.contains("a bare version needs a default remote"),
            "{no_remote}"
        );
        assert!(neither.contains("needs `version` or `rev`"), "{neither}");
        assert!(both.contains("both `version` and `rev`"), "{both}");
        assert!(bad.contains("`dependencies.axi.version`: `one`"), "{bad}");
    }

    #[test]
    fn a_version_written_as_a_number_is_read_as_written_in_a_table() {
        let table = bender_dependencies(
            "dependencies:\n  axi: { git: \"https://x/axi\", version: 1.10 }\n",
        );
        let bare =
            bender_dependencies("remotes: { pulp: \"https://x\" }\ndependencies:\n  axi: 1.10\n");

        assert_eq!(table.unwrap()["axi"], git("https://x/axi", version("1.10")));
        // A bare `1.10` is a number before it is anything else.
        let bare = bare.unwrap_err();
        assert!(bare.contains("expected a version, or a table"), "{bare}");
    }
}
