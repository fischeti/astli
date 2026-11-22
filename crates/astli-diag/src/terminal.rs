//! Terminal rendering of diagnostics with source snippets and caret annotations.
//!
//! Formats resolved diagnostics using `ariadne` reports. Source spans are interpreted
//! using byte offsets ([`IndexType::Byte`]), and macro expansion traces and include
//! hierarchies are attached as secondary labels and notes.

use std::io;

use anstyle::{Ansi256Color, AnsiColor};
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind};
use astli_text::{Severity, Span};

use crate::resolve::{Place, Resolved};
use crate::sources::Sources;

/// Output styling configuration for diagnostic rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Enables ANSI color escape sequences.
    pub color: bool,
    /// Enables Unicode box-drawing characters rather than ASCII equivalents.
    pub unicode: bool,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            color: true,
            unicode: true,
        }
    }
}

impl Style {
    /// Plain ASCII style without ANSI colors or box-drawing characters.
    pub fn plain() -> Style {
        Style {
            color: false,
            unicode: false,
        }
    }
}

/// The colour a diagnostic of `severity` is drawn in: its header, and the
/// caret at what it reports. These are the colours `ariadne` heads each kind
/// of report with, so that a tool drawing a problem with no source beside
/// these draws it alike.
pub fn colour(severity: Severity) -> anstyle::Color {
    match severity {
        Severity::Error => AnsiColor::Red.into(),
        Severity::Warning => AnsiColor::Yellow.into(),
        Severity::Note => Ansi256Color(147).into(),
    }
}

/// The colour of what explains a diagnostic, such as the text a macro call
/// stands for: quieter than its severity's, so the eye finds the problem
/// first.
pub const EXPLAINING: anstyle::Color = anstyle::Color::Ansi(AnsiColor::Blue);

/// `colour` as `ariadne` takes it, which writes the same escape for it.
fn ariadne(colour: anstyle::Color) -> Color {
    match colour {
        anstyle::Color::Ansi(colour) => match colour {
            AnsiColor::Black => Color::Black,
            AnsiColor::Red => Color::Red,
            AnsiColor::Green => Color::Green,
            AnsiColor::Yellow => Color::Yellow,
            AnsiColor::Blue => Color::Blue,
            AnsiColor::Magenta => Color::Magenta,
            AnsiColor::Cyan => Color::Cyan,
            AnsiColor::White => Color::White,
            AnsiColor::BrightBlack => Color::BrightBlack,
            AnsiColor::BrightRed => Color::BrightRed,
            AnsiColor::BrightGreen => Color::BrightGreen,
            AnsiColor::BrightYellow => Color::BrightYellow,
            AnsiColor::BrightBlue => Color::BrightBlue,
            AnsiColor::BrightMagenta => Color::BrightMagenta,
            AnsiColor::BrightCyan => Color::BrightCyan,
            AnsiColor::BrightWhite => Color::BrightWhite,
        },
        anstyle::Color::Ansi256(Ansi256Color(index)) => Color::Fixed(index),
        anstyle::Color::Rgb(anstyle::RgbColor(r, g, b)) => Color::Rgb(r, g, b),
    }
}

/// Converts a [`Span`] into the `(SourceId, Range<usize>)` tuple expected by `ariadne`.
fn span(at: Span) -> (astli_text::SourceId, std::ops::Range<usize>) {
    (at.src_id, at.start as usize..at.end as usize)
}

/// Renders a resolved diagnostic to the provided output writer.
pub fn write(
    out: &mut dyn io::Write,
    sources: &mut Sources,
    resolved: &Resolved,
    style: Style,
) -> io::Result<()> {
    let diagnostic = resolved.diagnostic;
    let kind = match diagnostic.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Note => ReportKind::Advice,
    };
    let (color, explaining) = (ariadne(colour(diagnostic.severity)), ariadne(EXPLAINING));

    let config = Config::default()
        .with_color(style.color)
        .with_index_type(IndexType::Byte)
        .with_char_set(match style.unicode {
            true => ariadne::CharSet::Unicode,
            false => ariadne::CharSet::Ascii,
        });

    // The problem in the severity's colour, and what explains it in a
    // quieter one, so the eye finds the former first.
    let mut labels = vec![(resolved.at, diagnostic.caret().to_string(), color)];
    if let Some(spelled) = resolved.spelled {
        labels.push((
            spelled,
            "this is the text it stands for".to_string(),
            explaining,
        ));
    }
    for through in resolved
        .through
        .iter()
        .filter(|link| link.call != resolved.at)
    {
        let message = format!("in this expansion of {}", through.name);
        labels.push((through.call, message, explaining));
    }
    for (at, message) in &resolved.labels {
        labels.push((*at, message.to_string(), explaining));
    }

    // ariadne starts a new snippet whenever a label sits above the one before
    // it, and heads every snippet of the reported file with the reported
    // place. Ordering each file's labels by position gives one snippet per
    // file, the reported file first, so no heading names a line its snippet
    // does not start from.
    let mut files = vec![resolved.at.src_id];
    for (at, ..) in &labels {
        if !files.contains(&at.src_id) {
            files.push(at.src_id);
        }
    }
    labels.sort_by_key(|(at, ..)| {
        let file = files.iter().position(|&id| id == at.src_id);
        (file, at.start)
    });

    let mut report = Report::build(kind, span(resolved.at))
        .with_config(config)
        .with_code(diagnostic.code)
        .with_message(&diagnostic.message);
    for (order, (at, message, color)) in (0..).zip(labels) {
        report = report.with_label(
            Label::new(span(at))
                .with_message(message)
                .with_color(color)
                .with_order(order),
        );
    }

    for note in &diagnostic.notes {
        report = report.with_note(note);
    }
    for help in &diagnostic.help {
        report = report.with_help(help);
    }
    for site in &resolved.included_from {
        let place = Place::of(sources.origins(), *site);
        report = report.with_note(format!("included from {}:{}", place.path, place.start));
    }

    report.finish().write(sources, out)
}
