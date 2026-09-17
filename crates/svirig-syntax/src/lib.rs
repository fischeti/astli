//! The vocabulary everything else is written in: what a token is, what a node
//! can be, and the lexer that produces the first of those.
//!
//! [`SyntaxKind`] covers both halves of a tree, tokens and nodes together,
//! which is what makes [`is_node`](SyntaxKind::is_node) a range check. It is
//! also why the lexer is here rather than in a crate of its own: `logos`
//! derives on that enum, [`keyword::lookup`] maps into it, and [`tree`]
//! implements `rowan`'s `Language` over it, so the four are one thing.
//!
//! What reads the tokens lives above: `svirig-preproc` for directives and
//! macros, `svirig-parse` for the grammar. See `docs/plan.md`.

pub mod keyword;
pub mod kind;
pub mod lexer;
pub mod tree;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
pub use lexer::{Lexer, Token, tokenize};
pub use tree::{SyntaxElement, SyntaxNode, SyntaxToken, SystemVerilog};
