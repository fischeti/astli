//! Source text management, spans, and token provenance tracking.
//!
//! This crate provides the foundational data structures for tracking source code
//! locations and token origins in SystemVerilog compilation:
//!
//! - [`Span`]: Half-open byte ranges tied to specific [`FileId`]s.
//! - [`Origins`]: Central registry of source buffers, tracking include hierarchies
//!   and macro expansion provenance.
//! - [`TokenOrigin`]: Associates each token with its physical byte location and the
//!   macro expansion chain that introduced it.
//! - [`Diagnostic`]: Structured compiler warnings and errors referencing token
//!   origins, cleanly separated from visual rendering (which is handled by `svirig-diag`).

pub mod diagnostic;
pub mod files;
pub mod origins;
pub mod span;

pub use diagnostic::{Code, Diagnostic, Label, Severity};
pub use files::{Disk, Reader, clean};
pub use origins::{Expansion, ExpansionId, Included, Origins, TokenOrigin};
pub use span::{FileId, LineCol, Span};
