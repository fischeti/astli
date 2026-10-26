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
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use anstream::{AutoStream, ColorChoice};
use anstyle::{Ansi256Color, AnsiColor};
use astli_diag::Style;
use astli_package::mark::{self, Kind};
use astli_resolve::{Report, Step, canonical};
use astli_text::{Code, Diagnostic, LineCol, Origins, Severity, Span};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::cli::{Color, DiagnosticFormat};
use crate::{json, render};

/// Writes what the run did, unless `-q`: `status!("Patched", "{name}")`.
macro_rules! status {
    ($verb:expr, $($arg:tt)*) => {
        $crate::term::write_status($verb, format_args!($($arg)*))
    };
}
pub(crate) use status;

/// Whether `-q` asked for the results, the errors and the warnings alone.
static QUIET: AtomicBool = AtomicBool::new(false);

/// Whether stderr is one document for a program, which a status line or a
/// colour would break.
static OWNED: AtomicBool = AtomicBool::new(false);

/// Settles colour from `--color`, and whether a status or a note is
/// written from `-q` and the diagnostic format, before anything is written.
pub fn init(color: Color, quiet: bool, format: DiagnosticFormat) {
    let owned = format.owns_stderr();
    OWNED.store(owned, Ordering::Relaxed);
    QUIET.store(quiet || owned, Ordering::Relaxed);
    let choice = match color {
        Color::Auto => ColorChoice::Auto,
        Color::Always => ColorChoice::Always,
        Color::Never => ColorChoice::Never,
    };
    choice.write_global();
}

/// What stderr takes, with `--color`, `NO_COLOR`, `CLICOLOR_FORCE` and
/// whether it is a terminal weighed once. A message in a JSON record is
/// plain, its names in backticks.
static STDERR: LazyLock<ColorChoice> = LazyLock::new(|| match OWNED.load(Ordering::Relaxed) {
    true => ColorChoice::Never,
    false => AutoStream::choice(&io::stderr()),
});

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
/// that a report written from another thread cannot cut into it. A step in
/// progress is hidden while it is written, and drawn again below it.
///
/// A status or a problem that cannot be written is dropped rather than
/// reported: with stderr gone there is nowhere left to say so.
pub fn write(text: &[u8]) -> io::Result<()> {
    let written = || AutoStream::new(io::stderr().lock(), *STDERR).write_all(text);
    match lock(&SPINNERS).is_empty() {
        true => written(),
        false => STEPS.suspend(written),
    }
}

/// The steps in progress, each drawn on its own line of stderr while it
/// runs, below everything written so far.
static STEPS: LazyLock<MultiProgress> = LazyLock::new(MultiProgress::new);

/// The spinner of each step in progress, by the status line it ends as:
/// steps run at once only for different repositories, so no two share one.
static SPINNERS: Mutex<Vec<(String, ProgressBar)>> = Mutex::new(Vec::new());

/// `mutex`, held, whether or not a thread panicked holding it: what it
/// guards is only ever replaced whole.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Shows each step a store takes as the status line it ends as. On a
/// terminal the line is drawn with a spinner left of its verb while the
/// step runs, and how far a transfer has come after it, one line for each
/// step running at once; anywhere else, and in a log, it is written as the
/// step begins, so that what a run is waiting on is among the last lines.
#[derive(Debug)]
pub struct Steps;

impl Report for Steps {
    fn begin(&self, step: &Step<'_>) {
        let (verb, what) = said(step);
        if !io::stderr().is_terminal() || QUIET.load(Ordering::Relaxed) {
            return write_status(verb, format_args!("{what}"));
        }
        // Cyan while the step runs, and green once its status line replaces
        // it. `indicatif` styles the spinner itself, so it is told to only
        // where stderr takes colour.
        let (style, spinner, what) = match *STDERR != ColorChoice::Never {
            true => (
                AnsiColor::Cyan.on_default().bold(),
                "{spinner:.cyan.bold}",
                what,
            ),
            false => {
                let what = anstream::adapter::strip_str(&what).to_string();
                (anstyle::Style::new(), "{spinner}", what)
            }
        };
        // The spinner sits in the verb's margin, so the verb stays in its
        // column; one that fills the column is pushed right of it.
        let margin = " ".repeat(12usize.saturating_sub(verb.len() + 2));
        let template = format!("{margin}{spinner} {{prefix}} {{msg}}");
        let spinner = ProgressBar::new_spinner()
            .with_style(ProgressStyle::with_template(&template).expect("a template"))
            .with_prefix(format!("{style}{verb}{style:#} {what}"));
        let spinner = STEPS.add(spinner);
        spinner.enable_steady_tick(Duration::from_millis(100));
        lock(&SPINNERS).push((key(step), spinner));
    }

