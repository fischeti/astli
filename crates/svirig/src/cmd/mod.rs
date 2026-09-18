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

/// Runs `each` over every file, with a heading when there is more than one.
///
/// `prefix` goes in front of that heading. It is `//` for the one output that
/// is meant to be read by something other than a person -- the preprocessed
/// source -- so that a run over several files is still a SystemVerilog file.
///
/// A file that fails is reported and the rest are still read, because the
/// usual reason to hand a command a filelist is to find out which files in it
/// have a problem. The exit code says how many did. The one failure that stops
/// everything is the output itself: nobody is reading any more, so there is
/// nothing to be gained by reading on.
pub fn each(
    out: &mut Out,
    files: &[impl AsRef<Path>],
    prefix: &str,
    mut each: impl FnMut(&mut Out, &Path) -> Result,
) -> Result {
    let many = files.len() > 1;
    let mut failed = 0;

    for (at, file) in files.iter().enumerate() {
        let file = file.as_ref();
        if many {
            if at > 0 {
                writeln!(out)?;
            }
            writeln!(out, "{prefix}=== {} ===", file.display())?;
        }

        match each(out, file) {
            Ok(()) => {}
            Err(err @ Error::Output(_)) => return Err(err),
            Err(err) => {
                out.flush()?;
                eprintln!("svirig: {err}");
                failed += 1;
            }
        }
    }

    match (failed, files.len()) {
        (0, _) => Ok(()),
        // One file, and it has already said what was wrong with it.
        (_, 1) => Err(Error::Silent),
        (failed, total) => Err(Error::failed(format!("{failed} of {total} file(s) failed"))),
    }
}
