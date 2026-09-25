//! The vocabulary of a SystemVerilog syntax tree: its kinds, its lexer, its
//! tree types, and typed views over them.
//!
//! This crate defines the tree but never builds one from source; that is
//! `astli-parse`. What is here is what every later stage shares:
//!
//! - [`SyntaxKind`]: one enum for every token kind and every node kind.
//! - [`tokenize`] / [`Lexer`]: source text to [`Token`]s.
//! - [`SyntaxNode`], [`SyntaxToken`], [`SyntaxElement`]: the [`rowan`] tree,
//!   bound to [`SyntaxKind`] through [`SystemVerilog`].
//! - [`ast`]: a typed view per node kind, such as
//!   [`ModuleDecl`](ast::ModuleDecl), with an accessor per part.
//!
//! # Lexing
//!
//! The lexer covers every byte: whitespace and comments are tokens (their
//! kind [`is_trivia`](SyntaxKind::is_trivia)), a byte it cannot place is a
//! `LEX_ERROR`, and the stream ends with an empty `EOF`. A [`Token`] is a
//! kind and a byte range, not text.
//!
//! ```
//! use astli_syntax::{SyntaxKind::*, tokenize};
//!
//! let source = "logic [7:0] q; // state\n";
//! let tokens = tokenize(source);
//!
//! let significant: Vec<_> = tokens.iter().filter(|token| !token.kind.is_trivia()).collect();
//! assert_eq!(significant[0].kind, LOGIC_KW);
//! assert_eq!(significant[6].text(source), "q");
//! assert_eq!(tokens.last().unwrap().kind, EOF);
//! ```
//!
//! # Kinds
//!
//! A token kind is named for what it spells rather than what it means, since
//! SystemVerilog reuses its punctuation: `<=` is `LT_EQ` whether it compares
//! or assigns. A keyword is its spelling with `_KW`, and a node kind names a
//! construct, such as `MODULE_DECL`. A few node kinds can stand almost
//! anywhere: `VERBATIM`, the tokens of a stretch the parser did not take
//! apart; and `MACRO_CALL`, `DIRECTIVE` and `CONDITIONAL_REGION`, since the
//! tree is of the file as written, before preprocessing.
//!
//! ```
//! use astli_syntax::SyntaxKind::*;
//!
//! assert!(MODULE_KW.is_keyword() && MODULE_KW.is_token());
//! assert_eq!(MODULE_KW.keyword_text(), Some("module"));
//! assert!(MODULE_DECL.is_node());
//! ```
//!
//! # The tree
//!
//! A tree is lossless: its leaves are every token of the file, trivia
//! included, so a node's text is the source it covers. Walk it with
//! `rowan`'s methods, such as [`children`](rowan::SyntaxNode::children),
//! [`descendants`](rowan::SyntaxNode::descendants) and
//! [`children_with_tokens`](rowan::SyntaxNode::children_with_tokens), and
//! read a node's kind with [`kind`](rowan::SyntaxNode::kind).
//!
//! Or view a node through [`ast`]. A view is a node whose kind has been
//! checked and nothing more; each accessor walks the children when called,
//! and returns `None` where the source leaves that part out or the parser
//! did not reach it. Each view's parts, in order, are in
//! [`astli.ungram`](https://github.com/fischeti/astli/blob/main/crates/astli-syntax/astli.ungram),
//! which the views are generated from.
//!
//! A tree normally comes from `astli-parse`. Built by hand, `module top;
//! endmodule` looks like this:
//!
//! ```
//! use astli_syntax::ast::{self, AstNode};
//! use astli_syntax::{SyntaxKind::*, SyntaxNode, SystemVerilog};
//! use rowan::{GreenNodeBuilder, Language};
//!
//! let mut builder = GreenNodeBuilder::new();
//! builder.start_node(SystemVerilog::kind_to_raw(SOURCE_FILE));
//! builder.start_node(SystemVerilog::kind_to_raw(MODULE_DECL));
//! for (kind, text) in [
//!     (MODULE_KW, "module"),
//!     (WHITESPACE, " "),
//!     (IDENT, "top"),
//!     (SEMICOLON, ";"),
//!     (WHITESPACE, "\n"),
//!     (ENDMODULE_KW, "endmodule"),
//! ] {
//!     builder.token(SystemVerilog::kind_to_raw(kind), text);
//! }
//! builder.finish_node();
//! builder.finish_node();
//! let root = SyntaxNode::new_root(builder.finish());
//! assert_eq!(root.text(), "module top;\nendmodule");
//!
//! let file = ast::SourceFile::cast(root).unwrap();
//! let Some(ast::Item::ModuleDecl(module)) = file.items().next() else { panic!() };
//! assert_eq!(module.name().unwrap().text(), "top");
//! assert!(module.port_list().is_none());
//! assert_eq!(module.items().count(), 0);
//! ```

pub mod ast;
mod keyword;
mod kind;
mod lexer;
mod tree;

pub use keyword::KeywordVersion;
pub use kind::SyntaxKind;
pub use lexer::{Lexer, Token, tokenize};
pub use tree::{SyntaxElement, SyntaxNode, SyntaxToken, SystemVerilog};
