//! SystemVerilog recursive-descent parser.
//!
//! The parser processes tokens emitted by a [`Tokens`] stream and generates a flat sequence
//! of [`Event`] items. Once parsing completes, [`build()`] resolves these events alongside the
//! input tokens to produce a lossless Rowan syntax tree that preserves all original source trivia.
//!
//! ### Architecture
//!
//! - [`event`]: Event recording, open node markers, and backtracking support.
//! - [`source`]: Token abstraction supporting both raw and expanded token streams.
//! - [`mod@build`]: Syntax tree assembly and trivia reattachment.
//! - [`mod@verbatim`]: Delimiter-balanced recovery for unrecognised syntactic regions.
//! - [`decl`]: Data types, type references, and variable/parameter declarations.
//! - [`mod@expr`]: Expression parsing using operator precedence climbing.
//! - [`stmt`]: Procedural statements, control flow, loops, and timing controls.
//! - [`mod@item`]: Module, package, interface, class, and port declarations.
//! - [`preprocessor`]: Directive parsing, macro invocation, and conditional compilation branches.
//! - [`tree`]: Standalone single-file syntax tree container.

pub mod build;
pub mod decl;
pub mod diagnostics;
pub mod event;
pub mod expr;
pub mod item;
pub mod preprocessor;
pub mod source;
pub mod stmt;
pub mod tree;
pub mod verbatim;

pub use build::build;
pub use decl::declaration;
pub use event::{Completed, Event, Events, Marker};
pub use expr::expr;
pub use item::item;
pub use source::{BranchShape, DirectiveShape, Expanded, Position, Raw, RegionShape, Tokens};
pub use stmt::statement;
pub use tree::SyntaxTree;
pub use verbatim::{Context, verbatim};

use svirig_preproc::{MacroTable, Session};
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};
use svirig_text::{Diagnostic, FileId, TokenOrigin};

/// Parser state tracking token consumption, emitted events, and grammatical scope.
pub struct Parser<T> {
    tokens: T,
    events: Events,
    scope: Scope,
}

/// Syntactic scope governing what constructs may appear in the current block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Outer module, interface, package, or description items.
    Item,
    /// Procedural statements within a block, loop, or subroutine.
    Statement,
}

/// Checkpoint of parser state across both the event buffer and the token stream.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
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

    /// Returns the current grammatical scope.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Sets the current grammatical scope and returns the previous scope.
    pub fn set_scope(&mut self, scope: Scope) -> Scope {
        std::mem::replace(&mut self.scope, scope)
    }

    /// Returns the syntax kind of the token `ahead` positions from the cursor.
    pub fn kind(&self, ahead: usize) -> SyntaxKind {
        self.tokens.kind(ahead)
    }

    /// Returns the source text of the token `ahead` positions from the cursor.
    pub fn text(&self, ahead: usize) -> &str {
        self.tokens.text(ahead)
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

    /// Consumes the token at the cursor without reclassifying its kind.
    pub fn bump(&mut self) {
        if !self.at_end() {
            self.bump_as(self.kind(0));
        }
    }

    /// Consumes the token at the cursor, reclassifying it as `kind` in the event stream.
    pub fn bump_as(&mut self, kind: SyntaxKind) {
        self.events.token(kind);
        self.tokens.bump();
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

    /// Returns the source origin of the token currently at the cursor.
    pub fn origin(&self) -> Option<TokenOrigin> {
        self.tokens.origin(0)
    }

    /// Records a diagnostic message in the parser event buffer.
    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.events.report(diagnostic);
    }

    /// Returns all diagnostics recorded and not rolled back.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.events.diagnostics()
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
pub fn any<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    match parser.scope {
        Scope::Item => item(parser, limit),
        Scope::Statement => statement(parser, limit),
    }
}

/// Parses `file` into a syntax tree using default preprocessor macro definitions.
pub fn parse(session: &Session, file: FileId) -> Parsed {
    parse_seeded(session, file, MacroTable::new())
}

/// Parser output containing resolved events and diagnostics prior to tree building.
#[derive(Debug)]
pub struct Finished {
    pub events: Vec<Event>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of parsing a file, containing the root syntax node and accumulated diagnostics.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub root: SyntaxNode,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses `file` into a syntax tree using a predefined table of macro definitions.
pub fn parse_seeded(session: &Session, file: FileId, seed: MacroTable) -> Parsed {
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
