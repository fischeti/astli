//! `svirig fmt` -- not implemented.
//!
//! Declared ahead of the formatter so that `--check` and `--write` are fixed
//! before anything is built behind them: those two are what every caller of a
//! formatter writes, and settling them late means settling them twice. What
//! the command does not have is a `--line-width` or an `--indent`, because
//! [D7](../../../../docs/plan.md) keeps the knobs few and adding one here
//! would be deciding that before the formatter exists.

use usage::Run;

use crate::cli::{BuildArgs, Fmt};
use crate::error::{Error, Result};
use crate::sources;

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
