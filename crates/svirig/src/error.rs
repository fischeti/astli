//! Driver error types and exit code representations.

use std::fmt;
use std::path::{Path, PathBuf};

/// Error conditions encountered during CLI command execution.
#[derive(Debug)]
pub enum Error {
    /// An I/O error occurred while reading or writing a file.
    Io(PathBuf, std::io::Error),
    /// Command execution failed with a user-facing error message.
    Failed(String),
    /// A syntax or resolution error occurred while parsing a `.f` filelist.
    Filelist(PathBuf, usize, String),
    /// The requested command or feature is not yet implemented.
    Unimplemented(&'static str),
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
        Error::Failed(what.into())
    }

    /// Creates a filelist syntax or resolution error at `line` in `path`.
    pub fn filelist(path: impl AsRef<Path>, line: usize, what: impl Into<String>) -> Error {
        Error::Filelist(path.as_ref().to_path_buf(), line, what.into())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Output(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(path, err) => write!(f, "{}: {err}", path.display()),
            Error::Failed(what) => write!(f, "{what}"),
            Error::Filelist(path, 0, what) => write!(f, "{}: {what}", path.display()),
            Error::Filelist(path, line, what) => write!(f, "{}:{line}: {what}", path.display()),
            Error::Unimplemented(what) => {
                write!(f, "{what} is not implemented yet; see docs/plan.md")
            }
            Error::Output(err) => write!(f, "{err}"),
            Error::Silent => Ok(()),
        }
    }
}

pub type Result<T = ()> = std::result::Result<T, Error>;