    fn advance(&self, step: &Step<'_>, said: &str) {
        let key = key(step);
        if let Some((_, spinner)) = lock(&SPINNERS).iter().find(|(it, _)| *it == key) {
            spinner.set_message(said.to_string());
        }
    }

    fn end(&self, step: &Step<'_>) {
        // Taken out first, so that the status line is not hidden around.
        let key = key(step);
        let spinner = {
            let mut spinners = lock(&SPINNERS);
            let Some(at) = spinners.iter().position(|(it, _)| *it == key) else {
                return;
            };
            spinners.remove(at).1
        };
        spinner.finish_and_clear();
        STEPS.remove(&spinner);
        let (verb, what) = said(step);
        write_status(verb, format_args!("{what}"));
    }
}

/// What tells `step` apart from the others in progress.
fn key(step: &Step<'_>) -> String {
    let (verb, what) = said(step);
    format!("{verb} {what}")
}

/// The verb and the rest of the status line `step` ends as.
fn said(step: &Step<'_>) -> (&'static str, String) {
    match *step {
        Step::Fetching { name, url } => {
            // The URL only says where the dependency comes from, and two
            // packages in one repository share its fetch.
            let dim = anstyle::Style::new().dimmed();
            (
                "Fetching",
                format!("{name} {dim}({}){dim:#}", canonical(url)),
            )
        }
        Step::CheckingOut {
            name,
            version: Some(version),
            ..
        } => ("Checking out", format!("{name} {version}")),
        Step::CheckingOut { name, commit, .. } => (
            "Checking out",
            format!("{name} {}", &commit.to_string()[..12]),
        ),
    }
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
    let what = styled(&what.to_string());
    let _ = write(format!("{style}{verb:>12}{style:#} {what}\n").as_bytes());
}

/// `message` with each [marked](mark) span in its kind's style, or in
/// backticks where stderr takes no colour, which leaves nothing else to
/// set a name apart from the words around it.
fn styled(message: &str) -> String {
    let colour = *STDERR != ColorChoice::Never;
    let plain = anstyle::Style::new();
    mark::pieces(message)
        .map(|(kind, text)| {
            let style = match kind {
                None => return text.to_string(),
                Some(_) if !colour => return format!("`{text}`"),
                Some(Kind::Name) => plain.bold(),
                Some(Kind::Version) => AnsiColor::Cyan.on_default(),
                Some(Kind::Path) => plain.italic(),
                Some(Kind::Code) => AnsiColor::Cyan.on_default().bold(),
            };
            format!("{style}{text}{style:#}")
        })
        .collect()
}

/// An error, a warning or a note, what explains it and what to do about it,
/// and where in a file it is, if anywhere.
#[derive(Debug)]
pub struct Problem {
    severity: Severity,
    pub message: String,
    notes: Vec<String>,
    help: Vec<String>,
    at: Option<At>,
}

/// Where in a file a problem is. The text that held it is gone by the time
/// it is reported, so the file is read again to show it.
#[derive(Debug)]
struct At {
    code: Code,
    path: PathBuf,
    place: Place,
    label: Option<String>,
}

/// A place in a file, in the terms whatever found it knows it by.
#[derive(Debug)]
pub enum Place {
    /// A name of so many characters, starting at the position.
    Name(LineCol, usize),
    /// A whole line, counted from one.
    Line(usize),
    /// A run of bytes.
    Bytes(Range<usize>),
}

