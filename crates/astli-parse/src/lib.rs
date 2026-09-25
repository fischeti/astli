//! A SystemVerilog parser that builds a lossless syntax tree.
//!
//! The tree holds every byte of the input, whitespace and comments included,
//! so `root.text()` is the source again. Parsing never fails: what the grammar
//! does not cover yet, or cannot make sense of, becomes a `VERBATIM` node over
//! the tokens as written, and a [`Diagnostic`] reports anything malformed.
//!
//! # Parsing one file
//!
//! [`SyntaxTree`] is the door for a tool that reads one file at a time.
//!
//! ```
//! use astli_parse::SyntaxTree;
//!
//! let text = "module top;\n  logic q;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", text.to_string());
//! // Or `SyntaxTree::read("top.sv")?`, which reads the file itself.
//!
//! assert_eq!(tree.root().text(), text);
//! assert!(tree.diagnostics().is_empty());
//! ```
//!
//! # Walking the tree
//!
//! The tree is a [`rowan`] tree: [`SyntaxNode`]s, each with a [`SyntaxKind`],
//! over [`SyntaxToken`](astli_syntax::SyntaxToken)s. Walk it untyped, by kind:
//!
//! ```
//! use astli_parse::SyntaxTree;
//! use astli_syntax::SyntaxKind::*;
//!
//!
//! let tree = SyntaxTree::parse("top.sv", "module a; endmodule\nmodule b; endmodule\n".into());
//! let modules = tree.root().descendants().filter(|node| node.kind() == MODULE_DECL);
//! assert_eq!(modules.count(), 2);
//! ```
//!
//! Or through the typed views in [`astli_syntax::ast`], which name each
//! node's parts. A view is a checked node and nothing more: cast any node
//! with [`AstNode::cast`](astli_syntax::ast::AstNode::cast), and expect an
//! accessor to return `None` where the source leaves a part out or the parser
//! fell back to `VERBATIM`.
//!
//! ```
//! use astli_parse::SyntaxTree;
//! use astli_syntax::ast::{self, AstNode};
//!
//! let text = "module top (input a);\n  assign y = a;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", text.to_string());
//! let file = ast::SourceFile::cast(tree.root().clone()).unwrap();
//!
//! for item in file.items() {
//!     if let ast::Item::ModuleDecl(module) = item {
//!         assert_eq!(module.name().unwrap().text(), "top");
//!         assert_eq!(module.port_list().unwrap().ports().count(), 1);
//!         assert!(matches!(module.items().next(), Some(ast::Item::ContinuousAssign(_))));
//!     }
//! }
//! ```
//!
//! # Diagnostics
//!
//! A [`Diagnostic`]'s span is a byte range in the file.
//! [`SyntaxTree::line_col`] turns an offset into a line and column; to render
//! one with the source under it, hand [`SyntaxTree::origins`] to `astli-diag`.
//!
//! ```
//! use astli_parse::SyntaxTree;
//!
//! let tree = SyntaxTree::parse("top.sv", "module top;\n  logic q;\n".into());
//! let diagnostic = &tree.diagnostics()[0];
//! assert_eq!(diagnostic.code.as_str(), "unclosed-at-end-of-file");
//! assert_eq!(tree.line_col(diagnostic.at.start).to_string(), "1:1");
//!
//! // The whole file became one `VERBATIM` node, and is still all there.
//! assert_eq!(tree.root().text(), tree.source());
//! ```
//!
//! # Directives and macros
//!
//! The parser reads a file as written, which is *raw mode*: it does not follow
//! an `` `include ``, and it keeps a macro call as a `MACRO_CALL` node rather
//! than expanding it. An `` `ifdef `` becomes a `CONDITIONAL_REGION` holding
//! every branch, since which one is taken depends on a build it does not see.
//!
//! What raw mode cannot tell from the file alone is a macro's arity: whether
//! `` `M (x) `` passes `(x)` to `M` or follows it. A macro defined in the file
//! answers that for itself; for one defined elsewhere, [`parse`] takes a
//! [`MacroTable`] to learn it from. That is the second tier: an explicit
//! [`Session`], which is also what a custom [`Reader`](astli_text::Reader)
//! or spans compared across files need.
//!
//! ```
//! use astli_parse::parse;
//! use astli_preproc::{Build, Session};
//! use astli_syntax::SyntaxKind::*;
//!
//! let build = Build::new().define("WIDTH", "32");
//! let mut session = Session::new().building(build);
//! let file = session.add("top.sv", "module top;\n  logic [`WIDTH-1:0] q;\nendmodule\n".into());
//!
//! let expanded = session.expand(file);
//! let parsed = parse(&session, file, expanded.macros);
//!
//! let calls = parsed.root.descendants().filter(|node| node.kind() == MACRO_CALL);
//! assert_eq!(calls.count(), 1);
//! ```
//!
//! [`parse`] takes the session by shared reference and the tree does not
//! borrow it, so more files can be added while earlier trees stay alive.
//!
//! # How it works
//!
//! Rules read a token stream and append a flat list of events rather than
//! building nodes, so a speculative parse is undone by truncating the list.
//! The tree is built once, at the end, with the trivia the rules never saw
//! put back. A comment on its own line goes with what follows it, one that
//! ends a line with what precedes it.

