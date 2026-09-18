//! What the driver reports, and what it exits with.
//!
//! Three exit codes. `0` is what was asked for, `1` is a file that is wrong,
//! and `2` is a command line that is -- the last of those is `usage`'s own and
//! never reaches this type.
//!
//! Nothing here renders a snippet or a span. That is `svirig-diag`, which
//! arrives with the first thing that has more than one error to report; until
//! then a location is `path:line:col` and a message is a line of text, which
//! is what the examples this crate replaces already printed.

use std::fmt;
use std::path::{Path, PathBuf};

pub enum Error {
    /// A path that could not be read.
    Io(PathBuf, std::io::Error),
    /// The file was read, and something about it is wrong. Whatever detail
    /// there is has already been printed; this is the summary and the exit
    /// code.
    Failed(String),
    /// A command that is declared and not built.
    Unimplemented(&'static str),
    /// Writing the output itself failed. A closed pipe is the one that
    /// happens -- `svirig parse big.sv | head` -- and it is not an error.
    Output(std::io::Error),
}

impl Error {
    pub fn io(path: impl AsRef<Path>, err: std::io::Error) -> Error {
        Error::Io(path.as_ref().to_path_buf(), err)
    }

    pub fn failed(what: impl Into<String>) -> Error {
        Error::Failed(what.into())
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
            Error::Unimplemented(what) => {
                write!(f, "{what} is not implemented yet; see docs/plan.md")
            }
            Error::Output(err) => write!(f, "{err}"),
        }
    }
}

pub type Result<T = ()> = std::result::Result<T, Error>;
