//! Diagnostic resolution and terminal rendering for SystemVerilog tools.
//!
//! [`Diagnostic`](svirig_text::Diagnostic) records error and warning details using
//! abstract token origins ([`TokenOrigin`](svirig_text::TokenOrigin)). This crate
//! maps those token origins to concrete source file spans, resolves macro expansion
//! traces and include hierarchies, and formats the resulting diagnostics for display.
//!
//! - [`resolve`] / [`resolve_all`]: token origins to concrete file spans, with the
//!   macro and include trace that reached them.
//! - [`Sources`]: `ariadne` source caching over [`Origins`](svirig_text::Origins).
//! - [`write()`]: code snippets and labels rendered to a terminal.

mod resolve;
mod sources;
mod terminal;

pub use resolve::{Resolved, Through, resolve, resolve_all};
pub use sources::Sources;
pub use terminal::{Style, write};
