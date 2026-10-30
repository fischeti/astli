//! `astli add` subcommand.
//!
//! Requires a dependency in the root's `astli.toml` and resolves again, so
//! that the lock holds it. A repository is named as its own manifest names
//! it and required at its highest tagged version, as a caret requirement;
//! a directory is required by its path relative to the manifest; and a bare
//! name requires a package the design already holds, from where it comes
//! and at the version it is locked at. An entry already there is changed
//! in place, keeping the rest of what it asks.

use std::path::{Component, Path, PathBuf};

use astli_package::{BENDER_MANIFEST, MANIFEST, mark};
use astli_resolve::{Lock, Origin};
use astli_text::clean;
use semver::VersionReq;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, Value};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::lockfile::{self, Write};
use crate::term::{self, status};

/// Add a dependency to astli.toml and lock it
#[derive(Args)]
pub struct Add {
    /// Git URL of the dependency's repository, or the name of a package the design already holds
    #[usage(arg, value_name = "SOURCE")]
    pub source: Option<String>,
    /// Local directory to depend on instead of a repository
    #[usage(long, value_name = "DIR")]
    pub path: Option<PathBuf>,
    /// Version requirement [default: the highest tagged version, as a caret requirement]
    #[usage(long, value_name = "REQ")]
    pub version: Option<String>,
    /// Branch, tag, or commit to track instead of a version
    #[usage(long, value_name = "REV")]
    pub rev: Option<String>,
    /// Name to require the dependency by [default: the name its manifest gives]
    #[usage(long)]
    pub name: Option<String>,
    /// Add to [dev-dependencies] instead of [dependencies]
    #[usage(long)]
    pub dev: bool,
    /// Enable a feature of the dependency (can be repeated)
    #[usage(long, value_name = "FEATURE")]
    pub features: Vec<String>,
    /// Disable the dependency's default features
    #[usage(long)]
    pub no_default_features: bool,
}

/// The keys of an entry that say where it comes from and which commit.
const SOURCE: [&str; 4] = ["git", "path", "version", "rev"];

/// What to require: by which name, and the source keys to write. Keys left
/// out keep what the entry already says.
struct Found {
    name: String,
    keys: Vec<(&'static str, String)>,
}

impl RunWith<Ctx<'_>> for Add {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let root = lockfile::here()?;
        let (manifest, text, mut document) = lockfile::manifest(&root)?;
        let (table, other) = tables(self.dev);
        if !self.dev
            && document.get("dependencies").is_none()
            && root.root.join(BENDER_MANIFEST).is_file()
        {
            return Err(from_bender());
        }
        let at = match (&self.version, &self.rev) {
            (Some(_), Some(_)) => {
                return Err(Error::failed(format!(
                    "{} and {} each say which commit to require: give one",
                    mark::code("--version"),
                    mark::code("--rev")
                )));
            }
            (Some(version), None) => {
                VersionReq::parse(version).map_err(|err| {
                    Error::failed(format!(
                        "{} is not a version requirement: {err}",
                        mark::version(version)
                    ))
                })?;
                Some(("version", version.clone()))
            }
            (None, Some(rev)) => Some(("rev", rev.clone())),
            (None, None) => None,
        };

