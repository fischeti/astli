//! `svirig completion` -- the shell script `usage` writes from the same
//! declarations the argument parser is generated from.

use std::io::Write;

use usage::RunWith;
use usage::complete::Shell as Target;

use crate::cli::{Completion, Shell, Svirig};
use crate::error::Result;
use crate::render::Out;

impl RunWith<&mut Out> for Completion {
    type Output = Result;

    fn run_with(self, out: &mut Out) -> Result {
        let target = match self.shell {
            Shell::Bash => Target::Bash,
            Shell::Zsh => Target::Zsh,
            Shell::Fish => Target::Fish,
        };
        write!(out, "{}", Svirig::completion_script(target))?;
        Ok(())
    }
}
