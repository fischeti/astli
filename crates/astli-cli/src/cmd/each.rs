//! Running a command's work on each of its files, on one thread or on the
//! pool, and writing what each says in the order the files are given; and
//! the tally of what failed.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, mpsc};

use crate::error::{Error, Result};
use crate::render::{Report, have, plural};
use crate::run::Run;

/// Output sinks for processing a single file.
///
/// Keeps primary output (stdout) separate from human-readable diagnostics (stderr)
/// so command output can be safely redirected or piped into downstream tools.
pub struct Sink<'a> {
    /// Standard output buffer for generated artifacts (tokens, AST, preprocessed source).
    pub out: &'a mut dyn Write,
    /// What the file's work says on stderr, and how many errors that holds.
    pub report: Report,
}

/// Aggregated results and error counts across all processed files.
pub struct Outcome<T> {
    /// Successfully processed file results, ordered by input sequence.
    pub values: Vec<T>,
    /// Number of files that could not be processed (e.g. I/O failure).
    pub failed: usize,
    /// Where among the files given to [`each`] each one that could not be
    /// processed is, in order.
    pub failures: Vec<usize>,
    /// Number of files that were processed but produced diagnostic errors.
    pub wrong: usize,
}

impl<T> Outcome<T> {
    pub(crate) fn new(files: usize) -> Outcome<T> {
        Outcome {
            values: Vec::with_capacity(files),
            failed: 0,
            failures: Vec::new(),
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

    /// The paths of `files`, as given to [`each`], that could not be
    /// processed.
    pub fn failed_of(&self, files: &[impl AsRef<Path>]) -> Vec<PathBuf> {
        (self.failures.iter())
            .map(|&at| files[at].as_ref().to_path_buf())
            .collect()
    }

    fn total(&self) -> usize {
        self.values.len() + self.failed
    }

    /// Evaluates overall run outcome and returns an appropriate error if any files failed.
    pub fn finish(&self) -> Result {
        self.finish_with(0)
    }

    /// Ends the run as [`Outcome::finish`] does, counting `unread` more
    /// files that could not be read, reported before these were processed.
    pub fn finish_with(&self, unread: usize) -> Result {
        match (self.failed + unread, self.wrong) {
            (0, 0) => Ok(()),
            (failed, wrong) => Err(tally(failed, wrong, self.total() + unread)),
        }
    }
}

/// The error that ends a run in which, of `total` files, `failed` could not
/// be processed and `wrong` held errors, each reported already.
///
/// Of one file, it says nothing more: what was reported of the file is the
/// whole story.
pub fn tally(failed: usize, wrong: usize, total: usize) -> Error {
    if total == 1 {
        return Error::Silent;
    }
    let files = plural(total, "file");
    Error::tally(match (failed, wrong) {
        (0, wrong) => format!("{wrong} of {files} {} errors", have(wrong)),
        (failed, 0) => format!("{failed} of {files} failed"),
        (failed, wrong) => format!(
            "{failed} of {files} failed, and {wrong} {} errors",
            have(wrong)
        ),
    })
}

/// Runs `work` on each of `files`, on this thread when the run has one job
/// and on the global pool otherwise, and writes what each says in the order given. With more than one file, each file's output
/// follows a heading naming it, after `heading`'s text, unless `heading` is
/// `None`.
pub fn each<T: Send, W: Write>(
    out: &mut W,
    files: &[impl AsRef<Path> + Sync],
    run: &Run,
    heading: Option<&str>,
    work: impl Fn(&mut Sink, &Path) -> Result<T> + Sync,
) -> Result<Outcome<T>> {
    let heading = heading.filter(|_| files.len() > 1);

    match run.args.jobs == 1 || files.len() < 2 {
        true => sequential(out, run, files, heading, work),
        false => parallel(out, run, files, heading, work),
    }
}

/// Executes file processing sequentially in the current thread.
fn sequential<T, W: Write>(
    out: &mut W,
    run: &Run,
    files: &[impl AsRef<Path>],
    heading: Option<&str>,
    work: impl Fn(&mut Sink, &Path) -> Result<T>,
) -> Result<Outcome<T>> {
    let mut outcome = Outcome::new(files.len());

    for (at, file) in files.iter().enumerate() {
        let file = file.as_ref();
        let mut said = Report::default();
        let value = turn(out, run, heading, at, file, &mut outcome, |out| {
            let mut sink = Sink {
                out,
                report: Report::default(),
            };
            let value = work(&mut sink, file);
            said = sink.report;
            value
        });
        report(out, &said, &mut outcome)?;
        value?;
    }

    Ok(outcome)
}

/// Flushes stdout before emitting diagnostics to stderr to maintain proper message ordering.
fn report<T>(out: &mut impl Write, said: &Report, outcome: &mut Outcome<T>) -> Result {
    // Counted whether or not anything was said: a GitLab report keeps a
    // file's errors to write at the end.
    if said.errors > 0 {
        outcome.wrong += 1;
    }
    if !said.said.is_empty() {
        out.flush()?;
    }
    said.tell().map_err(Error::Output)
}

/// Runs `work` on every file across rayon's global pool, which `main` sizes
/// from `-j`, and writes what each says in the order the files are given, as
/// soon as the files before it are written.
///
/// Each thread takes the next file as it finishes one, so a large file holds
/// up only its own thread. A thread takes no further file while the files
/// waiting to be written hold more than [`HELD`] bytes of output; the file
/// all the others wait for is always one already taken.
fn parallel<T: Send, W: Write>(
    out: &mut W,
    run: &Run,
    files: &[impl AsRef<Path> + Sync],
    heading: Option<&str>,
    work: impl Fn(&mut Sink, &Path) -> Result<T> + Sync,
) -> Result<Outcome<T>> {
    let threads = rayon::current_num_threads().min(files.len());
    let queue = Queue::default();
    let (sent, done) = mpsc::channel::<(usize, Done<T>)>();
    let mut outcome = Outcome::new(files.len());
    let written = rayon::in_place_scope(|scope| {
        for _ in 0..threads {
            let sent = sent.clone();
            let (queue, work) = (&queue, &work);
            scope.spawn(move |_| {
                // A thread that panics stops the others, rather than leaving
                // them to wait for output that will never be written.
                let _stop = Stop(queue);
                while let Some(at) = queue.take(files.len()) {
                    let mut printed = Vec::new();
                    let mut sink = Sink {
                        out: &mut printed,
                        report: Report::default(),
                    };
                    let value = work(&mut sink, files[at].as_ref());
                    let said = sink.report;
                    queue.hold(printed.len() + said.said.len());
                    let done = Done {
                        value,
                        printed,
                        said,
                    };
                    if sent.send((at, done)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sent);
        let written = write(out, run, files, heading, &done, &queue, &mut outcome);
        queue.stop();
        written
    });
    written?;
    Ok(outcome)
}

/// Writes each file's output as it arrives on `done`, in the order of
/// `files`.
fn write<T, W: Write>(
    out: &mut W,
    run: &Run,
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
        queue.release(done.printed.len() + done.said.said.len());
        let turned = turn(out, run, heading, at, file.as_ref(), outcome, |out| {
            out.write_all(&done.printed)?;
            done.value
        });
        report(out, &done.said, outcome)?;
        turned?;
    }
    Ok(())
}

/// Completed result and buffered outputs from a worker thread.
struct Done<T> {
    value: Result<T>,
    printed: Vec<u8>,
    said: Report,
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
fn turn<T, W: Write>(
    out: &mut W,
    run: &Run,
    heading: Option<&str>,
    at: usize,
    file: &Path,
    outcome: &mut Outcome<T>,
    write: impl FnOnce(&mut W) -> Result<T>,
) -> Result {
    if let Some(prefix) = heading {
        if at > 0 {
            writeln!(out)?;
        }
        writeln!(out, "{prefix}=== {} ===", file.display())?;
    }

    match write(out) {
        Ok(value) => outcome.values.push(value),
        // Neither is the file's: the run stops.
        Err(err @ (Error::Output(_) | Error::Overtaken)) => return Err(err),
        Err(err) => {
            out.flush()?;
            err.problem().report(run);
            outcome.failed += 1;
            outcome.failures.push(at);
        }
    }

    Ok(())
}
