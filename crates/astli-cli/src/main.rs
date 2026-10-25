//! CLI driver for the astli SystemVerilog toolchain.
//!
//! Subcommands format, lint and check a design, work on its filelist, or dump a single pipeline
//! stage (`lex`, `preprocess`, `parse`, `elab`); output goes to stdout and diagnostics to stderr.

mod cli;
mod cmd;
mod config;
mod discovery;
mod error;
mod filelist;
mod git;
mod lockfile;
mod render;
mod session;
mod sources;
mod term;

use std::io::{ErrorKind, Write};
use std::process::ExitCode;

use usage_rs::RunWith;

use cli::Astli;
use cmd::Ctx;

/// The system allocator on macOS spends a third of `fmt` in malloc and
/// contends across worker threads.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
use error::Error;
use render::Out;
use term::say;

fn main() -> ExitCode {
    let Astli { command, run } = Astli::parse();
    // Set before anything is written, and before any stage runs, so that
    // neither can be set twice.
    term::init(run.color);
    let _ = render::FORMAT.set(run.diagnostics);
    let mut out = Out::new(std::io::stdout());

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
            say!("astli: {err}");
            ExitCode::FAILURE
        }
    }
}
