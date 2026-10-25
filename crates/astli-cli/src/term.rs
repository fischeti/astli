//! What the driver writes to stderr besides its results, and whether each
//! stream takes colour.
//!
//! Every message goes through here rather than `eprintln!`, so that colour is
//! decided in one place, and a message may be styled without asking whether
//! stderr takes it: what it cannot take is stripped on the way out.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use anstream::{AutoStream, ColorChoice};
use anstyle::AnsiColor;
use astli_diag::Style;

use crate::cli::Color;

/// Writes a line to stderr, as `eprintln!` does.
macro_rules! say {
    ($($arg:tt)*) => {
        $crate::term::line(format_args!($($arg)*))
    };
}
pub(crate) use say;

/// Writes what the run did, unless `-q`: `status!("Patched", "{name}")`.
macro_rules! status {
    ($verb:expr, $($arg:tt)*) => {
        $crate::term::write_status($verb, format_args!($($arg)*))
    };
}
pub(crate) use status;

/// Whether `-q` asked for the results and the problems alone.
static QUIET: AtomicBool = AtomicBool::new(false);

/// Settles colour from `--color`, and whether a status is written from
/// `-q`, before anything is written.
pub fn init(color: Color, quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
    let choice = match color {
        Color::Auto => ColorChoice::Auto,
        Color::Always => ColorChoice::Always,
        Color::Never => ColorChoice::Never,
    };
    choice.write_global();
}

/// What stderr takes, with `--color`, `NO_COLOR`, `CLICOLOR_FORCE` and
/// whether it is a terminal weighed once.
static STDERR: LazyLock<ColorChoice> = LazyLock::new(|| AutoStream::choice(&io::stderr()));

/// Whether stdout takes colour. Piped, a diff stays a patch that `git apply`
/// or a pager like `delta` can read.
///
/// Asked rather than stripped as stderr is, since a run can write a whole
/// design to stdout and nearly all of it is never coloured.
pub fn stdout_colour() -> bool {
    static STDOUT: LazyLock<bool> =
        LazyLock::new(|| AutoStream::choice(&io::stdout()) != ColorChoice::Never);
    *STDOUT
}

/// How a diagnostic is drawn on stderr. Box drawing stays a terminal's, as
/// a log may be read where it does not render.
pub fn style() -> Style {
    Style {
        color: *STDERR != ColorChoice::Never,
        unicode: io::stderr().is_terminal(),
    }
}

/// Writes `text`, which may be several lines, to stderr in one piece, so
/// that a report written from another thread cannot cut into it.
pub fn write(text: &[u8]) -> io::Result<()> {
    AutoStream::new(io::stderr().lock(), *STDERR).write_all(text)
}

/// Writes a line to stderr; [`say!`] is the way to call it.
///
/// A message that cannot be written is dropped: with stderr gone there is
/// nowhere left to say so.
pub fn line(line: fmt::Arguments<'_>) {
    let _ = writeln!(AutoStream::new(io::stderr().lock(), *STDERR), "{line}");
}

/// Writes a status line; [`status!`] is the way to call it.
///
/// The verb is right-aligned in twelve columns, so that what each line acts
/// on starts in one column and a long run reads down it.
pub fn write_status(verb: &str, what: fmt::Arguments<'_>) {
    if QUIET.load(Ordering::Relaxed) {
        return;
    }
    let style = AnsiColor::Green.on_default().bold();
    line(format_args!("{style}{verb:>12}{style:#} {what}"));
}
