//! `svirig fmt` subcommand.
//!
//! Formats each file on its own. No include path or `+define+` reaches the
//! formatter, so the output is the same wherever it runs, and a filelist only
//! names the files.

use std::io::{self, Write};
use std::path::Path;

use svirig_fmt::format;
use svirig_parse::SyntaxTree;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render;
use crate::sources;

/// The name `-` gives the text it reads, in messages and diff headers.
const STDIN: &str = "<stdin>";

/// Format SystemVerilog source files, printing the result; `-` formats stdin
#[derive(Args)]
pub struct Fmt {
    #[usage(flatten)]
    pub sources: Sources,
    /// Print the files that are not formatted, and fail if there are any, without modifying them
    #[usage(long)]
    pub check: bool,
    /// Print how the files that are not formatted would change, and fail if there are any, without modifying them
    #[usage(long, conflicts("--check", "--write"))]
    pub diff: bool,
    /// Rewrite files in place with formatted output
    #[usage(short = 'w', long, conflicts("--check"))]
    pub write: bool,
}

/// What `fmt` does with each file; the flags that select one conflict.
#[derive(Clone, Copy)]
enum Mode {
    Print,
    Check,
    Diff,
    Write,
}

impl RunWith<Ctx<'_>> for Fmt {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let mode = match (self.check, self.diff, self.write) {
            (true, ..) => Mode::Check,
            (_, true, _) => Mode::Diff,
            (_, _, true) => Mode::Write,
            _ => Mode::Print,
        };
        // A heading separates printed files; the other modes print no file,
        // and a diff names its own.
        let run = RunArgs {
            quiet: run.quiet || !matches!(mode, Mode::Print),
            jobs: run.jobs,
        };

        let stdin = Path::new("-");
        if self.sources.files.iter().any(|path| path == stdin) {
            return from_stdin(out, &self.sources, mode, run.quiet);
        }

        let resolved = sources::resolve(&self.sources, &BuildArgs::default())?;
        let outcome = cmd::each(out, &resolved.files, &run, "", |sink, path| {
            let tree = SyntaxTree::read(path).map_err(|err| Error::io(path, err))?;
            report(sink.out, path, &tree, mode, run.quiet)
        })?;

        outcome.finish()?;
        let changed = outcome.values.iter().filter(|&&changed| changed).count();
        match matches!(mode, Mode::Check | Mode::Diff) && changed > 0 {
            true => Err(Error::failed(format!(
                "{changed} of {} are not formatted",
                outcome.files()
            ))),
            false => Ok(()),
        }
    }
}

/// Formats the text on stdin, which has no file behind it to write back to.
fn from_stdin(out: &mut dyn Write, sources: &Sources, mode: Mode, quiet: bool) -> Result {
    let alone =
        sources.files.len() == 1 && sources.filelist.is_empty() && sources.relative.is_empty();
    if !alone {
        return Err(Error::failed(
            "-: stdin is formatted on its own, without other files",
        ));
    }
    if matches!(mode, Mode::Write) {
        return Err(Error::failed("-: stdin has no file to write back to"));
    }

    let text = io::read_to_string(io::stdin()).map_err(|err| Error::io(STDIN, err))?;
    let tree = SyntaxTree::parse(STDIN, text);
    match report(out, Path::new(STDIN), &tree, mode, quiet)? {
        true if !matches!(mode, Mode::Print) => {
            Err(Error::failed(format!("{STDIN} is not formatted")))
        }
        _ => Ok(()),
    }
}

/// Formats `tree`, read from `path`, and does with it what `mode` asks.
/// Returns whether formatting changed it.
///
/// A refusal writes nothing, so an editor piping a buffer through keeps it.
fn report(
    out: &mut dyn Write,
    path: &Path,
    tree: &SyntaxTree,
    mode: Mode,
    quiet: bool,
) -> Result<bool> {
    let formatted = format(tree).map_err(|refusal| {
        let at = tree.line_col(refusal.offset);
        Error::failed(format!(
            "{}:{at}: {refusal}; left as it is, and this is a formatter bug",
            path.display()
        ))
    })?;

    let changed = formatted != tree.source();
    match mode {
        Mode::Print if !quiet => out.write_all(formatted.as_bytes())?,
        Mode::Check if changed => writeln!(out, "{}", path.display())?,
        Mode::Diff if changed => render::diff(out, path, tree.source(), &formatted)?,
        Mode::Write if changed => {
            std::fs::write(path, &formatted).map_err(|err| Error::io(path, err))?
        }
        _ => {}
    }
    Ok(changed)
}
