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
//! * the grammar itself, split by what it parses. **Not written yet**: every
//!   file currently parses to one [`VERBATIM`]
//!   node, which is what [`parse`] will keep doing for whatever the rules
//!   cannot make sense of.
//!
//! See `docs/plan.md` and `docs/next.md`.

pub mod build;
pub mod event;
pub mod source;
pub mod verbatim;

pub use build::build;
pub use event::{Completed, Event, Events, Marker};
pub use source::{Expanded, Position, Raw, Tokens};
pub use verbatim::{Context, verbatim};

use crate::preproc::Input;
use crate::{SyntaxKind, SyntaxKind::*, SyntaxNode};

/// A parse in progress: what is left to read, and what has been emitted.
///
/// Rules take one of these and nothing else. It is generic over the token
/// source so that the same rule serves both streams -- see [`Tokens`].
pub struct Parser<T> {
    tokens: T,
    events: Events,
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
        }
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

    /// How many tokens the macro reference at the cursor covers, if it is one.
    pub fn macro_call(&self) -> Option<u32> {
        self.tokens.macro_call()
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

/// Parses one file into a lossless tree.
///
/// There is no grammar yet, so the file comes back as a run of
/// [`VERBATIM`] nodes, one per item the fallback could delimit: correct,
/// useless, and the shape every later rung whittles down. What already holds
/// is the property the rungs must not break -- the tree's text is the file's,
/// byte for byte.
pub fn parse(input: Input) -> SyntaxNode {
    let mut parser = Parser::new(Raw::new(input));

    let file = parser.start();
    while !parser.at_end() {
        verbatim(&mut parser, Context::Terminated);
    }
    parser.complete(file, SOURCE_FILE);

    SyntaxNode::new_root(build(&parser.finish(), input))
}
