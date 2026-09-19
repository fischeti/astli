//! Input file and build configuration resolution.
//!
//! Reconciles source paths and preprocessor arguments across CLI options and `.f` filelists.

use std::path::{Path, PathBuf};

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::filelist::{self, Base, plus};

/// Reconciled build configuration containing search directories and macro definitions.
#[derive(Default)]
pub struct Build {
    /// Include search directories in lookup order.
    pub incdir: Vec<PathBuf>,
    /// Macro definitions in order of definition (later definitions override earlier ones).
    pub define: Vec<String>,
}

impl Build {
    /// Returns `true` if no include directories or macro definitions are configured.
    pub fn is_empty(&self) -> bool {
        self.incdir.is_empty() && self.define.is_empty()
    }
}

/// Resolved source files and preprocessor build configuration.
pub struct Resolved {
    /// All resolved source file paths to process.
    pub files: Vec<PathBuf>,
    /// Combined build configuration.
    pub build: Build,
}

/// Resolves input files and build configuration by merging filelists and command-line arguments.
pub fn resolve(sources: &Sources, build: &BuildArgs) -> Result<Resolved> {
    let mut files = Vec::new();
    let mut found = Build::default();

    let lists = (sources.filelist.iter().map(|path| (path, Base::Cwd)))
        .chain(sources.relative.iter().map(|path| (path, Base::File)));

    for (path, base) in lists {
        let list = filelist::read(path, base)?;
        if list.is_empty() {
            eprintln!("svirig: {}: names nothing", path.display());
        }
        files.extend(list.files);
        found.incdir.extend(list.incdir);
        found.define.extend(list.define);
    }

    if let Some(word) = sources.files.iter().find(|path| starts_with_plus(path)) {
        return Err(Error::failed(format!(
            "{}: not a file, and not an option this command takes; see --help",
            word.display()
        )));
    }
    files.extend(sources.files.iter().cloned());

    let dirs = build.incdir_plus.iter().flat_map(|arg| plus(arg));
    found.incdir.extend(dirs.map(PathBuf::from));
    found.define.extend(
        build
            .define_plus
            .iter()
            .flat_map(|arg| plus(arg))
            .map(String::from),
    );

    found.incdir.extend(build.incdir.iter().cloned());
    found.define.extend(build.define.iter().cloned());

    if files.is_empty() {
        return Err(Error::failed(
            "no input files: name one, or point -f at a filelist that does",
        ));
    }

    Ok(Resolved {
        files,
        build: found,
    })
}

/// Returns `true` if `path` begins with a `+` prefix indicating an unrecognized plusarg.
fn starts_with_plus(path: &Path) -> bool {
    path.to_str().is_some_and(|path| path.starts_with('+'))
}

impl Resolved {
    /// Warns if build configuration (include paths or definitions) is provided for a command that ignores them.
    pub fn warn_unused_build(&self, command: &str) {
        if self.build.is_empty() {
            return;
        }
        eprintln!(
            "svirig: {command} answers what the bytes are, so the include path and \
             the definitions are unused"
        );
    }
}
