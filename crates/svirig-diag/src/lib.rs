//! Diagnostic resolution and terminal rendering for SystemVerilog tools.
//!
//! [`Diagnostic`](svirig_text::Diagnostic) records error and warning details using
//! abstract token origins ([`TokenOrigin`](svirig_text::TokenOrigin)). This crate
//! maps those token origins to concrete source file spans, resolves macro expansion
//! traces and include hierarchies, and formats the resulting diagnostics for display.
//!
//! ### Modules
//!
//! - [`mod@resolve`]: Maps abstract diagnostic locations to concrete file spans and macro/include traces.
//! - [`sources`]: Implements `ariadne` source caching on top of [`Origins`](svirig_text::Origins).
//! - [`terminal`]: Renders formatted diagnostics with code snippets and labels to terminal output.

pub mod resolve;
pub mod sources;
pub mod terminal;

pub use resolve::{Resolved, Through, resolve, resolve_all};
pub use sources::Sources;
pub use terminal::{Style, write};
