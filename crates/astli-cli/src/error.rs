//! Driver error types and exit code representations.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::term::{self, Place, Problem};

/// Error conditions encountered during CLI command execution.
#[derive(Debug)]
pub enum Error {
    /// An I/O error occurred while reading or writing a file.
    Io(PathBuf, std::io::Error),
    /// Command execution failed, for the reason the problem gives. Boxed,
    /// so that a `Result` that holds no error stays small.
    Failed(Box<Problem>),
    /// A syntax or resolution error occurred while parsing a `.f` filelist.
    Filelist(PathBuf, usize, String),
    /// Failure has already been reported via diagnostics; exit without further output.
    Silent,
    /// An error occurred while writing output (e.g. broken pipe when piping to `head`).
    Output(std::io::Error),
}

impl Error {
    /// Creates a file I/O error for `path`.
    pub fn io(path: impl AsRef<Path>, err: std::io::Error) -> Error {
        Error::Io(path.as_ref().to_path_buf(), err)
    }

    /// Creates a generic command failure error with `what`.
    pub fn failed(what: impl Into<String>) -> Error {
        Error::Failed(Box::new(term::error(what)))
    }

    /// The problem to report the error as, ending the run or a file's part
    /// in it.
    pub fn problem(self) -> Problem {
        match self {
            Error::Failed(problem) => *problem,
            Error::Filelist(path, line @ 1.., what) => {
                term::error(what).at("filelist", &path, Place::Line(line))
            }
            err => term::error(err.to_string()),
        }
    }

    /// Creates a filelist syntax or resolution error at `line` in `path`.
    pub fn filelist(path: impl AsRef<Path>, line: usize, what: impl Into<String>) -> Error {
        Error::Filelist(path.as_ref().to_path_buf(), line, what.into())
    }
}

impl From<Problem> for Error {
    fn from(problem: Problem) -> Error {
        Error::Failed(Box::new(problem))
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Output(err)
    }
}

impl From<astli_package::Error> for Error {
    fn from(err: astli_package::Error) -> Error {
        match err {
            astli_package::Error::Io(path, err) => Error::Io(path, err),
            astli_package::Error::Unparsed(path, what, Some(bytes)) => term::error(what)
                .at("manifest", &path, Place::Bytes(bytes))
                .into(),
            err => Error::failed(err.to_string()),
        }
    }
}

impl From<astli_resolve::Error> for Error {
    fn from(err: astli_resolve::Error) -> Error {
        match err {
            astli_resolve::Error::Io(path, err) => Error::Io(path, err),
            astli_resolve::Error::Package(err) => err.into(),
            err => Error::failed(err.to_string()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(path, err) => write!(f, "{}: {err}", path.display()),
            Error::Failed(problem) => write!(f, "{}", problem.message),
            Error::Filelist(path, 0, what) => write!(f, "{}: {what}", path.display()),
            Error::Filelist(path, line, what) => write!(f, "{}:{line}: {what}", path.display()),
            Error::Output(err) => write!(f, "{err}"),
            Error::Silent => Ok(()),
        }
    }
}

pub type Result<T = ()> = std::result::Result<T, Error>;
