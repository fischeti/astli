//! SystemVerilog language tooling, built on one lossless syntax tree.
//!
//! Each module is one of the `astli-*` crates, re-exported whole, so that one
//! dependency brings the lot at versions that go together. Bottom up:
//!
//! - [`text`]: spans, the file store, reading a file.
//! - [`syntax`]: `SyntaxKind`, the lexer, the tree types and their typed views.
//! - [`preproc`]: directives, macros, includes.
//! - [`parse`]: the grammar, and [`parse::SyntaxTree`] for one file.
//! - [`fmt`]: the formatter.
//! - [`diag`]: rendering a diagnostic, with the chain that explains it.

pub use astli_diag as diag;
pub use astli_fmt as fmt;
pub use astli_parse as parse;
pub use astli_preproc as preproc;
pub use astli_syntax as syntax;
pub use astli_text as text;
