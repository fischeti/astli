//! What the command line says to read, once the filelists have been read.
//!
//! Every command past `completion` starts here, because a filelist carries
//! both halves of the question -- which files, and what build they are part of
//! -- and neither command should learn to unpack that twice.

use std::path::PathBuf;

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::filelist::{self, Base};

/// The files to read, and what to read them as.
pub struct Resolved {
    pub files: Vec<PathBuf>,
    pub build: BuildArgs,
}

/// Reads every filelist and puts what it found together with the flags.
///
/// The filelists come first and the flags last, so that `-D` on the command
/// line overrides the same name in a filelist: a later definition wins, and
/// the command line is the override. Include directories keep the order they
/// were written in, which is the order they are searched.
pub fn resolve(sources: &Sources, build: &BuildArgs) -> Result<Resolved> {
    let mut files = Vec::new();
    let mut found = BuildArgs::default();

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

    files.extend(sources.files.iter().cloned());
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

impl Resolved {
    /// Says that what a filelist carried is going nowhere, for the commands
    /// that cannot use it.
    ///
    /// Raw mode is handed no macro table, so `lex` and `parse` read a file as
    /// written whatever the build says. Printing it is the difference between
    /// a flag that is ignored and a flag that is ignored quietly.
    pub fn warn_unused_build(&self, command: &str) {
        if self.build.incdir.is_empty() && self.build.define.is_empty() {
            return;
        }
        eprintln!(
            "svirig: {command} reads each file as written, so the include path and \
             the definitions are unused"
        );
    }
}
