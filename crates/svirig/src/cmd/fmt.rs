//! `svirig fmt` -- not implemented.
//!
//! Declared ahead of the formatter so that `--check` and `--write` are fixed
//! before anything is built behind them: those two are what every caller of a
//! formatter writes, and settling them late means settling them twice. What
//! the command does not have is a `--line-width` or an `--indent`, because
//! [D7](../../../../docs/plan.md) keeps the knobs few and adding one here
//! would be deciding that before the formatter exists.

use crate::cli::Fmt;
use crate::error::{Error, Result};

pub fn run(_args: &Fmt) -> Result {
    Err(Error::Unimplemented("fmt"))
}