        let found = match (&self.source, &self.path) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(Error::failed(format!(
                    "give a repository's URL, the name of a package the design holds, or {}",
                    mark::code("--path")
                )));
            }
            (None, Some(dir)) => match at {
                Some((key, _)) => {
                    return Err(Error::failed(format!(
                        "{} is for a repository, and {} names a directory",
                        mark::code(format!("--{key}")),
                        mark::code("--path")
                    )));
                }
                None => directory(&root.root, dir, self.name.clone())?,
            },
            (Some(url), None) if is_url(url) => repository(url, self.name.clone(), at)?,
            (Some(name), None) => {
                if self.name.is_some() {
                    return Err(Error::failed(format!(
                        "{} renames a repository or a directory, and {} is a package's name",
                        mark::code("--name"),
                        mark::name(name)
                    )));
                }
                let written = document.get(table).and_then(|it| it.get(name));
                match written {
                    // Kept as written, unless asked otherwise.
                    Some(_) => Found {
                        name: name.clone(),
                        keys: at.into_iter().collect(),
                    },
                    None => held(name, at, ctx.run.locked)?,
                }
            }
        };
        let name = &found.name;
        if name.is_empty() || name.contains('/') {
            return Err(Error::failed(format!(
                "{} is not a dependency's name, which holds no {}",
                mark::name(name),
                mark::code("/")
            )));
        }
        if document.get(other).and_then(|it| it.get(name)).is_some() {
            let flag = match self.dev {
                true => "",
                false => " --dev",
            };
            return Err(term::error(format!(
                "{} is already in {}",
                mark::name(name),
                mark::code(format!("[{other}]"))
            ))
            .help(format!(
                "{} drops it from there",
                mark::code(format!("astli remove{flag} {name}"))
            ))
            .into());
        }

        let entries = entries(&mut document, table);
        let old = entries.get(name).and_then(inline);
        let entry = entry(
            old.as_ref(),
            &found.keys,
            &self.features,
            self.no_default_features,
        );
        let written = described(&entry);
        let sorted = (entries.iter().map(|(key, _)| key)).is_sorted();
        let new = entries.get(name).is_none();
        entries.insert(name, Item::Value(Value::InlineTable(entry)));
        if new && sorted {
            entries.sort_values();
        }
        lockfile::rewrite(&manifest, &text, &document, ctx.run.locked)?;
        status!("Required", "{name}{written} in [{table}]");
        Ok(())
    }
}

/// The table `--dev` names, and the other one.
pub(crate) fn tables(dev: bool) -> (&'static str, &'static str) {
    match dev {
        true => ("dev-dependencies", "dependencies"),
        false => ("dependencies", "dev-dependencies"),
    }
}

/// Why the root's dependencies cannot be changed here: without a
/// `[dependencies]` table, its `Bender.yml` lists them, and a table would
/// replace that list.
pub(crate) fn from_bender() -> Error {
    term::error(format!(
        "the root's dependencies come from its {BENDER_MANIFEST}, since its {MANIFEST} has no {} \
         table, and the {BENDER_MANIFEST} is the bender build's to edit",
        mark::code("[dependencies]")
    ))
    .help(format!(
        "a {} table replaces the {BENDER_MANIFEST}'s list, and {} copies that list in",
        mark::code("[dependencies]"),
        mark::code("astli bender import")
    ))
    .into()
}

/// Whether `source` is where a repository is rather than a package's name,
/// which holds no separator.
fn is_url(source: &str) -> bool {
    source.contains(['/', ':', '\\'])
}

