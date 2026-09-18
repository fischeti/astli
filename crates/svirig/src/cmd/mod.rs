//! One module per subcommand. Each takes the parsed arguments and does the
//! work; none of them mentions the argument parser.

pub mod completion;
pub mod fmt;
pub mod lex;
pub mod parse;
pub mod preprocess;

use std::io::Write;
use std::path::Path;

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

/// Runs `each` over every file, collecting what each one returned.
///
/// `heading` is the prefix that goes in front of a file's heading, and `None`
/// is no headings at all -- which is what a run that prints nothing per file
/// wants, since the headings would then be the whole output. The prefix is
/// `//` for the one output that is meant to be read by something other than a
/// person -- the preprocessed source -- so that a run over several files is
/// still a SystemVerilog file.
///
/// A file that fails is reported and the rest are still read, because the
/// usual reason to hand a command a filelist is to find out which files in it
/// have a problem. The exit code says how many did. The one failure that stops
/// everything is the output itself: nobody is reading any more, so there is
/// nothing to be gained by reading on.
pub fn each<T>(
    out: &mut Out,
    files: &[impl AsRef<Path>],
    heading: Option<&str>,
    mut each: impl FnMut(&mut dyn Write, &Path) -> Result<T>,
) -> Result<Outcome<T>> {
    let heading = heading.filter(|_| files.len() > 1);
    let mut values = Vec::with_capacity(files.len());
    let mut failed = 0;

    for (at, file) in files.iter().enumerate() {
        let file = file.as_ref();
        if let Some(prefix) = heading {
            if at > 0 {
                writeln!(out)?;
            }
            writeln!(out, "{prefix}=== {} ===", file.display())?;
        }

        match each(out, file) {
            Ok(value) => values.push(value),
            Err(err @ Error::Output(_)) => return Err(err),
            Err(err) => {
                out.flush()?;
                eprintln!("svirig: {err}");
                failed += 1;
            }
        }
    }

    Ok(Outcome { values, failed })
}
