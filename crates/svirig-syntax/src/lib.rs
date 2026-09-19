//! Syntax definitions and tokenization for SystemVerilog.
//!
//! This crate provides the concrete syntax tree (CST) definitions and lexing infrastructure:
//! - [`SyntaxKind`]: Unified enum representing all token kinds and syntax tree node types.
//! - [`Lexer`] / [`tokenize`]: Gapless SystemVerilog lexer built on `logos` with keyword recognition.
//! - [`keyword`]: Reserved keyword tables and version-aware lookup.
//! - [`tree`]: Integration with the `rowan` CST library ([`SyntaxNode`], [`SyntaxToken`], [`SyntaxElement`]).

pub mod keyword;
pub mod kind;
pub mod lexer;
pub mod tree;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
pub use lexer::{Lexer, Token, tokenize};
pub use tree::{SyntaxElement, SyntaxNode, SyntaxToken, SystemVerilog};
