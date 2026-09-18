//! One module per subcommand. Each takes the parsed arguments and does the
//! work; none of them mentions the argument parser.

pub mod completion;
pub mod fmt;
pub mod lex;
pub mod parse;
pub mod preprocess;

use std::io::Write;
use std::path::Path;

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::cli::Run;
use crate::error::{Error, Result};
use crate::render::Out;

/// What a run over several files produced, and how much of it did not.
///
/// The per-file work returns a value rather than printing one, so that the
/// figures are summed over the run: a file at a time is not a rate, and a
/// filelist is the size at which a rate means something.
pub struct Outcome<T> {
    /// One per file that made it, in the order the files were named.
    pub values: Vec<T>,
    pub failed: usize,
}

impl<T> Outcome<T> {
    fn new(files: usize) -> Outcome<T> {
        Outcome {
            values: Vec::with_capacity(files),
            failed: 0,
        }
    }

    /// The front of a summary line: `3 file(s)`, or `2 of 3 file(s)` when
    /// some are missing from the figures that follow.
    pub fn files(&self) -> String {
        match self.failed {
            0 => format!("{} file(s)", self.values.len()),
            _ => format!("{} of {} file(s)", self.values.len(), self.total()),
        }
    }

    fn total(&self) -> usize {
        self.values.len() + self.failed
    }

    /// The exit code the run earned, once its summary has been printed.
    pub fn finish(&self) -> Result {
        match (self.failed, self.total()) {
            (0, _) => Ok(()),
            // One file, and it has already said what was wrong with it.
            (_, 1) => Err(Error::Silent),
            (failed, total) => Err(Error::failed(format!("{failed} of {total} file(s) failed"))),
        }
    }
}

/// Runs `work` over every file, collecting what each one returned.
///
/// `prefix` goes in front of a file's heading. It is `//` for the one output
/// that is meant to be read by something other than a person -- the
/// preprocessed source -- so that a run over several files is still a
/// SystemVerilog file. A quiet run has no headings at all, since with nothing
/// under them they would be the whole output.
///
/// A file that fails is reported and the rest are still read, because the
/// usual reason to hand a command a filelist is to find out which files in it
/// have a problem. The exit code says how many did. The one failure that stops
/// everything is the output itself: nobody is reading any more, so there is
/// nothing to be gained by reading on.
///
/// # A file is the unit
///
/// Nothing finer pays -- the largest file in the corpus lexes in under 5 ms --
/// and nothing finer is available anyway: each file gets its own session, and
/// a session's lex cache is `Rc`, so it is built and dropped on the one thread
/// that uses it. What crosses back is bytes and a count.
pub fn each<T: Send>(
    out: &mut Out,
    files: &[impl AsRef<Path> + Sync],
    run: &Run,
    prefix: &str,
    work: impl Fn(&mut dyn Write, &Path) -> Result<T> + Sync,
) -> Result<Outcome<T>> {
    let heading = (!run.quiet).then_some(prefix).filter(|_| files.len() > 1);

    match run.jobs == 1 || files.len() < 2 {
        // Also the path a single file takes, which is the one that wants its
        // output as it is produced: `svirig parse big.sv | head` reads the
        // front of a dump that is never finished.
        true => sequential(out, files, heading, work),
        false => parallel(out, files, heading, work, run.jobs),
    }
}

fn sequential<T>(
    out: &mut Out,
    files: &[impl AsRef<Path>],
    heading: Option<&str>,
    work: impl Fn(&mut dyn Write, &Path) -> Result<T>,
) -> Result<Outcome<T>> {
    let mut outcome = Outcome::new(files.len());

    for (at, file) in files.iter().enumerate() {
        let file = file.as_ref();
        turn(out, heading, at, file, &mut outcome, |out| work(out, file))?;
    }

    Ok(outcome)
}

/// A wave of files at a time, each rendered into a buffer of its own and the
/// buffers written in the order the files were named.
///
/// A wave rather than the whole run because a file's output is held until its
/// turn comes, and the turn is the filelist's order and not the order the
/// threads happened to finish in. Four files per thread is enough for the
/// work-stealing to even out a corpus where one file is three hundred times
/// the size of its neighbour, and short enough that what is held is a wave and
/// not a run. `docs/limitations.md` has what that costs.
fn parallel<T: Send>(
    out: &mut Out,
    files: &[impl AsRef<Path> + Sync],
    heading: Option<&str>,
    work: impl Fn(&mut dyn Write, &Path) -> Result<T> + Sync,
    jobs: usize,
) -> Result<Outcome<T>> {
    // A pool of this run's own rather than the global one, so that how many
    // threads a command line asked for stays a property of the command line.
    let pool = ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .map_err(|err| Error::failed(format!("could not start {jobs} thread(s): {err}")))?;

    let wave = 4 * pool.current_num_threads();
    let mut outcome = Outcome::new(files.len());
    let mut at = 0;

    for wave in files.chunks(wave) {
        let done: Vec<(Result<T>, Vec<u8>)> = pool.install(|| {
            wave.par_iter()
                .map(|file| {
                    let mut printed = Vec::new();
                    let value = work(&mut printed, file.as_ref());
                    (value, printed)
                })
                .collect()
        });

        for (file, (value, printed)) in wave.iter().zip(done) {
            turn(out, heading, at, file.as_ref(), &mut outcome, |out| {
                // Whatever it managed to print goes out even when it failed:
                // a command that reports what is wrong with a file reports it
                // as it reads, and that half is worth having.
                out.write_all(&printed)?;
                value
            })?;
            at += 1;
        }
    }

    Ok(outcome)
}

/// One file's turn at the output: its heading, what it printed, and what to
/// say if it failed.
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
            eprintln!("svirig: {err}");
            outcome.failed += 1;
        }
    }

    Ok(())
}
