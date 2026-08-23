//! Lexing, preprocessing, and parsing of SystemVerilog into a lossless
//! syntax tree.
//!
//! The lexer is complete and the preprocessor is under way. See
//! `docs/plan.md`.

pub mod keyword;
pub mod kind;
pub mod lexer;
pub mod preproc;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
pub use lexer::{Lexer, Token, tokenize};
