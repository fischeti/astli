//! `svirig completion` -- the shell script `usage` writes from the same
//! declarations the argument parser is generated from.

use std::io::Write;

use usage::complete::Shell as Target;
use usage::{Args, RunWith, ValueEnum};

use crate::cli::Svirig;
use crate::cmd::Ctx;
use crate::error::Result;

/// Print a shell completion script.
#[derive(Args)]
pub struct Completion {
    /// Which shell to generate for
    #[usage(value_enum)]
    pub shell: Shell,
}

/// The shells `usage` can write a script for.
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
        write!(out, "{}", Svirig::completion_script(target))?;
        Ok(())
    }
}
