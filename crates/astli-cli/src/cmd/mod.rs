//! Subcommand implementations and batch file execution infrastructure.
//!
//! Each subcommand defines its specific arguments and execution logic in its own
//! module. This module provides execution contexts ([`Ctx`], [`Sink`]) and batch
//! execution runners ([`each`]) with support for parallel processing and output buffering.

pub mod cache;
pub mod check;
pub mod completion;
pub mod elab;
pub mod fetch;
pub mod files;
pub mod fmt;
pub mod init;
pub mod lex;
pub mod lint;
pub mod parse;
pub mod patch;
pub mod path;
pub mod pickle;
pub mod preprocess;
pub mod snapshot;
pub mod tree;
pub mod update;

use std::io::Write;
use std::path::Path;

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::cli::RunArgs;
use crate::error::{Error, Result};
use crate::render::{Out, plural};
use crate::term;

/// Global execution context passed to subcommands.
pub struct Ctx<'a> {
    /// Standard output writer for command results.
    pub out: &'a mut Out,
    /// Shared runtime arguments (verbosity, parallel jobs).
    pub run: &'a RunArgs,
}

/// Output sinks for processing a single file.
///
/// Keeps primary output (stdout) separate from human-readable diagnostics (stderr)
/// so command output can be safely redirected or piped into downstream tools.
pub struct Sink<'a> {
    /// Standard output buffer for generated artifacts (tokens, AST, preprocessed source).
    pub out: &'a mut dyn Write,
    /// Diagnostic buffer for compiler warnings and errors.
    pub diagnostics: &'a mut dyn Write,
    /// Total number of diagnostic errors encountered for this file.
    pub errors: usize,
}

/// Aggregated results and error counts across all processed files.
pub struct Outcome<T> {
    /// Successfully processed file results, ordered by input sequence.
    pub values: Vec<T>,
    /// Number of files that could not be processed (e.g. I/O failure).
    pub failed: usize,
    /// Number of files that were processed but produced diagnostic errors.
    pub wrong: usize,
}

impl<T> Outcome<T> {
    fn new(files: usize) -> Outcome<T> {
        Outcome {
            values: Vec::with_capacity(files),
            failed: 0,
            wrong: 0,
        }
    }

    /// Formats summary file count prefix (e.g., "3 files" or "2 of 3 files").
    pub fn files(&self) -> String {
        match self.failed {
            0 => plural(self.values.len(), "file"),
            _ => format!("{} of {}", self.values.len(), plural(self.total(), "file")),
        }
    }

    fn total(&self) -> usize {
        self.values.len() + self.failed
    }

    /// Evaluates overall run outcome and returns an appropriate error if any files failed.
    pub fn finish(&self) -> Result {
        match (self.failed, self.wrong, self.total()) {
            (0, 0, _) => Ok(()),
            // Single file failure: diagnostics have already been printed.
            (_, _, 1) => Err(Error::Silent),
            (0, wrong, total) => Err(Error::tally(format!(
                "{wrong} of {} have errors",
                plural(total, "file")
            ))),
            (failed, 0, total) => Err(Error::tally(format!(
                "{failed} of {} failed",
                plural(total, "file")
            ))),
            (failed, wrong, total) => Err(Error::tally(format!(
                "{failed} of {} failed, and {wrong} have errors",
                plural(total, "file")
            ))),
        }
    }
}

/// Processes a list of files with the provided worker function, collecting results.
///
/// Automatically runs sequentially for single files or `-j1`, or concurrently using
/// Rayon when multiple files and jobs are configured. Maintains ordered output regardless
/// of worker completion order.
pub fn each<T: Send>(
    out: &mut Out,
    files: &[impl AsRef<Path> + Sync],
    run: &RunArgs,
    prefix: &str,
    work: impl Fn(&mut Sink, &Path) -> Result<T> + Sync,
) -> Result<Outcome<T>> {
    let heading = (!run.quiet).then_some(prefix).filter(|_| files.len() > 1);

    match run.jobs == 1 || files.len() < 2 {
        true => sequential(out, files, heading, work),
        false => parallel(out, files, heading, work, run.jobs),
    }
}

