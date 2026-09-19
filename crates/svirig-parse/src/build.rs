//! Syntax tree reconstruction from parser event streams.
//!
//! Grammar rules emit a stream of [`Event`] markers over non-trivia tokens.
//! This module walks the resolved event stream alongside the original token input to
//! assemble a Rowan [`GreenNode`], preserving all intervening whitespace and comments.
//!
//! ### Trivia Attachment Rules
//!
//! - Leading whitespace and comments generally attach to the following token or node.
//! - A comment appearing on the same line as a preceding token attaches to that token as
//!   trailing trivia.
//! - Trailing trivia at the end of the file attaches inside the root node.

use rowan::{GreenNode, GreenNodeBuilder, Language};

use super::event::Event;
use svirig_preproc::Input;
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SystemVerilog};

/// Reconstructs a Rowan [`GreenNode`] syntax tree from parser events and raw input tokens.
///
/// The provided `events` slice must be resolved prior to building.
///
/// # Panics
///
/// - Panics if a rule attempts to consume more tokens than exist in the file.
/// - Panics if non-EOF tokens remain unconsumed after all events are processed.
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
                builder.emit_trailing();
                builder.open(kind);
            }
            Event::Token { kind } => {
                builder.emit_trivia();
                builder.emit_token(kind);
            }
            Event::Finish => {
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

/// Helper state for constructing the Rowan green tree while tracking trivia placement.
struct Builder<'a> {
    input: Input<'a>,
    green: GreenNodeBuilder<'static>,
    at: u32,
    depth: u32,
}

impl Builder<'_> {
    /// Opens a new syntax node of `kind`.
    fn open(&mut self, kind: SyntaxKind) {
        self.green.start_node(SystemVerilog::kind_to_raw(kind));
        self.depth += 1;
    }

    /// Closes the currently open syntax node.
    fn close(&mut self) {
        self.green.finish_node();
        self.depth -= 1;
    }

    /// Writes the token at the cursor as `kind`.
    fn emit_token(&mut self, kind: SyntaxKind) {
        assert!(
            self.at < self.input.len(),
            "a rule consumed more tokens than the file has"
        );
        let text = self.input.text(self.at);
        self.green.token(SystemVerilog::kind_to_raw(kind), text);
        self.at += 1;
    }

    /// Emits all consecutive trivia tokens at the cursor.
    fn emit_trivia(&mut self) {
        while self.at < self.input.len() && self.input.kind(self.at).is_trivia() {
            let kind = self.input.kind(self.at);
            self.emit_token(kind);
        }
    }

    /// Emits trailing trivia on the current line to attach to the preceding token.
    fn emit_trailing(&mut self) {
        if self.depth == 0 {
            return;
        }
        for _ in 0..self.trailing() {
            let kind = self.input.kind(self.at);
            self.emit_token(kind);
        }
    }

    /// Counts how many trivia tokens at the cursor belong to the current line.
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
            if kind == WHITESPACE && text.contains('\n') {
                break;
            }
            seen += 1;
            if kind != WHITESPACE {
                keep = seen;
                if text.contains('\n') {
                    break;
                }
            }
            at += 1;
        }

        keep
    }
}
