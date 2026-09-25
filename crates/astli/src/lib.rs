//! SystemVerilog language tooling, built on one lossless syntax tree.
//!
//! Each module is one of the `astli-*` crates, re-exported whole, so that one
//! dependency brings the lot at versions that go together. Each has a guide
//! of its own in its crate docs.
//!
//! # A tour
//!
//! Parse a file, walk the tree, report what is wrong with it, and format it:
//!
//! ```
//! use astli::diag::{Sources, Style, resolve_all, write};
//! use astli::parse::SyntaxTree;
//! use astli::syntax::ast::{AstNode, ModuleDecl};
//!
//! let source = "module top(input logic clk);\nendmodule\nmodule sub;\n";
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//! // Or `SyntaxTree::read("top.sv")?`.
//!
//! // The typed views name each construct's parts.
//! let names: Vec<_> = tree
//!     .root()
//!     .descendants()
//!     .filter_map(ModuleDecl::cast)
//!     .filter_map(|module| module.name())
//!     .map(|name| name.text().to_string())
//!     .collect();
//! assert_eq!(names, ["top"]);
//!
//! // `sub` is never closed: the parser says so, and keeps its text.
//! assert_eq!(tree.diagnostics().len(), 1);
//! let mut sources = Sources::new(tree.origins());
//! for diagnostic in resolve_all(tree.origins(), tree.diagnostics()) {
//!     write(&mut std::io::stderr(), &mut sources, &diagnostic, Style::plain()).unwrap();
//! }
//! assert_eq!(tree.root().text(), source);
//!
//! let formatted = astli::fmt::format(&tree).unwrap();
//! assert!(formatted.starts_with("module top (\n  input logic clk\n);\n"));
//! ```
//!
//! # Which module
//!
//! Bottom up, each built on the ones before it:
//!
//! | Module | For |
//! | --- | --- |
//! | [`text`] | Spans, the file store, and where a span came from through macros and includes. |
//! | [`diag`] | Rendering a diagnostic, with the macro calls and includes that explain it. |
//! | [`syntax`] | [`SyntaxKind`](syntax::SyntaxKind), the lexer, the tree types, and the typed views in [`syntax::ast`]. |
//! | [`preproc`] | Directives, macros, includes: expanding a file as a compiler would, or finding them in it as written. |
//! | [`parse`] | The grammar, and [`parse::SyntaxTree`] for one file. |
//! | [`fmt`] | The formatter. |
//!
//! Most tools start at [`parse::SyntaxTree`]. One that needs a file expanded,
//! a build's include directories and `+define+`s, or several files in one
//! store starts at [`preproc::Session`] instead, and parses against it with
//! [`parse::parse()`].
//!
//! # Scope
//!
//! `astli` lexes, preprocesses and parses; it does not elaborate or
//! type-check. The tree is of the file as written: macro calls, directives
//! and every branch of an `` `ifdef `` are in it, and nothing is substituted.
//! A construct the grammar does not cover yet is kept as its tokens, so
//! nothing is ever lost.

pub use astli_diag as diag;
pub use astli_fmt as fmt;
pub use astli_parse as parse;
pub use astli_preproc as preproc;
pub use astli_syntax as syntax;
pub use astli_text as text;
