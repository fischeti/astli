//! What the driver writes to stderr besides its results, and whether each
//! stream takes colour.
//!
//! Every message goes through here rather than `eprintln!`: a [`status!`]
//! for what the run did, a [`Problem`] for what went wrong, and [`write`]
//! for a file's diagnostics. Colour is decided in one place, and a message
//! may be styled without asking whether stderr takes it: what it cannot
//! take is stripped on the way out.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use anstream::{AutoStream, ColorChoice};
use anstyle::{Ansi256Color, AnsiColor};
use astli_diag::Style;
use astli_text::Severity;

use crate::cli::Color;

/// Writes what the run did, unless `-q`: `status!("Patched", "{name}")`.
macro_rules! status {
    ($verb:expr, $($arg:tt)*) => {
        $crate::term::write_status($verb, format_args!($($arg)*))
    };
}
pub(crate) use status;

/// Whether `-q` asked for the results, the errors and the warnings alone.
static QUIET: AtomicBool = AtomicBool::new(false);

/// Settles colour from `--color`, and whether a status or a note is
/// written from `-q`, before anything is written.
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
///
/// A status or a problem that cannot be written is dropped rather than
/// reported: with stderr gone there is nowhere left to say so.
pub fn write(text: &[u8]) -> io::Result<()> {
    AutoStream::new(io::stderr().lock(), *STDERR).write_all(text)
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
    let _ = write(format!("{style}{verb:>12}{style:#} {what}\n").as_bytes());
}

/// An error, a warning or a note with no place in a file to show, and what
/// explains it or what to do about it.
#[derive(Debug)]
pub struct Problem {
    severity: Severity,
    pub message: String,
    notes: Vec<String>,
    help: Vec<String>,
}

/// A problem that stops the run, or that the run reports as it goes on.
pub fn error(message: impl Into<String>) -> Problem {
    Problem::new(Severity::Error, message.into())
}

/// Something the run went on despite, which is likely a mistake.
pub fn warning(message: impl Into<String>) -> Problem {
    Problem::new(Severity::Warning, message.into())
}

/// Something worth knowing about the run, left out under `-q`.
pub fn note(message: impl Into<String>) -> Problem {
    Problem::new(Severity::Note, message.into())
}

impl Problem {
    fn new(severity: Severity, message: String) -> Problem {
        Problem {
            severity,
            message,
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    /// Adds what explains the problem.
    pub fn note(mut self, text: impl Into<String>) -> Problem {
        self.notes.push(text.into());
        self
    }

    /// Adds what to do about the problem.
    pub fn help(mut self, text: impl Into<String>) -> Problem {
        self.help.push(text.into());
        self
    }

    /// Writes the problem to stderr, in one piece, as
    ///
    /// ```text
    /// error: astli.lock would change, and `--locked` keeps it as it is
    ///   = help: `astli update` writes it
    /// ```
    ///
    /// in the colours a diagnostic of its severity takes.
    pub fn report(&self) {
        if self.severity == Severity::Note && QUIET.load(Ordering::Relaxed) {
            return;
        }
        let colour = match self.severity {
            Severity::Error => AnsiColor::Red.into(),
            Severity::Warning => AnsiColor::Yellow.into(),
            // The shade ariadne gives the header of advice.
            Severity::Note | Severity::Help => Ansi256Color(147).into(),
        };
        let label = anstyle::Style::new().fg_color(Some(colour)).bold();
        let bold = anstyle::Style::new().bold();
        let mut text = format!(
            "{label}{}{label:#}{bold}: {}{bold:#}\n",
            self.severity, self.message
        );
        let blue = AnsiColor::Blue.on_default().bold();
        let lines = (self.notes.iter().map(|it| ("note", it)))
            .chain(self.help.iter().map(|it| ("help", it)));
        for (kind, said) in lines {
            text.push_str(&format!("  {blue}={blue:#} {bold}{kind}:{bold:#} {said}\n"));
        }
        let _ = write(text.as_bytes());
    }
}
