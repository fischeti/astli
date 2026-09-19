//! Something is wrong, and where to say so.
//!
//! # Why the data is here and the rendering is not
//!
//! Every crate that reads source can find something wrong with it, so the type
//! they say it with has to sit below all of them -- which is here, and which is
//! why this module adds no dependency to a crate that has none. Rendering wants
//! the opposite: colour, terminal width, snippet framing, none of which an LSP
//! has any use for. Putting it here would make the preprocessor depend on a
//! terminal in order to report an undefined macro. So `svirig-diag` renders,
//! this describes, and a consumer that only wants a tree links neither.
//!
//! # Where a diagnostic points
//!
//! At a [`TokenOrigin`], not a [`Span`]. A token that came out of a macro has
//! two locations -- the body it is spelled in and the call that placed it --
//! and [`Origins::reported_at`] is what picks the one an author can see. A bare
//! span forces every producer to make that choice itself, and throws away the
//! expansion chain before anything can render it. For a token written where it
//! is used the two coincide, so [`TokenOrigin::written`] covers raw mode and it
//! pays nothing for the distinction.
//!
//! The chain itself is never stored. It is a function of `at` and the store, so
//! a renderer walks [`Origins::trace`] when it needs it; a copy kept here would
//! be one more thing for a producer to forget to fill in. [`Label`] is for the
//! places the producer knows that the chain does not -- where a macro was
//! defined, what was left unclosed.
//!
//! # Why the message is a `String`
//!
//! This crate is a sibling of `svirig-syntax` rather than its parent, so it
//! cannot name a token kind, and a structured "expected `;`" is not available
//! to it. The message therefore arrives rendered. That costs nothing, because
//! the crate that builds it is the one that *can* see the vocabulary: each
//! producer keeps a module of constructors, and formats its own text there.
//!
//! What stays machine-readable is [`Code`], which is what an LSP quotes and
//! what a future `--deny` would name.
//!
//! [`Span`]: crate::Span
//! [`TokenOrigin`]: crate::TokenOrigin
//! [`TokenOrigin::written`]: crate::TokenOrigin::written
//! [`Origins::trace`]: crate::Origins::trace
//! [`Origins::reported_at`]: crate::Origins::reported_at

use std::fmt;

use crate::origins::TokenOrigin;

/// How much a diagnostic matters.
///
/// Declared in increasing order, so that `Ord` means "more severe" and the
/// worst of a run is a `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// How to fix it, hung off something else.
    Help,
    /// Something worth knowing, hung off something else.
    Note,
    /// Accepted, and probably not what was meant.
    Warning,
    /// Wrong. What is produced anyway is a recovery, not an interpretation.
    Error,
}

impl Severity {
    pub fn is_error(self) -> bool {
        self == Severity::Error
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Help => "help",
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        })
    }
}

/// What kind of problem this is, in a form something other than a person can
/// match on.
///
/// A string rather than an enum, because an enum would have to live here and
/// name every problem every crate above this one can have -- the layering
/// inverted for the sake of a constant. Each producer declares its own beside
/// the constructor that uses it, so a code and its wording cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(pub &'static str);

impl Code {
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Somewhere else worth looking, and why.
///
/// Always secondary: what the diagnostic is *about* is
/// [`Diagnostic::at`](Diagnostic#structfield.at), and a label is the
/// supporting cast. There is no style field because that is the whole
/// distinction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub at: TokenOrigin,
    pub message: String,
}

/// One thing that is wrong with the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    /// Rendered by whoever produced it. See the [module docs](self).
    pub message: String,
    /// What this is about, and where a message about it should point.
    pub at: TokenOrigin,
    /// What to say *at* [`at`](Self#structfield.at), as against about the
    /// whole diagnostic: "not defined" under the caret where the message
    /// above reads "`FOO` is not defined".
    ///
    /// `None` where the message is short enough to serve as both, and a
    /// renderer falls back to it. Separate from [`message`](Self#structfield.message)
    /// because a snippet says where by pointing and a message cannot, so the
    /// two want different words for the same fault.
    pub label: Option<String>,
    /// Further places that explain it. Never the expansion chain, which a
    /// renderer derives.
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: Code,
        at: TokenOrigin,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            severity,
            code,
            message: message.into(),
            at,
            label: None,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn error(code: Code, at: TokenOrigin, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Error, code, at, message)
    }

    pub fn warning(code: Code, at: TokenOrigin, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Warning, code, at, message)
    }

    /// Sets what the caret itself says. See
    /// [`label`](Diagnostic#structfield.label).
    pub fn pointing(mut self, label: impl Into<String>) -> Diagnostic {
        self.label = Some(label.into());
        self
    }

    /// What the caret should say, falling back to the message where nothing
    /// shorter was given.
    pub fn caret(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.message)
    }

    /// Adds a place worth looking at, and what it explains.
    pub fn label(mut self, at: TokenOrigin, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            at,
            message: message.into(),
        });
        self
    }

    /// Adds a remark that belongs to the diagnostic rather than to any one
    /// location.
    pub fn note(mut self, note: impl Into<String>) -> Diagnostic {
        self.notes.push(note.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity.is_error()
    }
}
