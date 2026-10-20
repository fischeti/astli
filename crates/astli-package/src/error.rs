//! Why a package could not be found, read or walked.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Why a package could not be found or read.
#[derive(Debug)]
pub enum Error {
    /// A file could not be read, or a path made absolute.
    Io(PathBuf, io::Error),
    /// The manifest at the path is malformed, or names what it may not.
    Manifest(PathBuf, String),
    /// A directory or an ignore file in the package could not be read; the
    /// message names it.
    Walk(String),
    /// `bender` could not be run, or failed; the message says how.
    Bender(String),
    /// A feature was asked of a package that does not declare it, or of a
    /// package not in the design; the message names who asked.
    Selection(String),
    /// An override replaces a name twice, names a package not in the
    /// design, or would drop a file declaring a name it does not replace.
    Override(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(path, err) => write!(f, "{}: {err}", path.display()),
            Error::Manifest(path, what) => write!(f, "{}: {what}", path.display()),
            Error::Walk(what)
            | Error::Bender(what)
            | Error::Selection(what)
            | Error::Override(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(_, err) => Some(err),
            Error::Manifest(..)
            | Error::Walk(_)
            | Error::Bender(_)
            | Error::Selection(_)
            | Error::Override(_) => None,
        }
    }
}
