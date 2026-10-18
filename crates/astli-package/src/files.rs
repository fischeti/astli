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
    /// For a tool that has only a search path: the directory each include
    /// found by path suffix is found in, sorted.
    pub suffix_dirs: Vec<PathBuf>,
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
        let searched: Vec<PathBuf> = (self.include_dirs.iter().chain(dependencies))
            .cloned()
            .collect();
        let build = (searched.iter().cloned())
            .fold(Build::new(), Build::include_dir)
            .include_files(walked.iter().cloned());
        let included = |path: &PathBuf| included(path, &build, &searched);

        // Every walked file, in parallel: nearly all the work.
        let scanned: Vec<Vec<Include>> =
            walked.par_iter().map(included).collect::<Result<_, _>>()?;
        let mut found: Vec<Include> = scanned.into_iter().flatten().collect();

        // A header outside the walk names headers of its own. There are few,
        // so one at a time. The walked files are sorted. A dependency's
        // headers are its own to tell apart.
        let own = |path: &PathBuf| path.starts_with(&self.root);
        let unwalked = |path: &PathBuf| walked.binary_search(path).is_err();
        let mut headers = BTreeSet::new();
        let mut at = 0;
        while let Some(include) = found.get(at) {
            at += 1;
            let header = include.header.clone();
            if own(&header) && headers.insert(header.clone()) && unwalked(&header) {
                found.extend(included(&header)?);
            }
        }

        let units = (walked.iter())
            .filter(|path| is_unit(path) && !headers.contains(*path))
            .cloned()
            .collect();
        let suffix_dirs: BTreeSet<PathBuf> = (found.into_iter())
            .filter(|include| own(&include.header))
            .filter_map(|include| include.by_suffix)
            .collect();
        Ok(Files {
            walked,
            units,
            headers: headers.into_iter().collect(),
            build,
            suffix_dirs: suffix_dirs.into_iter().collect(),
        })
    }
}

/// A file an `` `include `` names.
struct Include {
    header: PathBuf,
    /// The directory a search path finds it through, when only the suffix
    /// search found it: its path less the name written.
    by_suffix: Option<PathBuf>,
}

/// Every file an `` `include `` in `path` could name, in any branch, given
/// the directories `searched` before the suffix search.
fn included(path: &Path, build: &Build, searched: &[PathBuf]) -> Result<Vec<Include>, Error> {
    let text = std::fs::read_to_string(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
    let mut session = Session::new();
    let file = session.add(path, text);
    let input = session.input(file);

    let beside = path.parent();
    let mut found = Vec::new();
    for directive in astli_preproc::scan(&input).directives() {
        let Operands::Include(named) = &directive.operands else {
            continue;
        };
        let Some((name, angle)) = named.written(&input) else {
            continue;
        };
        let headers = match build.resolve(name, Some(path), angle, Path::is_file) {
            Resolved::Found(header) => vec![header],
            Resolved::Ambiguous(headers) => headers,
            Resolved::NotFound => Vec::new(),
        };
        for header in headers {
            let dir = (0..Path::new(name).components().count())
                .try_fold(header.as_path(), |dir, _| dir.parent());
            let by_suffix = dir
                .filter(|dir| Some(*dir) != beside && !searched.iter().any(|it| it == dir))
                .map(Path::to_path_buf);
            found.push(Include { header, by_suffix });
        }
    }
    Ok(found)
}

fn is_unit(path: &Path) -> bool {
    (path.extension())
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| UNIT_EXTENSIONS.contains(&extension))
}
