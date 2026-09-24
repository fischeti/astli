//! Source text management, spans, and token provenance tracking.
//!
//! This crate provides the foundational data structures for tracking source code
//! locations and token origins in SystemVerilog compilation:
//!
//! - [`Span`]: Half-open byte ranges tied to specific [`SourceId`]s. A token's
//!   span also says which macro expansion, if any, placed it.
//! - [`Origins`]: Central registry of source buffers, tracking include hierarchies
//!   and macro expansion provenance.
//! - [`Diagnostic`]: Structured compiler warnings and errors referencing spans,
//!   cleanly separated from visual rendering (which is handled by `astli-diag`).

mod diagnostic;
mod files;
mod origins;
mod span;

pub use diagnostic::{Code, Diagnostic, Label, Severity};
pub use files::{Disk, Reader, clean};
pub use origins::{Expansion, ExpansionId, Included, Origins};
pub use span::{LineCol, SourceId, Span};
