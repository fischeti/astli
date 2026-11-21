//! Input file and build configuration resolution.
//!
//! Reconciles source paths and preprocessor arguments across CLI options and `.f` filelists.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use astli_preproc::Build;

use astli_package::mark;

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::filelist::{self, Base};
use crate::run::Run;
use crate::term;

/// Resolved source files and preprocessor build configuration.
pub struct Resolved {
    /// All resolved source file paths to process.
    pub files: Vec<PathBuf>,
    /// The include directories, in the order named, each once: a directory
    /// searched again finds nothing its first search missed.
    pub incdir: Vec<PathBuf>,
    /// The definitions, as `NAME` or `NAME=VALUE`, kept as written so that a
    /// filelist written from them says what the input said.
    pub define: Vec<String>,
}

/// Resolves input files and build configuration by merging filelists and command-line arguments.
pub fn resolve(run: &Run, sources: &Sources, build: &BuildArgs) -> Result<Resolved> {
    let mut files = Vec::new();
    let mut incdir = Vec::new();
    let mut define = Vec::new();

    let lists = (sources.filelist.iter().map(|path| (path, Base::Cwd)))
        .chain(sources.relative.iter().map(|path| (path, Base::File)));

    for (path, base) in lists {
        let list = filelist::read(path, base)?;
        if list.is_empty() {
            term::warning(format!("{}: names nothing", path.display())).report(run);
        }
        files.extend(list.files);
        incdir.extend(list.incdir);
        define.extend(list.define);
    }

    if let Some(word) = sources.files.iter().find(|path| starts_with_plus(path)) {
        return Err(term::error(format!(
            "{}: not a file, and not an option this command takes",
            word.display()
        ))
        .help(format!(
            "{} lists the options it takes",
            mark::code("--help")
        ))
        .into());
    }
    files.extend(sources.files.iter().cloned());

    // The flags' after the filelists', so that a flag wins.
    incdir.extend(build.incdirs());
    define.extend(build.defines());
    let mut seen = HashSet::new();
    incdir.retain(|dir| seen.insert(dir.clone()));

    if files.is_empty() {
        return Err(Error::failed(
            "no input files: name one, or point -f at a filelist that does",
        ));
    }

    Ok(Resolved {
        files,
        incdir,
        define,
    })
}

/// Splits a definition as `-D` gives it into its name and body. `NAME` alone
/// defines it as `1`, as a C compiler's `-D` does.
pub fn split_define(define: &str) -> (&str, &str) {
    define.split_once('=').unwrap_or((define, "1"))
}

/// `build` with `defines`, each as `-D` gives it: a later one of a name
/// wins.
pub fn defined(build: Build, defines: &[String]) -> Build {
    defines.iter().fold(build, |build, define| {
        let (name, body) = split_define(define);
        build.define(name, body)
    })
}

/// Returns `true` if `path` begins with a `+` prefix indicating an unrecognized plusarg.
fn starts_with_plus(path: &Path) -> bool {
    path.to_str().is_some_and(|path| path.starts_with('+'))
}

impl Resolved {
    /// The include directories and definitions, as the preprocessor takes them.
    pub fn build(&self) -> Build {
        let build = (self.incdir.iter().cloned()).fold(Build::new(), Build::include_dir);
        defined(build, &self.define)
    }

    /// Warns if build configuration (include paths or definitions) is provided for a command that ignores them.
    pub fn warn_unused_build(&self, run: &Run, command: &str) {
        if self.incdir.is_empty() && self.define.is_empty() {
            return;
        }
        term::warning(format!(
            "{command} answers what the bytes are, so the include path and the definitions \
             are unused"
        ))
        .report(run);
    }
}
