//! Lexing, preprocessing, and parsing of SystemVerilog into a lossless
//! syntax tree.
//!
//! Only the token inventory exists so far. See `docs/plan.md`.

pub mod keyword;
pub mod kind;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
