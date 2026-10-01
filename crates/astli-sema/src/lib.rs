//! Semantic analysis of SystemVerilog: what a tree means.
//!
//! [`lower`] turns one file's tree into its [`Hir`]: the scopes it declares
//! names in, the [`Symbol`]s declared there, and the statements and
//! expressions that use them, each addressed by an id and carrying its
//! [`Span`](astli_text::Span). Syntax variants are gone by then: ANSI and
//! non-ANSI ports are one port list, a generate `if` with its `else if`s is
//! one list of arms.
//!
//! ```
//! use astli_parse::parse_expanded;
//! use astli_preproc::Session;
//! use astli_sema::{SymbolKind, lower};
//!
//! let mut session = Session::new();
//! let file = session.add("top.sv", "module top (input a, output b);\n  assign b = a;\nendmodule\n".into());
//! let expanded = session.expand(file);
//! let parsed = parse_expanded(&session, &expanded.tokens);
//!
//! let hir = lower(&parsed);
//! let ports = hir.symbols().filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Port(_)));
//! assert_eq!(ports.count(), 2);
//! ```
//!
//! # Unknown is silent
//!
//! What sema does not model is lowered as [`Opaque`]: a construct the parser
//! kept as written, a class body, a property. It keeps only the names it
//! spells, each of which may be a use of anything by that name, so an
//! analysis concludes nothing from what an opaque region might hide. A false
//! error costs more trust than a missed one.
//!
//! # Threads
//!
//! A [`Hir`] holds no syntax node, so files lower in parallel and their HIRs
//! outlive their trees. Its spans resolve against the session its tree was
//! parsed in.

mod access;
mod check;
mod design;
mod diagnostics;
mod display;
mod hir;
mod lower;
mod resolve;

pub use access::{Access, Driver, accesses};
pub use check::check;
pub use design::{Design, FileId, SymbolRef};
pub use hir::*;
pub use lower::lower;
pub use resolve::{Names, Resolution};
