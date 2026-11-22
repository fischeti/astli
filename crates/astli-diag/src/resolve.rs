//! Resolution of diagnostic spans into concrete file locations and expansion traces.
//!
//! Diagnostics emitted during parsing or semantic analysis reference spans as
//! placed by macro expansion. This module maps those spans against the source
//! [`Origins`] database to determine:
//! - The reported primary span (pointing to macro invocation sites when applicable).
//! - The original definition span for tokens generated inside macro definitions.
//! - The chain of macro expansion invocations that produced the token.
//! - The file inclusion trace leading to the reported file.

use astli_text::{Diagnostic, LineCol, Origins, SourceId, Span};

/// Represents an intermediate macro invocation site along an expansion chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Through {
    /// Source span of the macro invocation, including arguments (e.g. `` `FOO(a, b) ``).
    pub call: Span,
    /// Spelled name of the macro as written in the source, including the leading backtick.
    pub name: String,
}

/// A diagnostic with all spans resolved to where they are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved<'a> {
    /// Underlying diagnostic definition.
    pub diagnostic: &'a Diagnostic,
    /// Primary source span targeted by the diagnostic message.
    pub at: Span,
    /// The file and the lines and columns [`at`](Self::at) covers, which
    /// every format that names the place writes.
    pub place: Place,
    /// Source span where the token was originally written, if different from [`at`](Self::at).
    pub spelled: Option<Span>,
    /// Intermediate macro expansion calls leading to the token, ordered innermost first.
    pub through: Vec<Through>,
    /// File inclusion chain leading to [`at`](Self::at), ordered innermost first.
    pub included_from: Vec<Span>,
    /// Additional labeled spans associated with the diagnostic.
    pub labels: Vec<(Span, &'a str)>,
}

/// Where a span is, as a reader names it: the file's path, and the line and
/// column it starts and ends at, each counted from one and the column in
/// characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The path the file was read from or named by, or `<expansion>` for
    /// text no file holds.
    pub path: String,
    /// Where the span starts.
    pub start: LineCol,
    /// Where the span ends, just past its last character.
    pub end: LineCol,
}

impl Place {
    /// Where `at` is in `origins`.
    pub fn of(origins: &Origins, at: Span) -> Place {
        Place {
            path: name(origins, at.src_id),
            start: origins.line_col(at.src_id, at.start),
            end: origins.line_col(at.src_id, at.end),
        }
    }
}

/// The name of `file` in a place: its path, or `<expansion>` without one.
pub(crate) fn name(origins: &Origins, file: SourceId) -> String {
    match origins.path(file) {
        Some(path) => path.display().to_string(),
        None => "<expansion>".to_string(),
    }
}

impl Resolved<'_> {
    /// Returns the sorting key for ordering diagnostics by source location.
    fn key(&self) -> (usize, u32) {
        (self.at.src_id.index(), self.at.start)
    }
}

/// Resolves a single diagnostic against the provided source [`Origins`] database.
fn resolve<'a>(origins: &Origins, diagnostic: &'a Diagnostic) -> Resolved<'a> {
    let at = origins.reported_at(diagnostic.at);
    Resolved {
        diagnostic,
        at,
        place: Place::of(origins, at),
        spelled: Some(origins.spelled(diagnostic.at)).filter(|&spelled| spelled != at),
        through: chain(origins, diagnostic.at.src_id),
        included_from: origins.include_trace(at.src_id).collect(),
        labels: diagnostic
            .labels
            .iter()
            .map(|label| (origins.reported_at(label.at), label.message.as_str()))
            .collect(),
    }
}

/// Resolves a slice of diagnostics, sorting by source order and removing duplicate reports.
pub fn resolve_all<'a>(origins: &Origins, diagnostics: &'a [Diagnostic]) -> Vec<Resolved<'a>> {
    let mut resolved: Vec<Resolved<'a>> = diagnostics
        .iter()
        .map(|diagnostic| resolve(origins, diagnostic))
        .collect();
    resolved.sort_by_key(Resolved::key);
    resolved.dedup_by(|a, b| {
        a.at == b.at
            && a.diagnostic.code == b.diagnostic.code
            && a.diagnostic.message == b.diagnostic.message
    });
    resolved
}

/// Reconstructs the chain of macro expansion calls that placed `file`, ordered innermost first.
fn chain(origins: &Origins, file: SourceId) -> Vec<Through> {
    origins
        .trace(file)
        .map(|expansion| Through {
            call: origins.spelled(expansion.call),
            name: origins.slice(expansion.name).to_string(),
        })
        .collect()
}