/// Executes file processing sequentially in the current thread.
fn sequential<T>(
    out: &mut Out,
    files: &[impl AsRef<Path>],
    heading: Option<&str>,
    work: impl Fn(&mut Sink, &Path) -> Result<T>,
) -> Result<Outcome<T>> {
    let mut outcome = Outcome::new(files.len());

    for (at, file) in files.iter().enumerate() {
        let file = file.as_ref();
        let mut said = Vec::new();
        let mut errors = 0;
        let value = turn(out, heading, at, file, &mut outcome, |out| {
            let mut sink = Sink {
                out,
                diagnostics: &mut said,
                errors: 0,
            };
            let value = work(&mut sink, file);
            errors = sink.errors;
            value
        });
        report(out, &said, errors, &mut outcome)?;
        value?;
    }

    Ok(outcome)
}

/// Flushes stdout before emitting diagnostics to stderr to maintain proper message ordering.
fn report<T>(out: &mut Out, said: &[u8], errors: usize, outcome: &mut Outcome<T>) -> Result {
    // Counted whether or not anything was said: a GitLab report keeps a
    // file's errors to write at the end.
    if errors > 0 {
        outcome.wrong += 1;
    }
    if said.is_empty() {
        return Ok(());
    }
    out.flush()?;
    term::write(said).map_err(Error::Output)?;
    Ok(())
}

/// Executes file processing in parallel batches (waves) across a thread pool.
///
/// Dynamically adjusts batch size based on memory usage of completed files while
/// writing outputs in strict input order.
fn parallel<T: Send>(
    out: &mut Out,
    files: &[impl AsRef<Path> + Sync],
    heading: Option<&str>,
    work: impl Fn(&mut Sink, &Path) -> Result<T> + Sync,
    jobs: usize,
) -> Result<Outcome<T>> {
    let pool = ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .map_err(|err| {
            Error::failed(format!("could not start {}: {err}", plural(jobs, "thread")))
        })?;

    let threads = pool.current_num_threads();
    let widest = 64 * threads;
    let mut wave = 4 * threads;
    let mut outcome = Outcome::new(files.len());
    let mut at = 0;

    while at < files.len() {
        let read = &files[at..(at + wave).min(files.len())];
        let done: Vec<Done<T>> = pool.install(|| {
            read.par_iter()
                .map(|file| {
                    let (mut printed, mut said) = (Vec::new(), Vec::new());
                    let mut sink = Sink {
                        out: &mut printed,
                        diagnostics: &mut said,
                        errors: 0,
                    };
                    let value = work(&mut sink, file.as_ref());
                    let errors = sink.errors;
                    Done {
                        value,
                        printed,
                        said,
                        errors,
                    }
                })
                .collect()
        });

        let held = done.iter().map(|done| done.printed.len()).sum::<usize>();
        wave = match held / read.len() {
            0 => widest,
            each => (HELD / each).clamp(threads, widest),
        };

        for (file, done) in read.iter().zip(done) {
            let turned = turn(out, heading, at, file.as_ref(), &mut outcome, |out| {
                out.write_all(&done.printed)?;
                done.value
            });
            report(out, &done.said, done.errors, &mut outcome)?;
            turned?;
            at += 1;
        }
    }

    Ok(outcome)
}

/// Completed result and buffered outputs from a worker thread.
struct Done<T> {
    value: Result<T>,
    printed: Vec<u8>,
    said: Vec<u8>,
    errors: usize,
}

/// Target memory threshold (64 MiB) for buffered output before flushing a wave.
const HELD: usize = 64 << 20;

/// Emits header, output, and error status for an individual file in sequence.
fn turn<T>(
    out: &mut Out,
    heading: Option<&str>,
    at: usize,
    file: &Path,
    outcome: &mut Outcome<T>,
    write: impl FnOnce(&mut Out) -> Result<T>,
) -> Result {
    if let Some(prefix) = heading {
        if at > 0 {
            writeln!(out)?;
        }
        writeln!(out, "{prefix}=== {} ===", file.display())?;
    }

    match write(out) {
        Ok(value) => outcome.values.push(value),
        Err(err @ Error::Output(_)) => return Err(err),
        Err(err) => {
            out.flush()?;
            err.problem().report();
            outcome.failed += 1;
        }
    }

    Ok(())
}
