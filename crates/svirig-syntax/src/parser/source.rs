//! Where a rule gets its tokens, and the one thing the grammar is generic
//! over.
//!
//! The preprocessor produces two streams from the same machinery: the **raw**
//! one, which keeps macro calls as written and every branch of a conditional,
//! and the **expanded** one, which has substituted the macros and followed the
//! includes. A formatter has to read the first and a compiler the second, and
//! the whole point of [`Tokens`] is that the grammar between them is one
//! grammar. A rule asks what kind is at the cursor; it does not ask which
//! stream it is reading.
//!
//! # Trivia is not here
//!
//! A rule never sees whitespace or a comment. Both streams keep them --
//! nothing is lost -- but a position here counts only tokens the grammar cares
//! about, so no rule has to remember to skip. Putting them back is the tree
//! builder's job, and it walks the original tokens to do it.
//!
//! That is also why a [`Position`] is an index into the *grammar* tokens
//! rather than a byte offset or a raw token index: it is the coordinate both
//! implementations can offer, and the only one a rule should ever hold.
//!
//! # `EOF` is a token
//!
//! Looking past the end gives [`EOF`] rather than `None`, as
//! many times as it is asked. A rule that has run off the end therefore
//! behaves like one that reached the end, which is the behaviour every rule
//! wants and none would remember to write.

use rustc_hash::FxHashMap;

use svirig_text::Origins;

use crate::preproc::{ExpandedToken, Input, Item, scan};
use crate::{SyntaxKind, SyntaxKind::*};

/// How far through the tokens a parse is.
///
/// Opaque, and only ever compared or handed back: what it counts is one
/// implementation's business. Taking one and later [seeking](Tokens::seek) to
/// it is the token half of a rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(u32);

/// A stream of tokens with the trivia already stepped over.
pub trait Tokens {
    /// The kind `ahead` tokens from the cursor; `0` is the cursor itself.
    fn kind(&self, ahead: usize) -> SyntaxKind;

    /// The text of the token `ahead` of the cursor, as it is written in
    /// whatever file it came from.
    fn text(&self, ahead: usize) -> &str;

    /// Advances one token. At the end it stays there.
    fn bump(&mut self);

    /// Where the cursor is.
    fn at(&self) -> Position;

    /// Puts the cursor back where it was.
    fn seek(&mut self, to: Position);

    /// How many tokens the macro reference at the cursor covers, name and
    /// argument list together, or `None` if the cursor is not on one.
    ///
    /// Only the raw stream ever answers: on the expanded path a macro has
    /// already become the tokens it stands for, so there is no reference left
    /// to see. Which is the point -- a rule that handles a call correctly in
    /// raw mode does nothing at all in expanded mode, without asking why.
    fn macro_call(&self) -> Option<u32>;

    /// Whether the cursor is at the end.
    fn at_end(&self) -> bool {
        self.kind(0) == EOF
    }
}

/// Indices of the tokens a grammar rule can see, in order.
fn grammar_tokens(kinds: impl Iterator<Item = SyntaxKind>) -> Vec<u32> {
    kinds
        .enumerate()
        .filter(|(_, kind)| !kind.is_trivia())
        .map(|(at, _)| at as u32)
        .collect()
}

/// The stream as written: macros unexpanded, includes not followed, every
/// branch of every conditional present.
///
/// What the formatter reads. See `docs/preprocessor.md`.
pub struct Raw<'a> {
    input: Input<'a>,
    /// The raw index of each token a rule can see.
    grammar: Vec<u32>,
    at: u32,
    /// Where a macro reference starts, and how many tokens it covers -- both
    /// in grammar positions, because that is what a rule counts in.
    calls: FxHashMap<u32, u32>,
}

impl<'a> Raw<'a> {
    pub fn new(input: Input<'a>) -> Raw<'a> {
        let grammar = grammar_tokens(input.tokens.iter().map(|token| token.kind));
        let position = |raw: u32| grammar.binary_search(&raw).ok().map(|at| at as u32);

        let calls = scan(&input)
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Macro(reference) => {
                    let start = position(reference.tokens.start)?;
                    // The end is exclusive and may sit on trivia, so it is
                    // counted rather than looked up.
                    let len = grammar[start as usize..]
                        .iter()
                        .take_while(|&&raw| raw < reference.tokens.end)
                        .count() as u32;
                    Some((start, len))
                }
                Item::Directive(_) => None,
            })
            .collect();

        Raw {
            input,
            grammar,
            at: 0,
            calls,
        }
    }

    /// The raw index of the token `ahead` of the cursor, if there is one.
    fn raw(&self, ahead: usize) -> Option<u32> {
        self.grammar.get(self.at as usize + ahead).copied()
    }
}

impl Tokens for Raw<'_> {
    fn kind(&self, ahead: usize) -> SyntaxKind {
        self.raw(ahead).map_or(EOF, |raw| self.input.kind(raw))
    }

    fn text(&self, ahead: usize) -> &str {
        self.raw(ahead).map_or("", |raw| self.input.text(raw))
    }

    fn bump(&mut self) {
        if (self.at as usize) < self.grammar.len() {
            self.at += 1;
        }
    }

    fn at(&self) -> Position {
        Position(self.at)
    }

    fn seek(&mut self, to: Position) {
        self.at = to.0;
    }

    fn macro_call(&self) -> Option<u32> {
        self.calls.get(&self.at).copied()
    }
}

/// The stream a compiler would read: macros substituted, includes followed,
/// the branch the definitions select and no other.
pub struct Expanded<'a> {
    origins: &'a Origins,
    tokens: &'a [ExpandedToken],
    grammar: Vec<u32>,
    at: u32,
}

impl<'a> Expanded<'a> {
    pub fn new(origins: &'a Origins, tokens: &'a [ExpandedToken]) -> Expanded<'a> {
        Expanded {
            origins,
            grammar: grammar_tokens(tokens.iter().map(|token| token.kind)),
            tokens,
            at: 0,
        }
    }

    fn token(&self, ahead: usize) -> Option<&ExpandedToken> {
        let at = *self.grammar.get(self.at as usize + ahead)?;
        self.tokens.get(at as usize)
    }
}

impl Tokens for Expanded<'_> {
    fn kind(&self, ahead: usize) -> SyntaxKind {
        self.token(ahead).map_or(EOF, |token| token.kind)
    }

    fn text(&self, ahead: usize) -> &str {
        self.token(ahead)
            .map_or("", |token| self.origins.slice(token.origin.spelled))
    }

    fn bump(&mut self) {
        if (self.at as usize) < self.grammar.len() {
            self.at += 1;
        }
    }

    fn at(&self) -> Position {
        Position(self.at)
    }

    fn seek(&mut self, to: Position) {
        self.at = to.0;
    }

    /// Always `None`: an expansion leaves no reference behind.
    fn macro_call(&self) -> Option<u32> {
        None
    }
}
