//! Lexing, preprocessing, and parsing of SystemVerilog into a lossless
//! syntax tree.
//!
//! The lexer and the preprocessor are complete and the parser is under way.
//! See `docs/plan.md`.

pub mod keyword;
pub mod kind;
pub mod lexer;
pub mod preproc;
pub mod tree;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
pub use lexer::{Lexer, Token, tokenize};
pub use tree::{SyntaxElement, SyntaxNode, SyntaxToken, SystemVerilog};