impl Place {
    /// The bytes of `text` the place covers, if it is in `text`.
    fn range(&self, text: &str) -> Option<Range<usize>> {
        let line = |number: usize| {
            let start = match number {
                1 => 0,
                _ => text.match_indices('\n').nth(number - 2)?.0 + 1,
            };
            let end = text[start..]
                .find('\n')
                .map_or(text.len(), |end| start + end);
            Some(start..end)
        };
        match self {
            Place::Name(at, chars) => {
                let line = line(at.line as usize)?;
                let mut offsets = (text[line.clone()]
                    .char_indices()
                    .map(|(at, _)| line.start + at))
                .chain([line.end]);
                let start = offsets.nth(at.col as usize - 1)?;
                let end = offsets.nth(chars.saturating_sub(1)).unwrap_or(line.end);
                Some(start..end)
            }
            Place::Line(number) => line(*number),
            Place::Bytes(range) => {
                let fits = range.end <= text.len()
                    && text.is_char_boundary(range.start)
                    && text.is_char_boundary(range.end);
                fits.then(|| range.clone())
            }
        }
    }
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
            at: None,
        }
    }

    /// Places the problem at `place` in the file at `path`, to be shown as a
    /// diagnostic called `code`.
    pub fn at(mut self, code: &'static str, path: &Path, place: Place) -> Problem {
        self.at = Some(At {
            code: Code(code),
            path: path.to_path_buf(),
            place,
            label: None,
        });
        self
    }

    /// Says what is at the place, under it, rather than the message again.
    pub fn label(mut self, text: impl Into<String>) -> Problem {
        if let Some(at) = &mut self.at {
            at.label = Some(text.into());
        }
        self
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
    /// in the colours a diagnostic of its severity takes. One with a place
    /// is a diagnostic, with the file under it, unless the file can no
    /// longer be read.
    pub fn report(&self) {
        if self.severity == Severity::Note && QUIET.load(Ordering::Relaxed) {
            return;
        }
        if let Some(text) = self.at.as_ref().and_then(|at| self.located(at)) {
            let _ = write(&text);
            return;
        }
        if render::FORMAT.get() == Some(&DiagnosticFormat::Json) {
            let line = json::unplaced(self.severity, &styled(&self.message), &self.said());
            let _ = write(format!("{line}\n").as_bytes());
            return;
        }
        let colour = match self.severity {
            Severity::Error => AnsiColor::Red.into(),
            Severity::Warning => AnsiColor::Yellow.into(),
            // The shade ariadne gives the header of advice.
            Severity::Note | Severity::Help => Ansi256Color(147).into(),
        };
        // The message is not bold, as a diagnostic's is not, so that a name
        // in it stands out.
        let label = anstyle::Style::new().fg_color(Some(colour)).bold();
        let bold = anstyle::Style::new().bold();
        let mut text = format!(
            "{label}{}{label:#}: {}\n",
            self.severity,
            styled(&self.message)
        );
        let blue = AnsiColor::Blue.on_default().bold();
        let lines = (self.notes.iter().map(|it| ("note", it)))
            .chain(self.help.iter().map(|it| ("help", it)));
        for (kind, said) in lines {
            let said = styled(said);
            text.push_str(&format!("  {blue}={blue:#} {bold}{kind}:{bold:#} {said}\n"));
        }
        let _ = write(text.as_bytes());
    }
}

impl Problem {
    /// The notes, then the help, styled, as a diagnostic holds them.
    fn said(&self) -> Vec<String> {
        (self.notes.iter().map(|note| styled(note)))
            .chain(
                self.help
                    .iter()
                    .map(|help| format!("help: {}", styled(help))),
            )
            .collect()
    }

    /// The problem rendered as a diagnostic at `at`, if the file still holds
    /// the place.
    fn located(&self, at: &At) -> Option<Vec<u8>> {
        let text = std::fs::read_to_string(&at.path).ok()?;
        let range = at.place.range(&text)?;
        let mut origins = Origins::new();
        let file = origins.add_file(&at.path, text);
        let span = Span::new(file, range.start as u32, range.end as u32);
        let mut diagnostic = Diagnostic::new(self.severity, at.code, span, styled(&self.message));
        diagnostic.label = at.label.as_deref().map(styled);
        diagnostic.notes = self.said();
        let mut out = Vec::new();
        render::diagnostics(&mut out, &origins, &[diagnostic]).ok()?;
        Some(out)
    }
}
