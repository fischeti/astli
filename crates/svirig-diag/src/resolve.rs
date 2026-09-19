//! Turning what a diagnostic points at into places in the store.
//!
//! A [`Diagnostic`] carries a [`TokenOrigin`] and nothing else: it is built by
//! a crate that knows what went wrong and not where anything is. Everything
//! positional is derived here, against the [`Origins`] the ids came from.
//!
//! # Why this is not in the renderer
//!
//! None of it is about a terminal. Which of a token's two locations a message
//! belongs at, what order complaints should be read in, how a chain of macro
//! calls flattens -- an editor wants all of that and wants none of the colour
//! and framing that goes with it. So the renderer takes a [`Resolved`], and a
//! second backend that speaks a protocol rather than a terminal starts here.

use svirig_text::{Diagnostic, Origins, Span, TokenOrigin};

/// One macro call a token arrived through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Through {
    /// The call as written, `` `FOO(a, b) ``.
    pub call: Span,
    /// The macro's name as written, backtick included -- what the author will
    /// be looking for in the file.
    pub name: String,
}

/// A diagnostic with every location it names turned into a place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved<'a> {
    pub diagnostic: &'a Diagnostic,
    /// Where the message points. For a token a macro placed this is the
    /// outermost call -- what the author wrote -- and not the body text they
    /// probably never saw.
    pub at: Span,
    /// Where the bytes are, when that is somewhere other than [`at`](Self::at).
    /// `None` for a token written where it is used, which is most of them.
    pub spelled: Option<Span>,
    /// The macro calls between the two, innermost first. Empty unless a macro
    /// expanded to a macro.
    pub through: Vec<Through>,
    /// The `` `include ``s above [`at`](Self::at), innermost first.
    pub included_from: Vec<Span>,
    /// The producer's own labels, resolved the same way as the primary.
    pub labels: Vec<(Span, &'a str)>,
}

impl Resolved<'_> {
    /// Where this sorts: the file it is reported in, then the byte it starts
    /// at.
    fn key(&self) -> (usize, u32) {
        (self.at.file.index(), self.at.start)
    }
}

/// Resolves one diagnostic against the store its ids came from.
pub fn resolve<'a>(origins: &Origins, diagnostic: &'a Diagnostic) -> Resolved<'a> {
    let at = origins.reported_at(diagnostic.at);
    Resolved {
        diagnostic,
        at,
        // Worth saying only when it is somewhere else: for a token written
        // where it is used the two are the same span.
        spelled: (diagnostic.at.spelled != at).then_some(diagnostic.at.spelled),
        through: chain(origins, diagnostic.at),
        included_from: origins.include_trace(at.file).collect(),
        labels: diagnostic
            .labels
            .iter()
            .map(|label| (origins.reported_at(label.at), label.message.as_str()))
            .collect(),
    }
}

/// Resolves a run, in the order it should be read, with exact repeats dropped.
///
/// Sorted by place rather than by when it was found, because expansion reaches
/// an `` `include `` in the middle of a file and comes back: the order things
/// go wrong in is not the order anyone reads them in.
pub fn resolve_all<'a>(origins: &Origins, diagnostics: &'a [Diagnostic]) -> Vec<Resolved<'a>> {
    let mut resolved: Vec<Resolved<'a>> = diagnostics
        .iter()
        .map(|diagnostic| resolve(origins, diagnostic))
        .collect();
    resolved.sort_by_key(Resolved::key);
    // One macro used twice says it twice, because the two uses are two places.
    // The same complaint about the same place is one.
    resolved.dedup_by(|a, b| {
        a.at == b.at
            && a.diagnostic.code == b.diagnostic.code
            && a.diagnostic.message == b.diagnostic.message
    });
    resolved
}

/// The macro calls a token came through, innermost first, with the name each
/// one used.
fn chain(origins: &Origins, origin: TokenOrigin) -> Vec<Through> {
    origins
        .trace(origin)
        .map(|expansion| Through {
            call: expansion.call,
            name: origins.slice(expansion.name).to_string(),
        })
        .collect()
}
