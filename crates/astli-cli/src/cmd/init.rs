//! `astli init` subcommand.
//!
//! Initializes a new `astli.toml` manifest in the current directory.
//!
//! Sets the package name based on an existing `Bender.yml` or the directory name.
//! When `--bender` is provided, dependencies declared in `Bender.yml` are converted
//! and imported into the `[dependencies]` table of `astli.toml`. With
//! `--check` as well, nothing is written: the two lists are compared, and
//! each dependency they disagree on is an error.

use std::io::Write as _;
use std::path::Path;

use astli_package::{At, BENDER_MANIFEST, MANIFEST, Requirement, Source, mark};
use semver::{Op, VersionReq};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::term::{self, status};

/// Initialize a new astli.toml manifest in the current directory
#[derive(Args)]
pub struct Init {
    /// Import dependencies from Bender.yml into astli.toml
    #[usage(long)]
    pub bender: bool,
    /// Write nothing, and exit with an error if astli.toml and Bender.yml list different dependencies
    #[usage(long, requires("--bender"))]
    pub check: bool,
}

impl RunWith<Ctx<'_>> for Init {
    type Output = Result;

    fn run_with(self, _: Ctx<'_>) -> Result {
        let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        if self.check {
            return check(&here);
        }
        let path = here.join(MANIFEST);
        if path.exists() {
            return Err(Error::failed(format!("{} already exists", path.display())));
        }
        let bender = here.join(BENDER_MANIFEST);
        let text = match bender.is_file() {
            true => Some(std::fs::read_to_string(&bender).map_err(|err| Error::io(&bender, err))?),
            false => None,
        };
        let wrong = |err: String| Error::failed(format!("{}: {err}", bender.display()));

        let mut document = DocumentMut::new();
        let name = match text.as_deref().map(astli_package::bender_name) {
            Some(named) => named.map_err(wrong)?,
            None => None,
        };
        let name = match name {
            Some(name) => name,
            None => (here.file_name())
                .map(|it| it.to_string_lossy().into_owned())
                .ok_or_else(|| Error::failed("the current directory has no name to give"))?,
        };
        let mut package = Table::new();
        package.insert("name", toml_edit::value(&name));
        document.insert("package", Item::Table(package));

        let copied = match (self.bender, &text) {
            (false, _) => None,
            (true, None) => {
                return Err(Error::failed(format!(
                    "no {BENDER_MANIFEST} here to copy dependencies from"
                )));
            }
            (true, Some(text)) => {
                let dependencies = astli_package::bender_dependencies(text).map_err(wrong)?;
                let mut table = Table::new();
                for (name, requirement) in &dependencies {
                    table.insert(name, Item::Value(Value::InlineTable(entry(requirement))));
                }
                document.insert("dependencies", Item::Table(table));
                Some(dependencies.len())
            }
        };

        let mut file = (std::fs::File::create_new(&path)).map_err(|err| Error::io(&path, err))?;
        (file.write_all(document.to_string().as_bytes())).map_err(|err| Error::io(&path, err))?;
        match (copied, &text) {
            (Some(count), _) => {
                let dependencies = match count {
                    1 => "1 dependency".to_string(),
                    count => format!("{count} dependencies"),
                };
                status!(
                    "Created",
                    "{MANIFEST} for {}, with the {dependencies} of {BENDER_MANIFEST}",
                    mark::name(&name)
                );
            }
            (None, Some(_)) => {
                status!("Created", "{MANIFEST} for {}", mark::name(&name));
                term::note(format!(
                    "dependencies are still read from {BENDER_MANIFEST}"
                ))
                .help(format!(
                    "{} copies them in",
                    mark::code("astli init --bender")
                ))
                .report();
            }
            (None, None) => status!("Created", "{MANIFEST} for {}", mark::name(&name)),
        }
        Ok(())
    }
}

/// Compares the dependencies the manifests in `here` list, reporting each
/// that is in one alone, or comes from somewhere else in each. Only where
/// one comes from is compared, since a `Bender.yml` enables no features.
fn check(here: &Path) -> Result {
    let read = |name: &str| -> Result<Option<String>> {
        let path = here.join(name);
        match path.is_file() {
            true => (std::fs::read_to_string(&path).map(Some)).map_err(|err| Error::io(&path, err)),
            false => Ok(None),
        }
    };
    let Some(bender) = read(BENDER_MANIFEST)? else {
        return Err(Error::failed(format!(
            "no {BENDER_MANIFEST} here to compare"
        )));
    };
    let Some(astli) = read(MANIFEST)? else {
        let missing = term::error(format!("no {MANIFEST} here to compare"));
        let help = format!("{} writes one", mark::code("astli init --bender"));
        return Err(missing.help(help).into());
    };
    let listed = |astli: Option<&str>, bender: Option<&str>| {
        astli_package::manifest(astli, bender)
            .map(|it| it.dependencies)
            .map_err(Error::failed)
    };
    // An `astli.toml` without `[dependencies]` reads the `Bender.yml`'s, so
    // the two cannot differ, and `manifest` says so by returning the same.
    let copied = listed(Some(&astli), Some(&bender))?;
    let original = listed(None, Some(&bender))?;

    let mut differ = 0;
    let names = (original.keys()).chain(copied.keys().filter(|it| !original.contains_key(*it)));
    for name in names {
        let problem = match (original.get(name), copied.get(name)) {
            (Some(_), None) => term::error(format!(
                "{} is in {BENDER_MANIFEST} but not in {MANIFEST}",
                mark::name(name)
            )),
            (None, Some(_)) => term::error(format!(
                "{} is in {MANIFEST} but not in {BENDER_MANIFEST}",
                mark::name(name)
            )),
            (Some(original), Some(copied)) if original.source != copied.source => {
                term::error(format!("{} comes from different places", mark::name(name)))
                    .note(format!("{BENDER_MANIFEST} has {}", written(original)))
                    .note(format!("{MANIFEST} has {}", written(copied)))
            }
            _ => continue,
        };
        problem.report();
        differ += 1;
    }
    match differ {
        0 => Ok(()),
        1 => Err(Error::Silent),
        differ => Err(Error::failed(format!(
            "{MANIFEST} and {BENDER_MANIFEST} disagree on {differ} dependencies"
        ))),
    }
}

/// `requirement` as its entry of `[dependencies]` reads, marked.
fn written(requirement: &Requirement) -> String {
    mark::code(entry(requirement).to_string().trim())
}

/// `requirement` as an entry of `[dependencies]`. A `Bender.yml` enables no
/// features, so only where it comes from is written.
fn entry(requirement: &Requirement) -> InlineTable {
    let mut entry = InlineTable::new();
    match &requirement.source {
        Some(Source::Git { url, at }) => {
            entry.insert("git", url.as_str().into());
            match at {
                At::Version(required) => entry.insert("version", version(required).into()),
                At::Rev(rev) => entry.insert("rev", rev.as_str().into()),
            };
        }
        Some(Source::Path(path)) => {
            entry.insert("path", path.to_string_lossy().as_ref().into());
        }
        None => {}
    }
    entry
}

/// `required` as a person writes it: a lone caret requirement without its
/// `^`, which a bare version means already.
fn version(required: &VersionReq) -> String {
    let written = required.to_string();
    match &required.comparators[..] {
        [only] if only.op == Op::Caret => written.trim_start_matches('^').to_string(),
        _ => written,
    }
}
