//! Source text, the byte spans that point into it, and where each span came
//! from.
//!
//! Every other `astli` crate talks about locations in these terms:
//!
//! - [`Origins`]: the store of every file and buffer, and the record of how
//!   includes and macro expansions connect them.
//! - [`SourceId`] and [`Span`]: a buffer in the store, and a byte range in it.
//! - [`Diagnostic`]: an error or warning at a span, as data. Rendering it is
//!   `astli-diag`'s job.
//! - [`Reader`]: how a file is read, from [`Disk`] or anywhere else.
//!
//! Most callers get an [`Origins`] from a session in `astli-preproc`, or from
//! a tree in `astli-parse`, rather than filling one themselves.
//!
//! # Files and spans
//!
//! A [`Span`] is a half-open byte range in one buffer. [`Origins`] turns it
//! back into text, and an offset into a 1-based line and column, counted in
//! characters.
//!
//! ```
//! use astli_text::{Origins, Span};
//!
//! let mut origins = Origins::new();
//! let file = origins.add_file("top.sv", "module top;\n  wire é, q;\nendmodule\n".into());
//!
//! // `é` is two bytes and one character.
//! let q = Span::new(file, 23, 24);
//! assert_eq!(origins.slice(q), "q");
//! assert_eq!(origins.line_col(file, q.start).to_string(), "2:11");
//! assert_eq!(origins.path(file).unwrap().to_str(), Some("top.sv"));
//! ```
//!
//! # Where a span came from
//!
//! A [`SourceId`] names a buffer *as placed*: the same bytes seen through a
//! macro expansion get an id of their own, so a span alone says both where
//! its bytes are written and which expansion put them where they are used.
//! The preprocessor records an [`Expansion`] per macro call and places each
//! token [`through`](Origins::through) it; reading back:
//!
//! - [`Origins::spelled`]: the span with its expansion dropped, as written.
//! - [`Origins::trace`]: the expansions that placed it, innermost first.
//! - [`Origins::reported_at`]: the outermost call, which is what a user
//!   wrote and where a diagnostic points.
//!
//! ```
//! use astli_text::{Expansion, Origins, Span};
//!
//! let mut origins = Origins::new();
//! let file = origins.add_file("top.sv", "`define W 8\nlogic [`W:0] q;\n".into());
//! let body = Span::new(file, 10, 11); // `8`
//! let call = Span::new(file, 19, 21); // `` `W ``
//!
//! let expansion = origins.expand(Expansion { name: call, call, def: Some(body) });
//! let placed = origins.through(body, Some(expansion));
//!
//! assert_eq!(origins.slice(placed), "8");
//! assert_ne!(placed, body);
//! assert_eq!(origins.spelled(placed), body);
//! assert_eq!(origins.trace(placed.src_id).count(), 1);
//! assert_eq!(origins.reported_at(placed), call);
//! ```
//!
//! An included file records the directive that loaded it, which
//! [`Origins::include_trace`] walks outward.
//!
//! # Diagnostics
//!
//! A [`Diagnostic`] has a [`Severity`], a [`Code`] naming the kind of problem,
//! a message, and a span, built up with secondary [`Label`]s and notes.
//!
//! ```
//! use astli_text::{Code, Diagnostic, Origins, Span};
//!
//! let mut origins = Origins::new();
//! let file = origins.add_file("top.sv", "wire a;\nwire a;\n".into());
//!
//! let diagnostic = Diagnostic::error(Code("redeclared"), Span::new(file, 13, 14), "`a` is declared twice")
//!     .pointing("declared again here")
//!     .label(Span::new(file, 5, 6), "first declared here")
//!     .note("each name may be declared once per scope");
//!
//! assert!(diagnostic.is_error());
//! assert_eq!(diagnostic.caret(), "declared again here");
//! ```

mod diagnostic;
mod files;
mod origins;
mod span;

pub use diagnostic::{Code, Diagnostic, Label, Severity};
pub use files::{Disk, Reader, clean};
pub use origins::{Expansion, ExpansionId, Included, Origins};
pub use span::{LineCol, SourceId, Span};
