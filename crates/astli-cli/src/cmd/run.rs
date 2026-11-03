//! `astli run` subcommand.
//!
//! Builds a top as `astli build` does, then runs the simulation from the
//! current directory, where a testbench finds the memory images and stimuli
//! it names by relative path. What follows `--` is passed to it as it is,
//! plusargs most often, and the run exits as the simulation does, so that a
//! `$fatal` or a failed assertion fails a CI job without a script to read
//! the log.

use std::ffi::OsString;
use std::process::{Command, ExitStatus};

use usage_rs::{Args, RunWith};

use astli_package::mark;

use crate::cmd::Ctx;
use crate::cmd::build::Build;
use crate::error::{Error, Result};
use crate::term::{self, status};

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
        let executable = self.build.built(ctx)?;
        let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        let shown = executable.strip_prefix(&here).unwrap_or(&executable);
        status!("Running", "{}", mark::path(shown.display()));
        let status = (Command::new(&executable).args(&self.args).status())
            .map_err(|err| Error::io(&executable, err))?;
        match status.code() {
            Some(0) => Ok(()),
            // Above 255 only on Windows, where a status is 32 bits.
            Some(code) => Err(Error::Exited(u8::try_from(code).unwrap_or(1))),
            None => Err(signalled(status)),
        }
    }
}

/// How a run ends whose simulation a signal ended: with the status a shell
/// gives it, 128 and the signal, and only a note, since Verilator before
/// 5.040 ends a `$fatal` or a `$stop` by aborting, after saying why.
#[cfg(unix)]
fn signalled(status: ExitStatus) -> Error {
    use std::os::unix::process::ExitStatusExt;

    let signal = status.signal().unwrap_or(0);
    term::note(format!("the simulation was ended by signal {signal}")).report();
    Error::Exited(u8::try_from(128 + signal).unwrap_or(u8::MAX))
}

/// Every process on Windows ends with a status.
#[cfg(not(unix))]
fn signalled(status: ExitStatus) -> Error {
    Error::failed(format!("the simulation ended without a status: {status}"))
}
