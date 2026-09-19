//! A diagnostic as a person reads it: a snippet, a caret, and the chains.
//!
//! # Byte offsets, said out loud
//!
//! `ariadne` reads a span as *character* offsets by default, and a
//! [`Span`] here is bytes. Handing one to the other without
//! saying so does not merely nudge a caret: the offset is counted from the top
//! of the buffer, so one multi-byte character anywhere above puts every later
//! diagnostic on the wrong line, and a copyright header is enough. So every
//! report is configured [`IndexType::Byte`], which is `ariadne`'s own answer
//! and does the conversion for the column internally.
//!
//! # What is drawn, and what is only mentioned
//!
//! The primary span gets the caret. A macro chain becomes a label per call,
//! because each is a real place in a real file and the point is to show the
//! reader the `` `FOO `` they wrote. An `` `include `` chain becomes notes
//! instead: it explains how the *file* was reached rather than pointing at
//! anything in it, and drawing a caret under an unrelated line of a different
//! file is noise.

use std::io;

use ariadne::{Config, IndexType, Label, Report, ReportKind};
use svirig_text::{Severity, Span};

use crate::resolve::Resolved;
use crate::sources::Sources;

/// How much a terminal will take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Colour. Off when the output is not a terminal, which only the caller
    /// can know.
    pub color: bool,
    /// Box-drawing characters, off for a terminal that cannot show them.
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
    /// Neither colour nor box drawing, for a pipe and for a test whose
    /// expectation is written out by hand.
    pub fn plain() -> Style {
        Style {
            color: false,
            unicode: false,
        }
    }
}

/// `ariadne` addresses a span as a file and a range.
fn span(at: Span) -> (svirig_text::FileId, std::ops::Range<usize>) {
    (at.file, at.start as usize..at.end as usize)
}

/// Writes one diagnostic.
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
        // `ariadne` draws no underline for a label with nothing to say, so
        // the caret always carries text -- the short form where the producer
        // wrote one, and the message where it did not.
        .with_label(Label::new(span(resolved.at)).with_message(diagnostic.caret()));

    // Where the bytes are, when the message is pointing somewhere else. Both
    // are worth showing: one is what the author wrote, the other is what it
    // turned into.
    if let Some(spelled) = resolved.spelled {
        report = report
            .with_label(Label::new(span(spelled)).with_message("this is the text it stands for"));
    }

    // The innermost call is already the primary when there is only one, so a
    // chain is only worth drawing from the second link on.
    for through in resolved
        .through
        .iter()
        .filter(|link| link.call != resolved.at)
    {
        report = report.with_label(
            Label::new(span(through.call))
                // The name arrives as written, backtick included, so nothing
                // quotes it further.
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
        let place = origins.line_col(site.file, site.start);
        report = report.with_note(format!("included from {}:{place}", sources.name(site.file)));
    }

    report.finish().write(sources, out)
}
