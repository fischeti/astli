//! What the command line says to read, once the filelists have been read.
//!
//! Every command past `completion` starts here, because a filelist carries
//! both halves of the question -- which files, and what build they are part of
//! -- and neither command should learn to unpack that twice.

use std::path::{Path, PathBuf};

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::filelist::{self, Base, plus};

/// What a build passes, with every way of asking for it already reconciled.
///
/// Not [`BuildArgs`]: that is the question, in the four spellings a caller may
/// write it, and this is the answer. Keeping them apart is what makes it
/// impossible to hand a session a plus-separated `+incdir+a+b` that nothing
/// ever unpacked.
#[derive(Default)]
pub struct Build {
    /// In search order.
    pub incdir: Vec<PathBuf>,
    /// In definition order, so a repeated name's last entry is the one that
    /// stands.
    pub define: Vec<String>,
}

impl Build {
    pub fn is_empty(&self) -> bool {
        self.incdir.is_empty() && self.define.is_empty()
    }
}

/// The files to read, and what to read them as.
pub struct Resolved {
    pub files: Vec<PathBuf>,
    pub build: Build,
}

/// Reads every filelist and puts what it found together with the flags.
///
/// Three layers, each the last word over the one before it: what a filelist
/// carried, then the plus-separated flags, then `-I` and `-D`. So `-D` on the
/// command line overrides the same name in a filelist -- a later definition
/// wins, and the command line is the override -- and `-D` overrides
/// `+define+` written beside it.
///
/// That last part is a rule and not an observation: the two spellings are one
/// layer as far as anything downstream is concerned, and the argument parser
/// reports each flag's values without saying where in argv they fell, so
/// `-D A=1 +define+A=2` cannot be told from `+define+A=2 -D A=1`. Rather than
/// pick per invocation and be wrong half the time, the dash form is always
/// the later one. Include directories keep the order they were written in,
/// which is the order they are searched.
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

    // A word the parser could not classify arrives here as a file, and every
    // plus-separated option it does not know is such a word. Rejecting it by
    // name is the rule a filelist already follows, and for the same reason
    // `unknown_flags = "error"` is set on the dash side: `+libext+.sv`
    // reported as a missing file is a mistyped option that looks like a
    // missing file.
    if let Some(word) = sources.files.iter().find(|path| starts_with_plus(path)) {
        return Err(Error::failed(format!(
            "{}: not a file, and not an option this command takes; see --help",
            word.display()
        )));
    }
    files.extend(sources.files.iter().cloned());

    // A relative path here is relative to the working directory, which is
    // what a bare path on a command line means everywhere else. Only a
    // filelist has a second answer to that question, and it has already
    // applied its own.
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

/// Whether a path is a plusarg rather than a file. A file really named that
/// way is still reachable as `./+name`, which is what the same ambiguity on
/// the dash side has always cost.
fn starts_with_plus(path: &Path) -> bool {
    path.to_str().is_some_and(|path| path.starts_with('+'))
}

impl Resolved {
    /// Says that what a filelist carried is going nowhere.
    ///
    /// `lex` only, now that raw mode can be seeded: lexing answers what the
    /// bytes are, and no definition anywhere changes that. Printing it is the
    /// difference between a flag that is ignored and a flag that is ignored
    /// quietly.
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
