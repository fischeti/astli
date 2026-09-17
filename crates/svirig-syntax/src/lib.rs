//! The vocabulary the rest of the tooling is written in: what a token is,
//! what a node can be, and the preprocessor that reads a file as written or
//! as it means.
//!
//! [`SyntaxKind`] covers both halves of a tree, which is what makes
//! [`is_node`](SyntaxKind::is_node) a range check and what keeps the lexer
//! here rather than in a crate of its own -- `logos` derives on that enum.
//! The parser that builds the nodes is `svirig-parse`. See `docs/plan.md`.

pub mod keyword;
pub mod kind;
pub mod lexer;
pub mod preproc;
pub mod tree;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
pub use lexer::{Lexer, Token, tokenize};
pub use tree::{SyntaxElement, SyntaxNode, SyntaxToken, SystemVerilog};
