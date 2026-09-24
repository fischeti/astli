//! Terminal rendering of diagnostics with source snippets and caret annotations.
//!
//! Formats resolved diagnostics using `ariadne` reports. Source spans are interpreted
//! using byte offsets ([`IndexType::Byte`]), and macro expansion traces and include
//! hierarchies are attached as secondary labels and notes.

use std::io;

use ariadne::{Config, IndexType, Label, Report, ReportKind};
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
    let kind = match diagnostic.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Note | Severity::Help => ReportKind::Advice,
    };

    let config = Config::default()
        .with_color(style.color)
        .with_index_type(IndexType::Byte)
        .with_char_set(match style.unicode {
            true => ariadne::CharSet::Unicode,
            false => ariadne::CharSet::Ascii,
        });

    let mut report = Report::build(kind, span(resolved.at))
        .with_config(config)
        .with_code(diagnostic.code)
        .with_message(&diagnostic.message)
        .with_label(Label::new(span(resolved.at)).with_message(diagnostic.caret()));

    if let Some(spelled) = resolved.spelled {
        report = report
            .with_label(Label::new(span(spelled)).with_message("this is the text it stands for"));
    }

    for through in resolved
        .through
        .iter()
        .filter(|link| link.call != resolved.at)
    {
        report = report.with_label(
            Label::new(span(through.call))
                .with_message(format!("in this expansion of {}", through.name)),
        );
    }

    for (at, message) in &resolved.labels {
        report = report.with_label(Label::new(span(*at)).with_message(message));
    }

    for note in &diagnostic.notes {
        report = report.with_note(note);
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
