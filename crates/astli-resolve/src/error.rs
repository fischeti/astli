//! Why a dependency could not be fetched or read.

use std::fmt;
use std::io;
use std::path::PathBuf;

use astli_package::mark;

/// Why a dependency could not be fetched or read.
#[derive(Debug)]
pub enum Error {
    /// A file or directory of the store could not be read or written.
    Io(PathBuf, io::Error),
    /// No directory to keep the store in: none of the variables that name
    /// one is set.
    NoStore,
    /// A repository could not be fetched or read; the message names its URL
    /// and what went wrong.
    Git(String),
    /// A package's manifest on disk could not be read.
    Package(astli_package::Error),
    /// A lockfile could not be read; the message says why.
    Lock(PathBuf, String),
    /// No version of each package meets every requirement, or a requirement
    /// names what does not exist; the message says which and why.
    Resolve(String),
}

impl From<astli_package::Error> for Error {
    fn from(err: astli_package::Error) -> Error {
        Error::Package(err)
    }
}

impl Error {
    /// What went wrong, with its spans [marked](astli_package::mark) for a
    /// driver to style. Displayed, the error is this with each span in
    /// backticks.
    pub fn message(&self) -> String {
        match self {
            Error::Io(path, err) => format!("{}: {err}", path.display()),
            Error::NoStore => format!(
                "no directory for the dependency cache: set {}, {} or {}",
                mark::code("ASTLI_CACHE_DIR"),
                mark::code("XDG_CACHE_HOME"),
                mark::code("HOME")
            ),
            Error::Git(what) | Error::Resolve(what) => what.clone(),
            Error::Package(err) => err.message(),
            Error::Lock(path, what) => format!("{}: {what}", path.display()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&mark::plain(&self.message()))
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(_, err) => Some(err),
            Error::Package(err) => Some(err),
            Error::NoStore | Error::Git(_) | Error::Resolve(_) | Error::Lock(..) => None,
        }
    }
}