mod build;
mod decl;
mod diagnostics;
mod event;
mod expr;
mod item;
mod preprocessor;
mod source;
mod stmt;
mod tree;
mod verbatim;

#[cfg(test)]
mod testing;

use source::{DirectiveShape, Position, Raw, RegionShape, Tokens};
pub use tree::SyntaxTree;

use build::build;
use event::{Completed, Event, Events, Marker};
use item::item;
use stmt::statement;

use astli_preproc::{MacroTable, Session};
use astli_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};
use astli_text::{Diagnostic, SourceId, Span};

/// Parser state tracking token consumption, emitted events, and grammatical scope.
pub(crate) struct Parser<T> {
    tokens: T,
    events: Events,
    scope: Scope,
}

/// Syntactic scope governing what constructs may appear in the current block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// Outer module, interface, package, or description items.
    Item,
    /// Procedural statements within a block, loop, or subroutine.
    Statement,
}

/// Checkpoint of parser state across both the event buffer and the token stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Snapshot {
    events: event::Snapshot,
    tokens: Position,
}

impl<T: Tokens> Parser<T> {
    /// Creates a new parser reading from `tokens` with initial [`Scope::Item`].
    pub fn new(tokens: T) -> Parser<T> {
        Parser {
            tokens,
            events: Events::new(),
            scope: Scope::Item,
        }
    }

    /// Sets the current grammatical scope and returns the previous scope.
    pub fn set_scope(&mut self, scope: Scope) -> Scope {
        std::mem::replace(&mut self.scope, scope)
    }

    /// Returns the syntax kind of the token `ahead` positions from the cursor.
    pub fn kind(&self, ahead: usize) -> SyntaxKind {
        self.tokens.kind(ahead)
    }

