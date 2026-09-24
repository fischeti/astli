//! `astli completion` subcommand.
//!
//! Generates shell completion scripts for supported shells (Bash, Zsh, Fish).

use std::io::Write;

use usage::complete::Shell as Target;
use usage::{Args, RunWith, ValueEnum};

use crate::cli::Astli;
use crate::cmd::Ctx;
use crate::error::Result;

/// Generate shell completion scripts.
#[derive(Args)]
pub struct Completion {
    /// Target shell to generate completions for
    #[usage(value_enum)]
    pub shell: Shell,
}

/// Supported shell completion targets.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl RunWith<Ctx<'_>> for Completion {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let out = ctx.out;
        let target = match self.shell {
            Shell::Bash => Target::Bash,
            Shell::Zsh => Target::Zsh,
            Shell::Fish => Target::Fish,
        };
        write!(out, "{}", Astli::completion_script(target))?;
        Ok(())
    }
}
