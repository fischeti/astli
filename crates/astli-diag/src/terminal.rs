//! Terminal rendering of diagnostics with source snippets and caret annotations.
//!
//! Formats resolved diagnostics using `ariadne` reports. Source spans are interpreted
//! using byte offsets ([`IndexType::Byte`]), and macro expansion traces and include
//! hierarchies are attached as secondary labels and notes.

use std::io;

use ariadne::{Color, Config, IndexType, Label, Report, ReportKind};
use astli_text::{Severity, Span};

use crate::resolve::Resolved;
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
    let (kind, color) = match diagnostic.severity {
        Severity::Error => (ReportKind::Error, Color::Red),
        Severity::Warning => (ReportKind::Warning, Color::Yellow),
        // The shade ariadne gives the header of advice.
        Severity::Note | Severity::Help => (ReportKind::Advice, Color::Fixed(147)),
    };

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
            Color::Blue,
        ));
    }
    for through in resolved
        .through
        .iter()
        .filter(|link| link.call != resolved.at)
    {
        let message = format!("in this expansion of {}", through.name);
        labels.push((through.call, message, Color::Blue));
    }
    for (at, message) in &resolved.labels {
        labels.push((*at, message.to_string(), Color::Blue));
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
        let origins = sources.origins();
        let place = origins.line_col(site.src_id, site.start);
        report = report.with_note(format!(
            "included from {}:{place}",
            sources.name(site.src_id)
        ));
    }

    report.finish().write(sources, out)
}

/// Writes a resolved diagnostic as one line, `file:line:col: severity[code]:
/// message`, the shape editors and CI log scanners look for.
///
/// The place is where the user wrote the text, as in [`write()`]. The source,
/// the macro chain, and the notes are left out: a line holds one location.
pub fn write_short(
    out: &mut dyn io::Write,
    sources: &Sources,
    resolved: &Resolved,
) -> io::Result<()> {
    let diagnostic = resolved.diagnostic;
    let at = resolved.at;
    let place = sources.origins().line_col(at.src_id, at.start);
    writeln!(
        out,
        "{}:{place}: {}[{}]: {}",
        sources.name(at.src_id),
        diagnostic.severity,
        diagnostic.code,
        diagnostic.message
    )
}

/// Writes a resolved diagnostic as a GitHub Actions workflow command, which
/// the runner turns into an annotation on the line of the pull request's diff.
///
/// The command's message is the line [`write_short`] writes, since the log
/// shows the message alone, and the place in it is what a reader of the log
/// looks for.
pub fn write_github(
    out: &mut dyn io::Write,
    sources: &Sources,
    resolved: &Resolved,
) -> io::Result<()> {
    let diagnostic = resolved.diagnostic;
    let at = resolved.at;
    let origins = sources.origins();
    let (start, end) = (
        origins.line_col(at.src_id, at.start),
        origins.line_col(at.src_id, at.end),
    );
    let command = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note | Severity::Help => "notice",
    };
    let file = sources.name(at.src_id);
    let message = format!(
        "{file}:{start}: {}[{}]: {}",
        diagnostic.severity, diagnostic.code, diagnostic.message
    );
    writeln!(
        out,
        "::{command} title={},file={},line={},col={},endLine={},endColumn={}::{}",
        property(&diagnostic.code.to_string()),
        property(&file),
        start.line,
        start.col,
        end.line,
        end.col,
        data(&message)
    )
}

/// Escapes a workflow command's message, which ends at the line.
fn data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a workflow command's property, which also ends at a comma, and its
/// list at a colon.
fn property(text: &str) -> String {
    data(text).replace(':', "%3A").replace(',', "%2C")
}
