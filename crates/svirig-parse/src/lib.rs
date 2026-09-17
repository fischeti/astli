//! The parser: a hand-written recursive descent over a token source, emitting
//! a flat list of [events](event) that a tree is built from afterwards.
//!
//! # Why events, and not a builder
//!
//! The obvious shape is to drive `rowan`'s `GreenNodeBuilder` inline, opening
//! and closing nodes as rules recurse. It works for a language that is nearly
//! LL(1), and SystemVerilog is not: `foo bar;` is a declaration only if `foo`
//! names a type, and `(a)(b)` is a cast or a call. Both are settled by parsing
//! one way, finding out, and **undoing it** -- and a builder's checkpoint can
//! wrap a node retroactively but cannot take one back.
//!
//! So a rule appends to a `Vec<Event>` instead. A snapshot is how long that
//! vector is; undoing is a truncate. The tree is built once, at the end, from
//! events that are known to be final.
//!
//! # The layout this grows into
//!
//! * [`event`] -- the event list, markers over it, and rollback.
//! * [`source`] -- the tokens, parameterised so that one grammar serves both
//!   the raw stream the formatter reads and the expanded one a compiler would.
//! * [`mod@build`] -- walks the events against the original tokens and puts the
//!   trivia back.
//! * [`mod@verbatim`] -- the fallback, for what no rule can make sense of.
//! * [`preprocessor`] -- directives, macro calls and conditional regions.
//! * [`mod@expr`] -- expressions, by precedence climbing.
//! * [`mod@decl`] -- data types, declarations, and the shapes that decide
//!   whether something is one.
//! * [`mod@item`] -- the shells that hold declarations, and what goes in them.
//! * [`mod@stmt`] -- statements, and the loops and conditionals that nest
//!   them.
//! * the rest of the grammar, split by what it parses. Whatever no rule
//!   claims still parses to a [`VERBATIM`] node, which is what [`parse`]
//!   keeps doing for what the rules cannot make sense of.
//!
//! # One question the rules ask about where they are
//!
//! The same five keywords write the same five constructs in a module and in
//! an `always` block, and only what *surrounds* them says whether a `begin`
//! holds items or statements. Threading that through every call is noise, so
//! it is [one field](Parser::scope) instead, set by the four rules that open
//! a body of a different kind -- and it exists at all because a conditional
//! branch is reached from the preprocessor, which cannot be told by its
//! caller what the text it guards is made of.
//!
//! See `docs/plan.md` and `docs/grammar-coverage.md`.

pub mod build;
pub mod decl;
pub mod event;
pub mod expr;
pub mod item;
pub mod preprocessor;
pub mod source;
pub mod stmt;
pub mod verbatim;

pub use build::build;
pub use decl::declaration;
pub use event::{Completed, Event, Events, Marker};
pub use expr::expr;
pub use item::item;
pub use source::{BranchShape, DirectiveShape, Expanded, Position, Raw, RegionShape, Tokens};
pub use stmt::statement;
pub use verbatim::{Context, verbatim};

use svirig_syntax::preproc::Input;
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};

/// A parse in progress: what is left to read, and what has been emitted.
///
/// Rules take one of these and nothing else. It is generic over the token
/// source so that the same rule serves both streams -- see [`Tokens`].
pub struct Parser<T> {
    tokens: T,
    events: Events,
    scope: Scope,
}

/// What the text at the cursor is made of.
///
/// Two, because two is what the difference is: the constructs a description
/// holds, and the statements a procedural block holds. A `begin` … `end`, a
/// `for` and an `if` are written the same way in both and differ only in what
/// their bodies may contain, which is why this is a property of the *place*
/// rather than a second set of rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Descriptions, and the items inside one.
    Item,
    /// Statements, as a procedural block or a subroutine body holds them.
    Statement,
}

/// Where a parse was, in both of the things that move.
///
/// The events and the tokens advance together and have to be put back
/// together, which is the whole reason this pairs them: rolling back one
/// without the other leaves a parse describing tokens it has not read.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    events: event::Snapshot,
    tokens: Position,
}

impl<T: Tokens> Parser<T> {
    pub fn new(tokens: T) -> Parser<T> {
        Parser {
            tokens,
            events: Events::new(),
            scope: Scope::Item,
        }
    }

    /// What the text at the cursor is made of.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Says what the text from here on is made of, and gives back what it was
    /// so that the rule which changed it can put it back.
    pub fn set_scope(&mut self, scope: Scope) -> Scope {
        std::mem::replace(&mut self.scope, scope)
    }

