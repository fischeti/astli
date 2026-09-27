//! Syntax tree reconstruction from parser event streams.
//!
//! Grammar rules emit a stream of [`Event`] markers over non-trivia tokens.
//! This module walks the resolved event stream alongside every token, trivia
//! included, to assemble a Rowan [`GreenNode`]: a file's raw tokens, or the
//! [`Piece`]s of an expansion.
//!
//! ### Trivia Attachment Rules
//!
//! - Leading whitespace and comments generally attach to the following token or node.
//! - A comment appearing on the same line as a preceding token attaches to that token as
//!   trailing trivia.
//! - Trailing trivia at the end of the file attaches inside the root node.

use rowan::{GreenNode, GreenNodeBuilder, Language};

use super::event::Event;
use super::source::Piece;
use astli_preproc::Input;
use astli_syntax::{SyntaxKind, SyntaxKind::*, SystemVerilog};
use astli_text::{Diagnostic, Span};

/// How deep a tree may be. A node that would sit deeper is left out, and its
/// tokens go to a `VERBATIM` node in place of the one at this depth.
///
/// Parsing never recurses this deep, but a long run of a left-associative
/// operator nests one node per operator without recursing at all, and
/// `rowan` drops a tree recursively. Real code nests a `|` over hundreds of
/// operands, generated register maps most of all, so this is several times
/// that.
pub(crate) const MAX_DEPTH: u32 = 2048;

/// What the builder reads: every token, trivia included, with its text.
pub trait Leaves {
    fn len(&self) -> u32;
    fn kind(&self, at: u32) -> SyntaxKind;
    fn text(&self, at: u32) -> &str;
    fn span(&self, at: u32) -> Option<Span>;
}

impl Leaves for Input<'_> {
    fn len(&self) -> u32 {
        Input::len(self)
    }

    fn kind(&self, at: u32) -> SyntaxKind {
        Input::kind(self, at)
    }

    fn text(&self, at: u32) -> &str {
        Input::text(self, at)
    }

    fn span(&self, at: u32) -> Option<Span> {
        let token = self.token(at);
        Some(Span::new(self.src_id, token.start, token.end))
    }
}

impl Leaves for [Piece<'_>] {
    fn len(&self) -> u32 {
        <[Piece]>::len(self) as u32
    }

    fn kind(&self, at: u32) -> SyntaxKind {
        self[at as usize].kind
    }

    fn text(&self, at: u32) -> &str {
        self[at as usize].text
    }

    fn span(&self, at: u32) -> Option<Span> {
        self[at as usize].span
    }
}

/// Reconstructs a Rowan [`GreenNode`] syntax tree from parser events and the tokens they were read from.
///
/// The provided `events` slice must be resolved prior to building. Where
/// the tree would be deeper than [`MAX_DEPTH`], it is flattened, and the
/// warning that says so is returned with it.
///
/// # Panics
///
/// - Panics if a rule attempts to consume more tokens than exist in the file.
/// - Panics if non-EOF tokens remain unconsumed after all events are processed.
pub fn build<L: Leaves + ?Sized>(events: &[Event], input: &L) -> (GreenNode, Option<Diagnostic>) {
    let mut builder = Builder {
        input,
        green: GreenNodeBuilder::new(),
        at: 0,
        depth: 0,
        flattening: 0,
        flattened: None,
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
                if builder.flattening > 0 {
                    builder.flattening += 1;
                } else if builder.depth + 1 == MAX_DEPTH {
                    builder.open(VERBATIM);
                    builder.flattening = 1;
                    if builder.flattened.is_none() {
                        builder.flattened = (builder.at..input.len())
                            .find(|&at| !input.kind(at).is_trivia())
                            .and_then(|at| input.span(at));
                    }
                } else {
                    builder.open(kind);
                }
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
                builder.flattening = builder.flattening.saturating_sub(1);
                if builder.flattening == 0 {
                    builder.close();
                }
            }
            Event::Tombstone => unreachable!("events were not resolved"),
        }
    }

    let left = (builder.at..input.len())
        .filter(|&at| input.kind(at) != EOF)
        .count();
    assert_eq!(left, 0, "{left} tokens were never put in the tree");

    let deep = (builder.flattened).map(|at| crate::diagnostics::nested_too_deep(at, MAX_DEPTH));
    (builder.green.finish(), deep)
}

/// Helper state for constructing the Rowan green tree while tracking trivia placement.
struct Builder<'a, L: ?Sized> {
    input: &'a L,
    green: GreenNodeBuilder<'static>,
    at: u32,
    depth: u32,
    /// How many of the nodes open in the events are left out of the tree,
    /// the one at [`MAX_DEPTH`] included.
    flattening: u32,
    /// Where the first node left out starts.
    flattened: Option<Span>,
}

impl<L: Leaves + ?Sized> Builder<'_, L> {
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
        // With nothing emitted yet there is no token to trail, and a comment
        // opening the file belongs with the rest of its block to what follows.
        if self.depth == 0 || self.at == 0 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Events;
    use crate::testing::Source;

    #[test]
    #[should_panic(expected = "tokens were never put in the tree")]
    fn a_rule_that_stops_early_is_a_bug() {
        let source = Source::new("module m; endmodule");
        let mut events = Events::new();
        let file = events.start();
        events.token(MODULE_KW);
        file.complete(&mut events, SOURCE_FILE);
        build(&events.resolve(), &source.input());
    }

    #[test]
    #[should_panic(expected = "more tokens than the file has")]
    fn a_rule_that_runs_past_the_end_is_a_bug() {
        let source = Source::new("module");
        let mut events = Events::new();
        let file = events.start();
        for _ in 0..3 {
            events.token(MODULE_KW);
        }
        file.complete(&mut events, SOURCE_FILE);
        build(&events.resolve(), &source.input());
    }
}
