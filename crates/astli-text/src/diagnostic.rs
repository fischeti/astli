//! Compiler diagnostics data structures and error codes.
//!
//! This module defines the data model for compiler diagnostics (errors, warnings,
//! and notes). Diagnostics reference source locations via [`Span`], allowing
//! downstream renderers (such as `astli-diag`) to display both the physical source
//! code and any macro expansion chains involved.

use std::fmt;

use crate::span::Span;

/// Severity level of a compiler diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Suggested fix or remedial guidance.
    Help,
    /// Informational note providing additional context.
    Note,
    /// Warning indicating potentially unintended or deprecated syntax.
    Warning,
    /// Fatal or syntax error preventing valid compilation.
    Error,
}

impl Severity {
    /// Returns `true` if this severity represents an error.
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

/// Machine-readable diagnostic error or warning identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(pub &'static str);

impl Code {
    /// Returns the string representation of this diagnostic code.
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Secondary source location annotation with an explanatory message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub at: Span,
    pub message: String,
}

/// Structured compiler diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    /// Primary human-readable diagnostic message.
    pub message: String,
    /// Primary source location for this diagnostic.
    pub at: Span,
    /// Optional short text displayed directly at the primary source caret.
    pub label: Option<String>,
    /// Secondary source annotations providing supporting context.
    pub labels: Vec<Label>,
    /// Additional informational notes appended to the diagnostic.
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Constructs a new diagnostic with the given severity, code, location, and message.
    pub fn new(severity: Severity, code: Code, at: Span, message: impl Into<String>) -> Diagnostic {
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

    /// Constructs an error-level diagnostic.
    pub fn error(code: Code, at: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Error, code, at, message)
    }

    /// Constructs a warning-level diagnostic.
    pub fn warning(code: Code, at: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Warning, code, at, message)
    }

    /// Sets the short label text displayed directly at the primary source caret.
    pub fn pointing(mut self, label: impl Into<String>) -> Diagnostic {
        self.label = Some(label.into());
        self
    }

    /// Returns the caret label, falling back to the main message if no specific label was set.
    pub fn caret(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.message)
    }

    /// Adds a secondary source location annotation with an explanatory message.
    pub fn label(mut self, at: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            at,
            message: message.into(),
        });
        self
    }

    /// Appends an informational note to the diagnostic.
    pub fn note(mut self, note: impl Into<String>) -> Diagnostic {
        self.notes.push(note.into());
        self
    }

    /// Returns `true` if this diagnostic has error severity.
    pub fn is_error(&self) -> bool {
        self.severity.is_error()
    }
}
