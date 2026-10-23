//! Why a dependency could not be fetched or read.

use std::fmt;
use std::io;
use std::path::PathBuf;

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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(path, err) => write!(f, "{}: {err}", path.display()),
            Error::NoStore => write!(
                f,
                "no directory for the dependency cache: set `ASTLI_CACHE_DIR`, \
                 `XDG_CACHE_HOME` or `HOME`"
            ),
            Error::Git(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(_, err) => Some(err),
            Error::NoStore | Error::Git(_) => None,
        }
    }
}