    /// The kind `ahead` tokens from the cursor; `0` is the cursor itself.
    pub fn kind(&self, ahead: usize) -> SyntaxKind {
        self.tokens.kind(ahead)
    }

    /// The text of the token `ahead` of the cursor.
    pub fn text(&self, ahead: usize) -> &str {
        self.tokens.text(ahead)
    }

    /// Whether the cursor is on `kind`.
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.kind(0) == kind
    }

    pub fn at_end(&self) -> bool {
        self.tokens.at_end()
    }

    /// Where the cursor is, for comparing against a bound.
    pub fn position(&self) -> Position {
        self.tokens.at()
    }

    /// The position `ahead` tokens from the cursor, clamped to the end.
    pub fn ahead(&self, ahead: u32) -> Position {
        self.tokens.ahead(ahead)
    }

    /// Whether the token `ahead` of the cursor touches the one before it.
    pub fn adjacent(&self, ahead: usize) -> bool {
        self.tokens.adjacent(ahead)
    }

    /// The index just past the bracket group opening at `ahead`.
    ///
    /// Answers the end of the tokens on a group that never closes, so that a
    /// caller's loop terminates on malformed input rather than on trust.
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

    /// How many tokens the macro reference at the cursor covers, if it is one.
    pub fn macro_call(&self) -> Option<u32> {
        self.tokens.macro_call()
    }

    /// The directive at the cursor, if there is one.
    pub fn directive(&self) -> Option<DirectiveShape> {
        self.tokens.directive()
    }

    /// The conditional region opening at the cursor, if one does.
    pub fn region(&self) -> Option<RegionShape> {
        self.tokens.region()
    }

    /// Takes the token at the cursor as it is.
    ///
    /// At the end this does nothing, so that a rule which loses track cannot
    /// emit tokens the file does not have.
    pub fn bump(&mut self) {
        if !self.at_end() {
            self.bump_as(self.kind(0));
        }
    }

    /// Takes the token at the cursor as `kind` instead of as it was lexed.
    ///
    /// For where the grammar knows better than the lexer could: an `IDENT`
    /// that names a type, a `<=` that is an assignment rather than a
    /// comparison.
    pub fn bump_as(&mut self, kind: SyntaxKind) {
        self.events.token(kind);
        self.tokens.bump();
    }

    /// Opens a node whose kind is not decided yet.
    pub fn start(&mut self) -> Marker {
        self.events.start()
    }

    /// Gives an open node its kind and closes it.
    pub fn complete(&mut self, marker: Marker, kind: SyntaxKind) -> Completed {
        marker.complete(&mut self.events, kind)
    }

    /// Drops an open node, keeping what was emitted inside it.
    pub fn abandon(&mut self, marker: Marker) {
        marker.abandon(&mut self.events)
    }

    /// Opens a node that will contain one already finished.
    pub fn precede(&mut self, node: Completed) -> Marker {
        node.precede(&mut self.events)
    }

    /// How far along the parse is, for a later [`Parser::rollback`].
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            events: self.events.snapshot(),
            tokens: self.tokens.at(),
        }
    }

    /// Puts the parse back where it was.
    pub fn rollback(&mut self, snapshot: Snapshot) {
        self.events.rollback(snapshot.events);
        self.tokens.seek(snapshot.tokens);
    }

    /// The events, in the order a tree is built in.
    pub fn finish(self) -> Vec<Event> {
        self.events.resolve()
    }
}

/// Parses one thing at the cursor, whatever the [scope](Scope) says it is.
///
/// Always takes at least one token unless the cursor is at the end or already
/// at `limit`, so a caller can loop on it without checking for progress.
pub fn any<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    match parser.scope {
        Scope::Item => item(parser, limit),
        Scope::Statement => statement(parser, limit),
    }
}

/// Parses one file into a lossless tree.
///
/// A file is a sequence of items, and what no rule can make sense of is a
/// [`VERBATIM`] node holding a balanced run of its tokens. What holds
/// whatever the rules do or do not reach is the property no rung may break --
/// the tree's text is the file's, byte for byte.
pub fn parse(input: Input) -> SyntaxNode {
    let mut parser = Parser::new(Raw::new(input));
    let file = parser.start();
    while !parser.at_end() {
        item(&mut parser, None);
    }
    parser.complete(file, SOURCE_FILE);

    SyntaxNode::new_root(build(&parser.finish(), input))
}
