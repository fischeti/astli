//! CLI driver for the astli SystemVerilog toolchain.
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

use cli::Astli;
use cmd::Ctx;

/// The system allocator on macOS spends a third of `fmt` in malloc and
/// contends across worker threads.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
use error::Error;
use render::Out;

fn main() -> ExitCode {
    let Astli { command, run } = Astli::parse();
    let mut out = Out::new();

    let result = command.run_with(Ctx {
        out: &mut out,
        run: &run,
    });

    // Flushed whatever the result, so that what a failed run printed comes out
    // ahead of the error that ends it.
    let flushed = out.flush();
    let result = result.and_then(|()| flushed.map_err(Error::from));

    match result {
        Ok(()) => ExitCode::SUCCESS,
        // Broken pipe (e.g. piping into head) is treated as a clean exit.
        Err(Error::Output(err)) if err.kind() == ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        // Failure diagnostics have already been printed.
        Err(Error::Silent) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("astli: {err}");
            ExitCode::FAILURE
        }
    }
}
