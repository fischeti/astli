//! CLI driver for the astli SystemVerilog toolchain.
//!
//! Subcommands format, lint and check a design, work on its filelist, or dump a single pipeline
//! stage (`lex`, `preprocess`, `parse`, `elab`); output goes to stdout and diagnostics to stderr.

mod analysis;
mod cli;
mod cmd;
mod config;
mod discovery;
mod error;
mod filelist;
mod formats;
mod git;
mod host;
mod json;
mod listing;
mod manifest;
mod marks;
mod points;
mod render;
mod run;
mod session;
mod settle;
mod sim;
mod sources;
mod target;
mod term;
mod tops;
mod verilator;

use std::io::{ErrorKind, Write};
use std::process::ExitCode;

use usage_rs::RunWith;

use cli::{Astli, DiagnosticFormat};
use cmd::Ctx;

/// The system allocator on macOS spends a third of `fmt` in malloc and
/// contends across worker threads.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
use error::Error;
use render::Out;

fn main() -> ExitCode {
    let Astli { command, run } = Astli::parse();
    // Set before anything is written, and before any stage runs, so that
    // neither can be set twice.
    term::init(run.color, run.quiet, run.diagnostics);
    let _ = render::FORMAT.set(run.diagnostics);
    let run = run::Run::new(run);
    let mut out = Out::new(std::io::stdout());

    // Sized once, before any stage runs, so that every `par_iter`, in the
    // driver or in a library, shares the `-j` threads.
    let result = rayon::ThreadPoolBuilder::new()
        .num_threads(run.args.jobs)
        .build_global()
        .map_err(|err| {
            let threads = render::plural(run.args.jobs, "thread");
            Error::failed(format!("could not start {threads}: {err}"))
        })
        .and_then(|()| {
            command.run_with(Ctx {
                out: &mut out,
                run: &run,
            })
        });

    // Flushed whatever the result, so that what a failed run printed comes out
    // ahead of the error that ends it.
    let flushed = out.flush();
    let result = result.and_then(|()| flushed.map_err(Error::from));

    let code = match result {
        Ok(()) => ExitCode::SUCCESS,
        // Broken pipe (e.g. piping into head) is treated as a clean exit.
        Err(Error::Output(err)) if err.kind() == ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        // Failure diagnostics have already been printed.
        Err(Error::Silent) => ExitCode::FAILURE,
        Err(Error::Exited(code)) => ExitCode::from(code),
        Err(err) => {
            err.problem().report(&run);
            ExitCode::FAILURE
        }
    };

    // Written last, whatever the result, so that it holds the whole run.
    if run.args.diagnostics == DiagnosticFormat::Gitlab {
        let mut report = Vec::new();
        let _ = formats::write_gitlab(&mut report);
        let _ = term::write(&report);
    }
    code
}
