//! `astli run` subcommand.
//!
//! Builds a top as `astli build` does, then runs the simulation from the
//! current directory, where a testbench finds the memory images and stimuli
//! it names by relative path. What follows `--` is passed to it as it is,
//! plusargs most often, after what the profile gives every run, and the
//! run exits as the simulation does, so that a `$fatal` or a failed
//! assertion fails a CI job without a script to read the log.

use std::ffi::OsString;
use std::process::Command;

use usage_rs::{Args, RunWith};

use astli_package::mark;

use crate::cmd::Ctx;
use crate::cmd::build::Build;
use crate::error::{Error, Result};
use crate::sim;
use crate::term::status;

/// Build a top of the package and run its simulation
#[derive(Args)]
pub struct Run {
    #[usage(flatten)]
    pub build: Build,
    /// Arguments passed to the simulation, such as plusargs, after `--`
    #[usage(arg, value_name = "ARGS")]
    pub args: Vec<OsString>,
}

impl RunWith<Ctx<'_>> for Run {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let run = ctx.run;
        let (executable, mut args) = self.build.built(ctx)?;
        args.extend(self.args);
        let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        let shown = executable.strip_prefix(&here).unwrap_or(&executable);
        status!("Running", "{}", mark::path(shown.display()));
        let status = (Command::new(&executable).args(&args).status())
            .map_err(|err| Error::io(&executable, err))?;
        match status.code() {
            Some(0) => Ok(()),
            // Above 255 only on Windows, where a status is 32 bits.
            Some(code) => Err(Error::Exited(u8::try_from(code).unwrap_or(1))),
            None => Err(sim::signalled(run, status)),
        }
    }
}
