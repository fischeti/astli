//! Turning events back into a tree, and putting the trivia back.
//!
//! A rule emits [events](super::event) over the tokens it can see, which are
//! the ones [`Tokens`](super::Tokens) let through. The whitespace and comments
//! it never saw are still in the file, and every one of them has to end up in
//! the tree: a formatter that cannot see a comment will delete it.
//!
//! So this walks the events and the original tokens together. Each
//! [`Event::Token`] consumes exactly one token the grammar could see, and the
//! trivia in between is placed around it by the rule below.
//!
//! # Where a comment belongs
//!
//! > Leading trivia belongs to the item that follows; a comment on the same
//! > line as the token before it stays with that token.
//!
//! Which is to say: `logic x; // why` keeps its comment inside the
//! declaration, and
//!
//! ```systemverilog
//! // what the next thing is for
//! logic y;
//! ```
//!
//! puts its comment inside the declaration that follows. Whitespace alone
//! never attaches backwards -- it carries no signal, and the formatter asks
//! for the separation it wants rather than reading it here.
//!
//! The rule is applied at every node boundary, in both directions: a node
//! about to open takes nothing that belongs to what came before, and a node
//! about to close takes its own trailing comment with it.
//!
//! # This is the raw path
//!
//! Losslessness is a raw-mode idea: the tree is meant to reproduce one file,
//! byte for byte. An expanded stream has followed includes and dropped
//! branches, so its tokens are not one file's and there is nothing for them to
//! round-trip against.

use rowan::{GreenNode, GreenNodeBuilder, Language};

use super::event::Event;
use svirig_preproc::Input;
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SystemVerilog};

/// Builds the tree `events` describe over the tokens of `input`.
///
/// `events` must be [resolved](super::Events::resolve).
///
/// # Panics
///
/// If the events do not fit the tokens -- more [`Event::Token`]s than there
/// are tokens to consume, or tokens left over that no event asked for. Either
/// is a bug in a rule, and either would produce a tree that is no longer the
/// file.
pub fn build(events: &[Event], input: Input) -> GreenNode {
    let mut builder = Builder {
        input,
        green: GreenNodeBuilder::new(),
        at: 0,
        depth: 0,
    };

    let last = events.len().saturating_sub(1);
    for (index, event) in events.iter().enumerate() {
        match *event {
            Event::Start {
                kind,
                forward_parent,
            } => {
                debug_assert!(forward_parent.is_none(), "events were not resolved");
                // Whatever belongs to the token before this node stays outside
                // it; the rest is leading trivia and falls inside.
                builder.emit_trailing();
                builder.open(kind);
            }
            Event::Token { kind } => {
                builder.emit_trivia();
                builder.emit_token(kind);
            }
            Event::Finish => {
                // The last one closes the root, so there is nowhere left for
                // trivia to go afterwards.
                if index == last {
                    builder.emit_trivia();
                } else {
                    builder.emit_trailing();
                }
                builder.close();
            }
            Event::Tombstone => unreachable!("events were not resolved"),
        }
    }

    let left = (builder.at..input.len())
        .filter(|&at| input.kind(at) != EOF)
        .count();
    assert_eq!(left, 0, "{left} tokens were never put in the tree");

    builder.green.finish()
}

struct Builder<'a> {
    input: Input<'a>,
    green: GreenNodeBuilder<'static>,
    /// The next token of the file to put in the tree.
    at: u32,
    /// Nodes open. A tree has one root, so nothing may be written while this
    /// is zero.
    depth: u32,
}

impl Builder<'_> {
    fn open(&mut self, kind: SyntaxKind) {
        self.green.start_node(SystemVerilog::kind_to_raw(kind));
        self.depth += 1;
    }

    fn close(&mut self) {
        self.green.finish_node();
        self.depth -= 1;
    }

    /// Writes the token at the cursor, as `kind`.
    ///
    /// The kind comes from the event rather than from the token because a rule
    /// may reclassify what it consumes; the text is the file's either way.
    fn emit_token(&mut self, kind: SyntaxKind) {
        assert!(
            self.at < self.input.len(),
            "a rule consumed more tokens than the file has"
        );
        let text = self.input.text(self.at);
        self.green.token(SystemVerilog::kind_to_raw(kind), text);
        self.at += 1;
    }

    /// Writes every trivium at the cursor.
    fn emit_trivia(&mut self) {
        while self.at < self.input.len() && self.input.kind(self.at).is_trivia() {
            let kind = self.input.kind(self.at);
            self.emit_token(kind);
        }
    }

    /// Writes only the trivia that belongs to the token already written: a
    /// comment on that same line, and the space in front of it.
    ///
    /// Before the root opens there is no token already written and no node to
    /// write into, so a file that opens with a comment keeps it as leading
    /// trivia rather than putting it outside the tree.
    fn emit_trailing(&mut self) {
        if self.depth == 0 {
            return;
        }
        for _ in 0..self.trailing() {
            let kind = self.input.kind(self.at);
            self.emit_token(kind);
        }
    }

    /// How many trivia at the cursor belong backwards.
    fn trailing(&self) -> u32 {
        let mut seen = 0;
        let mut keep = 0;
        let mut at = self.at;

        while at < self.input.len() {
            let kind = self.input.kind(at);
            if !kind.is_trivia() {
                break;
            }
            let text = self.input.text(at);
            // The line has ended, so whatever follows annotates what comes
            // next rather than what came before.
            if kind == WHITESPACE && text.contains('\n') {
                break;
            }
            seen += 1;
            if kind != WHITESPACE {
                keep = seen;
                // A block comment that spans lines ends the line too, but it
                // began on this one, so it is kept.
                if text.contains('\n') {
                    break;
                }
            }
            at += 1;
        }

        keep
    }
}
