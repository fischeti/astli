//! `svirig fmt` -- not implemented.
//!
//! Declared ahead of the formatter so that `--check` and `--write` are fixed
//! before anything is built behind them: those two are what every caller of a
//! formatter writes, and settling them late means settling them twice. What
//! the command does not have is a `--line-width` or an `--indent`, because
//! [D7](../../../../docs/plan.md) keeps the knobs few and adding one here
//! would be deciding that before the formatter exists.

use usage::{Args, Run};

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::sources;

/// Format a file. Not implemented.
#[derive(Args)]
pub struct Fmt {
    #[usage(flatten)]
    pub sources: Sources,
    /// Exit non-zero if a file is not already formatted, and write nothing
    #[usage(long)]
    pub check: bool,
    /// Rewrite each file in place instead of printing it
    #[usage(short = 'w', long)]
    pub write: bool,
}

impl Run for Fmt {
    type Output = Result;

    fn run(self) -> Result {
        // Resolved even though nothing is done with it: a filelist that names
        // nothing, or names a file that is not there, should say so here rather
        // than on the day there is a formatter.
        sources::resolve(&self.sources, &BuildArgs::default())?;
        Err(Error::Unimplemented("fmt"))
    }
}