    /// Returns `true` if the token at the cursor matches `kind`.
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.kind(0) == kind
    }

    /// Returns `true` if the parser has reached the end of the token stream.
    pub fn at_end(&self) -> bool {
        self.tokens.at_end()
    }

    /// Returns the current token stream position.
    pub fn position(&self) -> Position {
        self.tokens.at()
    }

    /// Returns the position `ahead` tokens from the cursor, clamped to stream length.
    pub fn ahead(&self, ahead: u32) -> Position {
        self.tokens.ahead(ahead)
    }

    /// Returns `true` if the token `ahead` positions from the cursor touches its predecessor directly.
    pub fn adjacent(&self, ahead: usize) -> bool {
        self.tokens.adjacent(ahead)
    }

    /// Finds the index immediately following a balanced group of `open` and `close` tokens.
    pub fn past_group(&self, ahead: usize, open: SyntaxKind, close: SyntaxKind) -> usize {
        let mut depth = 0u32;
        let mut at = ahead;
        loop {
            let kind = self.kind(at);
            if kind == open {
                depth += 1;
            } else if kind == close {
                depth -= 1;
            } else if kind == EOF {
                return at;
            }
            at += 1;
            if depth == 0 {
                return at;
            }
        }
    }

    /// Returns the token count covered by a macro call at the cursor, if one is present.
    pub fn macro_call(&self) -> Option<u32> {
        self.tokens.macro_call()
    }

    /// Returns the directive shape at the cursor, if one is present.
    pub fn directive(&self) -> Option<DirectiveShape> {
        self.tokens.directive()
    }

    /// Returns the conditional region shape at the cursor, if one begins here.
    pub fn region(&self) -> Option<RegionShape> {
        self.tokens.region()
    }

    /// Consumes the token at the cursor.
    pub fn bump(&mut self) {
        if !self.at_end() {
            self.events.token(self.kind(0));
            self.tokens.bump();
        }
    }

    /// Starts a new syntax node and returns an uncompleted marker.
    pub fn start(&mut self) -> Marker {
        self.events.start()
    }

    /// Completes an open marker as `kind` and returns a handle to the completed node.
    pub fn complete(&mut self, marker: Marker, kind: SyntaxKind) -> Completed {
        marker.complete(&mut self.events, kind)
    }

    /// Discards an open marker, retaining any tokens emitted inside it.
    pub fn abandon(&mut self, marker: Marker) {
        marker.abandon(&mut self.events)
    }

    /// Opens a new marker that will wrap an already completed node as its parent.
    pub fn precede(&mut self, node: Completed) -> Marker {
        node.precede(&mut self.events)
    }

    /// Captures a snapshot of parser position and event buffer state.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            events: self.events.snapshot(),
            tokens: self.tokens.at(),
        }
    }

    /// Rolls back the parser and token stream to the given snapshot.
    pub fn rollback(&mut self, snapshot: Snapshot) {
        self.events.rollback(snapshot.events);
        self.tokens.seek(snapshot.tokens);
    }

    /// Returns the span of the token currently at the cursor.
    pub fn span(&self) -> Option<Span> {
        self.tokens.span(0)
    }

    /// Finishes parsing and returns the resolved event list and diagnostics.
    pub fn finish(mut self) -> Finished {
        let diagnostics = self.events.take_diagnostics();
        Finished {
            events: self.events.resolve(),
            diagnostics,
        }
    }
}

/// Parses a single item or statement at the cursor according to the active scope.
pub(crate) fn any<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    match parser.scope {
        Scope::Item => item(parser, limit),
        Scope::Statement => statement(parser, limit),
    }
}

/// Parser output containing resolved events and diagnostics prior to tree building.
#[derive(Debug)]
pub(crate) struct Finished {
    pub(crate) events: Vec<Event>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// What [`parse`] makes of one file.
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The `SOURCE_FILE` node, whose text is the file's.
    pub root: SyntaxNode,
    /// What the grammar found malformed, in the order it found it.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses `file` in raw mode, taking the arity of a macro the file does not
/// define from `seed`.
///
/// Pass an empty [`MacroTable`] when there is no build to learn from, or the
/// `macros` of [`Session::expand`] when there is. See the
/// [crate docs](crate#directives-and-macros).
pub fn parse(session: &Session, file: SourceId, seed: MacroTable) -> Parsed {
    let input = session.input(file);
    let mut parser = Parser::new(Raw::seeded(input, seed));
    let root = parser.start();
    while !parser.at_end() {
        item(&mut parser, None);
    }
    parser.complete(root, SOURCE_FILE);

    let finished = parser.finish();
    Parsed {
        root: SyntaxNode::new_root(build(&finished.events, input)),
        diagnostics: finished.diagnostics,
    }
}
