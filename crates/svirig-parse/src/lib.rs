//! SystemVerilog recursive-descent parser.
//!
//! Rules read a [`Tokens`] stream and append a flat list of events rather than
//! building nodes, so a speculative parse is undone by truncating the list. The
//! tree is built once, at the end, with the trivia the rules never saw put back.
//!
//! The door is [`SyntaxTree`] for one file, or [`parse`] against a session.
//! Everything else is internal: the event list (`event`), the tree builder
//! (`build`), the fallback (`verbatim`), and the grammar (`expr`, `decl`,
//! `stmt`, `item`, `preprocessor`).

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

use svirig_preproc::{MacroTable, Session};
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};
use svirig_text::{Diagnostic, SourceId, Span};

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

/// Result of parsing a file, containing the root syntax node and accumulated diagnostics.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub root: SyntaxNode,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses `file` in raw mode, taking the arity of a macro the file does not
/// define from `seed`.
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
