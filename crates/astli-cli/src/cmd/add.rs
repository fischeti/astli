//! `astli add` subcommand.
//!
//! Requires a dependency in the root's `astli.toml` and resolves again, so
//! that the lock holds it. A repository is named as its own manifest names
//! it and required at its highest tagged version, as a caret requirement;
//! a directory is required by its path relative to the manifest; and a bare
//! name requires a package the design already holds, from where it comes
//! and at the version it is locked at. An entry already there is changed
//! in place, keeping the rest of what it asks.

use std::path::{Path, PathBuf};

use astli_package::{BENDER_MANIFEST, MANIFEST, key, mark};
use astli_resolve::{Lock, Origin};
use astli_text::clean;
use semver::VersionReq;
use toml_edit::{InlineTable, Value};
use usage_rs::{Args, RunWith};

use crate::cli::PackageArg;
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::manifest::{self, Editable, relative, tables, written};
use crate::run::Run;
use crate::settle::{self, Tell, Write};
use crate::target::Here;
use crate::term::{self, status};

/// Add a dependency to astli.toml and update astli.lock
#[derive(Args)]
pub struct Add {
    /// Git repository URL, or name of an existing package in the design
    #[usage(arg, value_name = "SOURCE")]
    pub source: Option<String>,
    /// Local directory path of the dependency
    #[usage(long, value_name = "DIR")]
    pub path: Option<PathBuf>,
    /// Version requirement [default: highest compatible semver tag]
    #[usage(long, value_name = "REQ")]
    pub version: Option<String>,
    /// Git branch, tag, or commit to track instead of a version
    #[usage(long, value_name = "REV")]
    pub rev: Option<String>,
    /// Package name override [default: name from manifest]
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
    #[usage(flatten)]
    pub package: PackageArg,
}

/// What to require: by which name, and the source keys to write. Keys left
/// out keep what the entry already says.
struct Found {
    name: String,
    keys: Vec<(&'static str, String)>,
}

impl RunWith<Ctx<'_>> for Add {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let run = ctx.run;
        let target = cmd::choose(self.package.package.as_deref(), false)?;
        let here = target.here(run)?;
        let root = &here.package.package;
        let mut manifest = Editable::open(root)?;
        let (table, other) = tables(self.dev);
        if !self.dev
            && let Some(problem) = manifest::from_bender(&here, root)
        {
            return Err(problem.into());
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
                Some((key::VERSION, version.clone()))
            }
            (None, Some(rev)) => Some((key::REV, rev.clone())),
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
            (Some(url), None) if is_url(url) => repository(run, url, self.name.clone(), at)?,
            (Some(name), None) => {
                if self.name.is_some() {
                    return Err(Error::failed(format!(
                        "{} renames a repository or a directory, and {} is a package's name",
                        mark::code("--name"),
                        mark::name(name)
                    )));
                }
                match manifest.holds(table, name) {
                    // Kept as written, unless asked otherwise.
                    true => Found {
                        name: name.clone(),
                        keys: at.into_iter().collect(),
                    },
                    false => held(run, &here, name, at)?,
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
        if manifest.holds(other, name) {
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

        let entry = manifest.require(
            table,
            name,
            &found.keys,
            &self.features,
            self.no_default_features,
        );
        let described = described(&entry);
        manifest::commit(run, &here, &[manifest], None, Tell::Design, run.args.locked)?;
        status!("Required", "{name}{described} in [{table}]");
        Ok(())
    }
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
    run: &Run,
    url: &str,
    name: Option<String>,
    at: Option<(&'static str, String)>,
) -> Result<Found> {
    let basename = basename(url);
    let store = run.store()?;
    let mirror = store.fetched(name.as_deref().unwrap_or(&basename), url)?;
    let versions = mirror.versions()?;
    let (at, commit) = match at {
        Some((key::VERSION, written)) => {
            let required = VersionReq::parse(&written).expect("checked");
            let admitted = (versions.iter().rev()).find(|(version, _)| required.matches(version));
            ((key::VERSION, written), admitted.map(|it| it.1))
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
            ((key::VERSION, version.to_string()), Some(*commit))
        }
    };
    let name = match name {
        Some(name) => name,
        // A rev the mirror lacks is resolution's to report; the name is
        // read where the repository is now.
        None => match commit.or_else(|| mirror.rev("HEAD")) {
            Some(commit) => {
                let (astli, bender) = mirror.manifests(commit, Path::new(""))?;
                self::name(astli.as_deref(), bender.as_deref()).unwrap_or(basename)
            }
            None => basename,
        },
    };
    Ok(Found {
        name,
        keys: vec![(key::GIT, url.to_string()), at],
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
        keys: vec![(key::PATH, written(&relative(root, &dir)))],
    })
}

/// The package `name` the design of `here` already holds, from where it
/// comes, at the version it is locked at, or by what names its commit if it
/// has no version, unless `at` says otherwise. Resolved as if it were not
/// patched.
fn held(run: &Run, here: &Here, name: &str, at: Option<(&'static str, String)>) -> Result<Found> {
    let mut overrides = here.overrides();
    overrides.patches.0.remove(name);
    let resolution = settle::settle(
        run,
        here,
        &overrides,
        Lock::clone,
        Write::Manifest,
        settle::Tell::Design,
        run.args.locked,
    )?;
    let root = &here.package.package;
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
        Origin::Git { url, commit } => {
            let at = at.unwrap_or_else(|| match (&package.version, package.revs.first()) {
                (Some(version), _) => (key::VERSION, version.to_string()),
                (None, Some(rev)) => (key::REV, rev.clone()),
                (None, None) => (key::REV, commit.to_string()),
            });
            vec![(key::GIT, url.clone()), at]
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
            None => vec![(key::PATH, written(&relative(&root.root, dir)))],
        },
        Origin::Within { url, subdir, .. } => {
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

/// What an entry requires, for the status line: its version, its rev or
/// its path.
fn described(entry: &InlineTable) -> String {
    let value = |key| entry.get(key).and_then(Value::as_str);
    match (value(key::VERSION), value(key::REV), value(key::PATH)) {
        (Some(version), _, _) => format!(" {version}"),
        (_, Some(rev), _) => format!(" at {rev}"),
        (_, _, Some(path)) => format!(" from {path}"),
        _ => String::new(),
    }
}
