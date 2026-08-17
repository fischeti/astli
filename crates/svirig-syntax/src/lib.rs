//! Lexing, preprocessing, and parsing of SystemVerilog into a lossless
//! syntax tree.
//!
//! Only the lexer exists so far. See `docs/plan.md`.

pub mod keyword;
pub mod kind;
pub mod lexer;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
pub use lexer::{Lexer, Token, tokenize};
