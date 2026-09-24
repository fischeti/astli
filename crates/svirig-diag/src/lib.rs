//! Diagnostic resolution and terminal rendering for SystemVerilog tools.
//!
//! [`Diagnostic`](svirig_text::Diagnostic) records error and warning details at
//! spans as placed by macro expansion. This crate maps those spans to where they
//! are written, resolves macro expansion traces and include hierarchies, and
//! formats the resulting diagnostics for display.
//!
//! - [`resolve`] / [`resolve_all`]: placed spans to written ones, with the
//!   macro and include trace that reached them.
//! - [`Sources`]: `ariadne` source caching over [`Origins`](svirig_text::Origins).
//! - [`write()`]: code snippets and labels rendered to a terminal.

mod resolve;
mod sources;
mod terminal;

pub use resolve::{Resolved, Through, resolve, resolve_all};
pub use sources::Sources;
pub use terminal::{Style, write};
