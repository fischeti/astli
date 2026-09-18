//! The driver: one subcommand per stage of the pipeline, each printing what
//! that stage produced.
//!
//! `lex` is the tokens, `preprocess` is what the directives and macros make of
//! them, `parse` is the tree, and `fmt` is the file -- when there is a
//! formatter. Every one of them was an example under `crates/*/examples`
//! first, each with its own hand-rolled argument loop; this is those, with one
//! argument parser and one convention about where output goes.
//!
//! What stays an example is anything that sweeps the corpus. The dividing line
//! is the file: a per-file dump is a subcommand, and a question about a
//! corpus is a research instrument that only this repository runs.
//!
//! # Output
//!
//! Dumps go to stdout and diagnostics to stderr, so that either can be
//! redirected without the other. `0` is what was asked for, `1` is a file that
//! is wrong, `2` is a command line that is.

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

    // The flush is where a buffered write actually fails, so it is part of the
    // result rather than something left to the drop.
    let result = result.and_then(|()| out.flush().map_err(Error::from));

    match result {
        Ok(()) => ExitCode::SUCCESS,
        // Whoever was reading stopped reading. `head` does it on every file
        // large enough to be worth piping, and it is not a failure.
        Err(Error::Output(err)) if err.kind() == ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        // Already reported, by whoever knew what to say about it.
        Err(Error::Silent) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("svirig: {err}");
            ExitCode::FAILURE
        }
    }
}
