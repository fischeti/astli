//! A package's files told apart: the units compiled on their own, and the
//! headers others include.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use astli_preproc::{Build, Operands, Resolved, Session};
use rayon::prelude::*;

use crate::{Error, Package};

/// The extensions of a file compiled on its own, unless another includes it.
const UNIT_EXTENSIONS: [&str; 2] = ["sv", "v"];

/// A package's files, and the build that preprocesses them.
#[derive(Debug, Clone)]
pub struct Files {
    /// What the walk found, sorted.
    pub walked: Vec<PathBuf>,
    /// The walked files with a unit extension that nothing includes, sorted.
    pub units: Vec<PathBuf>,
    /// Every file of the package an `` `include `` names, in any branch,
    /// sorted. One the walk did not find, such as a header in an include
    /// directory outside `sources`, is here too; a dependency's is not.
    pub headers: Vec<PathBuf>,
    /// The package's include directories, then those its dependencies
    /// export, then its walked files by path suffix.
    pub build: Build,
}

impl Package {
    /// Walks the package and tells its headers from its units. `dependencies`
    /// are the include directories its dependencies export, searched after
    /// its own.
    ///
    /// A header is any file another includes, whatever its extension, since
    /// designs include `.sv` files too. Every branch counts, as one a build
    /// does not take is still not meant to be compiled alone. An include
    /// that resolves to nothing, or to more than one file, is left to the
    /// expansion to report where it matters; each file it could be is a
    /// header.
    pub fn files(&self, dependencies: &[PathBuf]) -> Result<Files, Error> {
        let walked = self.walk()?;
        let build = (self.include_dirs.iter().chain(dependencies))
            .cloned()
            .fold(Build::new(), Build::include_dir)
            .include_files(walked.iter().cloned());

        // Every walked file, in parallel: nearly all the work.
        let scanned: Vec<Vec<PathBuf>> = (walked.par_iter())
            .map(|path| included(path, &build))
            .collect::<Result<_, _>>()?;
        // A dependency's headers are its own to tell apart.
        let own = |path: &PathBuf| path.starts_with(&self.root);
        let mut headers: BTreeSet<PathBuf> = (scanned.into_iter().flatten()).filter(own).collect();

        // A header outside the walk names headers of its own. There are few,
        // so one at a time. The walked files are sorted.
        let unwalked = |path: &PathBuf| walked.binary_search(path).is_err();
        let mut queue: Vec<PathBuf> = headers.iter().filter(|h| unwalked(h)).cloned().collect();
        while let Some(header) = queue.pop() {
            for found in included(&header, &build)?.into_iter().filter(own) {
                // A header either way, and scanned here unless the walk found it.
                if headers.insert(found.clone()) && unwalked(&found) {
                    queue.push(found);
                }
            }
        }

        let units = (walked.iter())
            .filter(|path| is_unit(path) && !headers.contains(*path))
            .cloned()
            .collect();
        Ok(Files {
            walked,
            units,
            headers: headers.into_iter().collect(),
            build,
        })
    }
}

/// Every file an `` `include `` in `path` could name, in any branch.
fn included(path: &Path, build: &Build) -> Result<Vec<PathBuf>, Error> {
    let text = std::fs::read_to_string(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
    let mut session = Session::new();
    let file = session.add(path, text);
    let input = session.input(file);

    let mut found = Vec::new();
    for directive in astli_preproc::scan(&input).directives() {
        let Operands::Include(named) = &directive.operands else {
            continue;
        };
        let Some((name, angle)) = named.written(&input) else {
            continue;
        };
        match build.resolve(name, Some(path), angle, Path::is_file) {
            Resolved::Found(header) => found.push(header),
            Resolved::Ambiguous(headers) => found.extend(headers),
            Resolved::NotFound => {}
        }
    }
    Ok(found)
}

fn is_unit(path: &Path) -> bool {
    (path.extension())
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| UNIT_EXTENSIONS.contains(&extension))
}
