//! Subcommand implementations and batch file execution infrastructure.
//!
//! Each subcommand defines its specific arguments and execution logic in its own
//! module. This module provides execution contexts ([`Ctx`], [`Sink`]) and batch
//! execution runners ([`each`]) with support for parallel processing and output buffering.

pub mod add;
pub mod bender;
pub mod build;
pub mod cache;
pub mod check;
pub mod clean;
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
pub mod remove;
pub mod run;
pub mod snapshot;
pub mod tree;
pub mod update;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Condvar, Mutex, mpsc};

use rayon::ThreadPoolBuilder;

use crate::cli::RunArgs;
use crate::error::{Error, Result};
use crate::render::{Out, have, plural};
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
    pub(crate) fn new(files: usize) -> Outcome<T> {
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
                "{wrong} of {} {} errors",
                plural(total, "file"),
                have(wrong),
            ))),
            (failed, 0, total) => Err(Error::tally(format!(
                "{failed} of {} failed",
                plural(total, "file")
            ))),
            (failed, wrong, total) => Err(Error::tally(format!(
                "{failed} of {} failed, and {wrong} {} errors",
                plural(total, "file"),
                have(wrong),
            ))),
        }
    }
}

/// Runs `work` on each of `files` on `jobs` threads, and writes what each
/// says in the order given. With more than one file, each file's output
/// follows a heading naming it, after `heading`'s text, unless `heading` is
/// `None`.
pub fn each<T: Send>(
    out: &mut Out,
    files: &[impl AsRef<Path> + Sync],
    jobs: usize,
    heading: Option<&str>,
    work: impl Fn(&mut Sink, &Path) -> Result<T> + Sync,
) -> Result<Outcome<T>> {
    let heading = heading.filter(|_| files.len() > 1);

    match jobs == 1 || files.len() < 2 {
        true => sequential(out, files, heading, work),
        false => parallel(out, files, heading, work, jobs),
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

/// Runs `work` on every file across a pool of `jobs` threads, and writes what
/// each says in the order the files are given, as soon as the files before it
/// are written.
///
/// Each thread takes the next file as it finishes one, so a large file holds
/// up only its own thread. A thread takes no further file while the files
/// waiting to be written hold more than [`HELD`] bytes of output; the file
/// all the others wait for is always one already taken.
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

    let threads = pool.current_num_threads().min(files.len());
    let queue = Queue::default();
    let (sent, done) = mpsc::channel::<(usize, Done<T>)>();
    let mut outcome = Outcome::new(files.len());
    let written = pool.in_place_scope(|scope| {
        for _ in 0..threads {
            let sent = sent.clone();
            let (queue, work) = (&queue, &work);
            scope.spawn(move |_| {
                // A thread that panics stops the others, rather than leaving
                // them to wait for output that will never be written.
                let _stop = Stop(queue);
                while let Some(at) = queue.take(files.len()) {
                    let (mut printed, mut said) = (Vec::new(), Vec::new());
                    let mut sink = Sink {
                        out: &mut printed,
                        diagnostics: &mut said,
                        errors: 0,
                    };
                    let value = work(&mut sink, files[at].as_ref());
                    let errors = sink.errors;
                    queue.hold(printed.len() + said.len());
                    let done = Done {
                        value,
                        printed,
                        said,
                        errors,
                    };
                    if sent.send((at, done)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sent);
        let written = write(out, files, heading, &done, &queue, &mut outcome);
        queue.stop();
        written
    });
    written?;
    Ok(outcome)
}

/// Writes each file's output as it arrives on `done`, in the order of
/// `files`.
fn write<T>(
    out: &mut Out,
    files: &[impl AsRef<Path>],
    heading: Option<&str>,
    done: &mpsc::Receiver<(usize, Done<T>)>,
    queue: &Queue,
    outcome: &mut Outcome<T>,
) -> Result {
    let mut waiting: BTreeMap<usize, Done<T>> = BTreeMap::new();
    for (at, file) in files.iter().enumerate() {
        let done = loop {
            if let Some(done) = waiting.remove(&at) {
                break done;
            }
            match done.recv() {
                Ok((it, done)) => {
                    waiting.insert(it, done);
                }
                // A thread panicked, which the scope raises again.
                Err(_) => return Ok(()),
            }
        };
        queue.release(done.printed.len() + done.said.len());
        let turned = turn(out, heading, at, file.as_ref(), outcome, |out| {
            out.write_all(&done.printed)?;
            done.value
        });
        report(out, &done.said, done.errors, outcome)?;
        turned?;
    }
    Ok(())
}

/// Completed result and buffered outputs from a worker thread.
struct Done<T> {
    value: Result<T>,
    printed: Vec<u8>,
    said: Vec<u8>,
    errors: usize,
}

/// The most output the files waiting to be written may hold before threads
/// stop taking files, unless the file they wait for is still to come.
const HELD: usize = 64 << 20;

/// Which file is next, and how much output waits to be written.
#[derive(Default)]
struct Queue {
    state: Mutex<Taken>,
    released: Condvar,
}

#[derive(Default)]
struct Taken {
    next: usize,
    held: usize,
    stopped: bool,
}

impl Queue {
    /// The next file of `files`, once the output waiting allows another;
    /// `None` when every file is taken or the run stopped.
    fn take(&self, files: usize) -> Option<usize> {
        let mut state = self.state.lock().unwrap();
        while state.held > HELD && !state.stopped {
            state = self.released.wait(state).unwrap();
        }
        if state.stopped || state.next == files {
            return None;
        }
        state.next += 1;
        Some(state.next - 1)
    }

    fn hold(&self, bytes: usize) {
        self.state.lock().unwrap().held += bytes;
    }

    fn release(&self, bytes: usize) {
        self.state.lock().unwrap().held -= bytes;
        self.released.notify_all();
    }

    fn stop(&self) {
        // A thread panicking while holding the lock poisons it, and the scope
        // raises that panic anyway.
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
        }
        self.released.notify_all();
    }
}

/// Stops the queue when dropped by a panicking thread.
struct Stop<'a>(&'a Queue);

impl Drop for Stop<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.stop();
        }
    }
}

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
