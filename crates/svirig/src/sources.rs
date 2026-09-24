//! Input file and build configuration resolution.
//!
//! Reconciles source paths and preprocessor arguments across CLI options and `.f` filelists.

use std::path::{Path, PathBuf};

use svirig_preproc::Build;

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::filelist::{self, Base, plus};

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
    let mut incdir = Vec::new();
    let mut define = Vec::new();

    let lists = (sources.filelist.iter().map(|path| (path, Base::Cwd)))
        .chain(sources.relative.iter().map(|path| (path, Base::File)));

    for (path, base) in lists {
        let list = filelist::read(path, base)?;
        if list.is_empty() {
            eprintln!("svirig: {}: names nothing", path.display());
        }
        files.extend(list.files);
        incdir.extend(list.incdir);
        define.extend(list.define);
    }

    if let Some(word) = sources.files.iter().find(|path| starts_with_plus(path)) {
        return Err(Error::failed(format!(
            "{}: not a file, and not an option this command takes; see --help",
            word.display()
        )));
    }
    files.extend(sources.files.iter().cloned());

    let dirs = build.incdir_plus.iter().flat_map(|arg| plus(arg));
    incdir.extend(dirs.map(PathBuf::from));
    define.extend(
        build
            .define_plus
            .iter()
            .flat_map(|arg| plus(arg))
            .map(String::from),
    );

    incdir.extend(build.incdir.iter().cloned());
    define.extend(build.define.iter().cloned());

    if files.is_empty() {
        return Err(Error::failed(
            "no input files: name one, or point -f at a filelist that does",
        ));
    }

    let found = incdir.into_iter().fold(Build::new(), Build::include_dir);
    let found = define.iter().fold(found, |found, define| {
        // `NAME` alone defines it as `1`, as a C compiler's `-D` does.
        let (name, body) = define.split_once('=').unwrap_or((define, "1"));
        found.define(name, body)
    });
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
