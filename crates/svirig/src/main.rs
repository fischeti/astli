//! CLI driver for the svirig SystemVerilog toolchain.
//!
//! Subcommands correspond to individual pipeline stages (`lex`, `preprocess`, `parse`, `fmt`),
//! outputting transformed text or debug tree representations to stdout and diagnostics to stderr.

mod cli;
mod cmd;
mod error;
mod filelist;
mod render;
mod session;
mod sources;

use std::io::{ErrorKind, Write};
use std::process::ExitCode;

use usage::RunWith;

use cli::Svirig;
use cmd::Ctx;
use error::Error;
use render::Out;

fn main() -> ExitCode {
    let Svirig { command, run } = Svirig::parse();
    let mut out = Out::new();

    let result = command.run_with(Ctx {
        out: &mut out,
        run: &run,
    });

    let result = result.and_then(|()| out.flush().map_err(Error::from));

    match result {
        Ok(()) => ExitCode::SUCCESS,
        // Broken pipe (e.g. piping into head) is treated as a clean exit.
        Err(Error::Output(err)) if err.kind() == ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        // Failure diagnostics have already been printed.
        Err(Error::Silent) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("svirig: {err}");
            ExitCode::FAILURE
        }
    }
}
