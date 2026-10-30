//! `astli init` subcommand.
//!
//! Initializes a new `astli.toml` manifest in the current directory.
//!
//! Sets the package name based on an existing `Bender.yml` or the directory name.
//! Its dependencies are left to `astli bender import`, so that they are
//! copied only on purpose.

use std::io::Write as _;
use std::path::Path;

use astli_package::{BENDER_MANIFEST, MANIFEST, mark};
use toml_edit::{DocumentMut, Item, Table};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::term::{self, status};

/// Initialize a new astli.toml manifest in the current directory
#[derive(Args)]
pub struct Init {}

impl RunWith<Ctx<'_>> for Init {
    type Output = Result;

    fn run_with(self, _: Ctx<'_>) -> Result {
        let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        let path = here.join(MANIFEST);
        if path.exists() {
            return Err(Error::failed(format!("{} already exists", path.display())));
        }
        let bender = read_bender(&here)?;
        let (name, document) = named(&here, bender.as_deref())?;
        create(&path, &document)?;
        status!("Created", "{MANIFEST} for {}", mark::name(&name));
        if bender.is_some() {
            term::note(format!(
                "dependencies are still read from {BENDER_MANIFEST}"
            ))
            .help(format!(
                "{} copies them in",
                mark::code("astli bender import")
            ))
            .report();
        }
        Ok(())
    }
}

/// The text of the `Bender.yml` in `here`, if there is one.
pub(crate) fn read_bender(here: &Path) -> Result<Option<String>> {
    let path = here.join(BENDER_MANIFEST);
    match path.is_file() {
        true => (std::fs::read_to_string(&path).map(Some)).map_err(|err| Error::io(&path, err)),
        false => Ok(None),
    }
}

/// A manifest for the package in `here`, holding `[package] name` alone:
/// the name `bender`, its `Bender.yml`, gives, lowercased, or else the
/// directory's. Returns the name too.
pub(crate) fn named(here: &Path, bender: Option<&str>) -> Result<(String, DocumentMut)> {
    let name = match bender.map(astli_package::bender_name) {
        Some(named) => named.map_err(|err| {
            Error::failed(format!("{}: {err}", here.join(BENDER_MANIFEST).display()))
        })?,
        None => None,
    };
    let name = match name {
        Some(name) => name,
        None => (here.file_name())
            .map(|it| it.to_string_lossy().into_owned())
            .ok_or_else(|| Error::failed("the current directory has no name to give"))?,
    };
    let mut document = DocumentMut::new();
    let mut package = Table::new();
    package.insert("name", toml_edit::value(&name));
    document.insert("package", Item::Table(package));
    Ok((name, document))
}

/// Writes `document` to `path`, which must not exist yet.
pub(crate) fn create(path: &Path, document: &DocumentMut) -> Result {
    let mut file = (std::fs::File::create_new(path)).map_err(|err| Error::io(path, err))?;
    (file.write_all(document.to_string().as_bytes())).map_err(|err| Error::io(path, err))
}
