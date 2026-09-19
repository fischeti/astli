//! `svirig fmt` subcommand (placeholder).
//!
//! Defines the CLI interface for the planned code formatter (`--check`, `--write`).
//! The formatting engine itself is not yet implemented.

use usage::{Args, Run};

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::sources;

/// Format SystemVerilog source files (not yet implemented).
#[derive(Args)]
pub struct Fmt {
    #[usage(flatten)]
    pub sources: Sources,
    /// Check formatting and exit with a non-zero code if unformatted, without modifying files
    #[usage(long)]
    pub check: bool,
    /// Rewrite files in place with formatted output
    #[usage(short = 'w', long)]
    pub write: bool,
}

impl Run for Fmt {
    type Output = Result;

    fn run(self) -> Result {
        // Validate source paths and filelists up front even while formatting is unimplemented.
        sources::resolve(&self.sources, &BuildArgs::default())?;
        Err(Error::Unimplemented("fmt"))
    }
}