/// The dependency at the repository `url`: named `name`, or as the
/// manifest at the commit it would require names it, and required at `at`,
/// or else at the highest version it tags.
fn repository(
    url: &str,
    name: Option<String>,
    at: Option<(&'static str, String)>,
) -> Result<Found> {
    let basename = basename(url);
    let store = lockfile::store()?;
    let mut mirror = store.mirror(name.as_deref().unwrap_or(&basename), url)?;
    if !mirror.fresh {
        mirror.fetch()?;
    }
    let versions = mirror.versions()?;
    let (at, commit) = match at {
        Some(("version", written)) => {
            let required = VersionReq::parse(&written).expect("checked");
            let admitted = (versions.iter().rev()).find(|(version, _)| required.matches(version));
            (("version", written), admitted.map(|it| it.1))
        }
        Some((key, rev)) => {
            let commit = mirror.rev(&rev);
            ((key, rev), commit)
        }
        None => {
            let released = (versions.iter().rev()).find(|(version, _)| version.pre.is_empty());
            let Some((version, commit)) = released else {
                return Err(term::error(format!(
                    "{} tags no version {}, so there is none to require",
                    mark::path(url),
                    mark::code("vX.Y.Z")
                ))
                .help(format!(
                    "{} tracks a branch, a tag or a commit instead",
                    mark::code("--rev")
                ))
                .into());
            };
            (("version", version.to_string()), Some(*commit))
        }
    };
    let name = match name {
        Some(name) => name,
        // A rev the mirror lacks is resolution's to report; the name is
        // read where the repository is now.
        None => match commit.or_else(|| mirror.rev("HEAD")) {
            Some(commit) => {
                let read = |path| -> Result<Option<String>> {
                    let text = mirror.file(commit, path)?;
                    Ok(text.map(|it| String::from_utf8_lossy(&it).into_owned()))
                };
                let (astli, bender) = (read(MANIFEST)?, read(BENDER_MANIFEST)?);
                self::name(astli.as_deref(), bender.as_deref()).unwrap_or(basename)
            }
            None => basename,
        },
    };
    Ok(Found {
        name,
        keys: vec![("git", url.to_string()), at],
    })
}

/// The name an `astli.toml`, or else a `Bender.yml`, gives its package. A
/// manifest that does not read is resolution's to report.
fn name(astli: Option<&str>, bender: Option<&str>) -> Option<String> {
    let astli = astli.and_then(|text| text.parse::<toml::Table>().ok());
    let named = (astli.as_ref())
        .and_then(|it| it.get("package")?.get("name")?.as_str())
        .map(str::to_string);
    named.or_else(|| bender.and_then(|text| astli_package::bender_name(text).ok().flatten()))
}

/// The last part of a URL, without `.git`, lowercased as a `Bender.yml`'s
/// names are.
fn basename(url: &str) -> String {
    let last = (url.trim_end_matches(['/', '\\']).rsplit(['/', ':', '\\']))
        .next()
        .unwrap_or(url);
    last.strip_suffix(".git").unwrap_or(last).to_lowercase()
}

/// The dependency in the directory `dir`, named `name` or as its manifest
/// names it, or else after the directory; required by its path relative to
/// the root, where the manifest is.
fn directory(root: &Path, dir: &Path, name: Option<String>) -> Result<Found> {
    let dir = clean(&std::path::absolute(dir).map_err(|err| Error::io(dir, err))?);
    if !dir.is_dir() {
        return Err(Error::failed(format!(
            "{} is not a directory",
            mark::path(dir.display())
        )));
    }
    let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok();
    let name = name
        .or_else(|| self::name(read(MANIFEST).as_deref(), read(BENDER_MANIFEST).as_deref()))
        .or_else(|| dir.file_name().map(|it| it.to_string_lossy().into_owned()));
    let Some(name) = name else {
        return Err(Error::failed(format!(
            "{} has no name to require it by: give one with {}",
            mark::path(dir.display()),
            mark::code("--name")
        )));
    };
    Ok(Found {
        name,
        keys: vec![("path", written(&relative(root, &dir)))],
    })
}

/// The package `name` the design already holds, from where it comes, at
/// the version it is locked at, or by what names its commit if it has no
/// version, unless `at` says otherwise. Resolved as if it were not patched.
fn held(name: &str, at: Option<(&'static str, String)>, locked: bool) -> Result<Found> {
    let mut root = lockfile::here()?;
    root.patches.0.remove(name);
    let resolution = lockfile::settle(&root, Lock::clone, Write::Manifest, locked)?;
    let Some(package) = resolution.packages.iter().find(|it| it.name == name) else {
        return Err(term::error(format!(
            "{} is not in the design, so there is nowhere to require it from",
            mark::name(name)
        ))
        .help(format!(
            "give its repository's URL, or {} its directory",
            mark::code("--path")
        ))
        .into());
    };
    let keys = match &package.origin {
        Origin::Git { url } => {
            let at = at.unwrap_or_else(|| match (&package.version, package.revs.first()) {
                (Some(version), _) => ("version", version.to_string()),
                (None, Some(rev)) => ("rev", rev.clone()),
                (None, None) => ("rev", package.commit.expect("a commit").to_string()),
            });
            vec![("git", url.clone()), at]
        }
        Origin::Dir(dir) => match at {
            Some((key, _)) => {
                return Err(Error::failed(format!(
                    "{} is for a repository, and {} comes from {}",
                    mark::code(format!("--{key}")),
                    mark::name(name),
                    mark::path(dir.display())
                )));
            }
            None => vec![("path", written(&relative(&root.root, dir)))],
        },
        Origin::Within { url, subdir } => {
            return Err(Error::failed(format!(
                "{} is the directory {} of {}, which only that repository's packages can require",
                mark::name(name),
                mark::path(subdir.display()),
                mark::path(url)
            )));
        }
        Origin::Patched(_) => unreachable!("resolved without its patch"),
    };
    Ok(Found {
        name: name.to_string(),
        keys,
    })
}

/// `to` relative to `from`, both absolute and clean. On Windows, a path on
/// another drive stays absolute.
pub(crate) fn relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    let common = (from.iter().zip(&to)).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return to.iter().collect();
    }
    let up = (common..from.len()).map(|_| Component::ParentDir);
    let path: PathBuf = up.chain(to[common..].iter().copied()).collect();
    match path.as_os_str().is_empty() {
        true => PathBuf::from("."),
        false => path,
    }
}

