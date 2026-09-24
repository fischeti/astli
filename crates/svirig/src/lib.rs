//! SystemVerilog language tooling, built on one lossless syntax tree.
//!
//! Each module is one of the `svirig-*` crates, re-exported whole, so that one
//! dependency brings the lot at versions that go together. Bottom up:
//!
//! - [`text`]: spans, the file store, reading a file.
//! - [`syntax`]: `SyntaxKind`, the lexer, the tree types and their typed views.
//! - [`preproc`]: directives, macros, includes.
//! - [`parse`]: the grammar, and [`parse::SyntaxTree`] for one file.
//! - [`fmt`]: the formatter.
//! - [`diag`]: rendering a diagnostic, with the chain that explains it.

pub use svirig_diag as diag;
pub use svirig_fmt as fmt;
pub use svirig_parse as parse;
pub use svirig_preproc as preproc;
pub use svirig_syntax as syntax;
pub use svirig_text as text;
