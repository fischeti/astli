//! Files, spans, and where an expanded token came from.
//!
//! Everything downstream needs to answer two questions about a token: *what
//! bytes is it* and *where should a message about it point*. For source read
//! straight from a file those are the same question. Once a macro expands they
//! stop being: a token can be written inside a `` `define `` in one file,
//! placed by a call in another, and made of bytes that are in no file at all.
//!
//! # Why this exists before expansion does
//!
//! Retrofitting provenance means touching everything that already consumes
//! tokens, so the map comes first and expansion is written against it. It is
//! also what joins the two output modes: an editor holds the raw tree of the
//! buffer it is showing and hangs analysis of the expanded program off it, and
//! the join runs through here.
//!
//! # The model
//!
//! A [`Span`] is a byte range in one [`FileId`]. A file is a real file or a
//! buffer that expansion synthesised, and an [`Origins`] holds them all. A
//! [`TokenOrigin`] pairs the span a token's bytes live at with the [`Expansion`]
//! that placed it, if one did; expansions chain through their parent, so a
//! macro that expands to a macro reads back as a chain of calls.
//!
//! The provenance is recorded **per token**, not per byte. That is the one
//! place this departs from how a preprocessor that re-emits *text* has to work,
//! and it is what makes a macro argument ordinary rather than a special case.
//! See [`TokenOrigin`].
//!
//! Diagnostic rendering is not here yet. [`Origins::trace`] and
//! [`Origins::reported_at`] carry everything a renderer needs; what to do with
//! it waits for there being a diagnostics layer to do it in.

pub mod origins;
pub mod span;

pub use origins::{Expansion, ExpansionId, Origins, TokenOrigin};
pub use span::{FileId, LineCol, Span};