/// A path as a manifest writes it, with `/` on every platform.
pub(crate) fn written(path: &Path) -> String {
    let parts = path.components().map(|it| it.as_os_str().to_string_lossy());
    parts.collect::<Vec<_>>().join("/")
}

/// The root's table `table`, made if it has none.
pub(crate) fn entries<'a>(document: &'a mut DocumentMut, table: &str) -> &'a mut dyn TableLike {
    (document.entry(table).or_insert(Item::Table(Table::new())))
        .as_table_like_mut()
        .expect("a dependency table")
}

/// An entry as an inline table, whichever way it is written.
fn inline(item: &Item) -> Option<InlineTable> {
    match item {
        Item::Value(Value::InlineTable(entry)) => Some(entry.clone()),
        Item::Table(entry) => Some(entry.clone().into_inline_table()),
        _ => None,
    }
}

/// The entry `old` with the source keys `keys` written over it, its
/// features with `features` added, and its default features off if
/// `no_default_features`. A new repository or directory replaces every
/// source key; a version or a rev alone replaces only which commit.
fn entry(
    old: Option<&InlineTable>,
    keys: &[(&'static str, String)],
    features: &[String],
    no_default_features: bool,
) -> InlineTable {
    let given = |key: &str| keys.iter().any(|(it, _)| *it == key);
    let moved = given("git") || given("path");
    let at = given("version") || given("rev");
    let mut entry = InlineTable::new();
    for key in SOURCE {
        if let Some((_, value)) = keys.iter().find(|(it, _)| *it == key) {
            entry.insert(key, value.as_str().into());
            continue;
        }
        let replaced = moved || (at && (key == "version" || key == "rev"));
        if let Some(value) = old.and_then(|it| it.get(key))
            && !replaced
        {
            entry.insert(key, value.clone());
        }
    }
    for (key, value) in old.into_iter().flat_map(InlineTable::iter) {
        if !SOURCE.contains(&key) {
            entry.insert(key, value.clone());
        }
    }
    if !features.is_empty() {
        let mut enabled = (entry.get("features").and_then(Value::as_array))
            .cloned()
            .unwrap_or_else(Array::new);
        for feature in features {
            if !enabled.iter().any(|it| it.as_str() == Some(feature)) {
                enabled.push(feature.as_str());
            }
        }
        entry.insert("features", Value::Array(enabled));
    }
    if no_default_features {
        entry.insert("default-features", false.into());
    }
    entry.fmt();
    entry
}

/// What an entry requires, for the status line: its version, its rev or
/// its path.
fn described(entry: &InlineTable) -> String {
    let value = |key| entry.get(key).and_then(Value::as_str);
    match (value("version"), value("rev"), value("path")) {
        (Some(version), _, _) => format!(" {version}"),
        (_, Some(rev), _) => format!(" at {rev}"),
        (_, _, Some(path)) => format!(" from {path}"),
        _ => String::new(),
    }
}
