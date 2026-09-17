//! Addressing a token once there is more than one file.
//!
//! A bare index identifies a token only while there is one slice to index it
//! into. `` `include `` makes a second, and the two then mix freely: a macro
//! defined in a header is substituted into the file that included it, so one
//! expansion reads a body from one file and the arguments from another.
//!
//! [`TokenId`] and [`TokenSpan`] are the token-space counterparts of
//! [`Span`], which answers the same question for bytes. Everything the
//! preprocessor records -- a definition's body, a reference's arguments, the
//! recursion guard -- is written in them rather than in `u32`, so that an
//! index can only be read against the tokens it was taken from.

use std::ops::Range;

use svirig_text::{FileId, Span};

use crate::{SyntaxKind, Token};

/// One token: which file, and where in that file's tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenId {
    pub file: FileId,
    pub index: u32,
}

impl TokenId {
    pub fn new(file: FileId, index: u32) -> TokenId {
        TokenId { file, index }
    }

    /// The span covering just this token.
    pub fn span(self) -> TokenSpan {
        TokenSpan::new(self.file, self.index, self.index + 1)
    }

    /// The bytes this token covers. `tokens` must be its own file's.
    pub fn bytes(self, tokens: &[Token]) -> Span {
        let token = tokens[self.index as usize];
        Span::new(self.file, token.start, token.end)
    }
}

/// A half-open range of tokens in one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenSpan {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl TokenSpan {
    pub fn new(file: FileId, start: u32, end: u32) -> TokenSpan {
        debug_assert!(start <= end, "a span may not run backwards");
        TokenSpan { file, start, end }
    }

    /// A span of no tokens, which is what an operand that is explicitly
    /// nothing comes back as.
    pub fn empty(file: FileId, at: u32) -> TokenSpan {
        TokenSpan::new(file, at, at)
    }

    /// The same range in the same file, narrowed to `range`.
    pub fn with(self, range: Range<u32>) -> TokenSpan {
        TokenSpan::new(self.file, range.start, range.end)
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The indices, for slicing the file's tokens.
    pub fn range(self) -> Range<u32> {
        self.start..self.end
    }

    pub fn at(self, index: u32) -> TokenId {
        TokenId::new(self.file, index)
    }

    pub fn iter(self) -> impl Iterator<Item = TokenId> {
        self.range().map(move |at| self.at(at))
    }

    /// The bytes the range covers, whatever lies between the tokens included.
    ///
    /// An empty range has no bytes, so it comes back as the point where they
    /// would have been rather than as the following token.
    pub fn bytes(self, tokens: &[Token]) -> Span {
        let at = (self.start as usize).min(tokens.len().saturating_sub(1));
        if self.is_empty() {
            return Span::point(self.file, tokens[at].start);
        }
        Span::new(
            self.file,
            tokens[at].start,
            tokens[self.end as usize - 1].end,
        )
    }
}

/// One file as the preprocessor reads it.
///
/// The three travel together everywhere below and are useless apart: the
/// tokens are what an index addresses, the text is what a token's bytes are
/// read out of, and the id is what makes the index mean something once a
/// second file exists.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    pub file: FileId,
    pub source: &'a str,
    pub tokens: &'a [Token],
}

impl<'a> Input<'a> {
    /// Built by the session, which is what holds a file's tokens; everything
    /// else is handed one.
    pub(crate) fn new(file: FileId, source: &'a str, tokens: &'a [Token]) -> Input<'a> {
        Input {
            file,
            source,
            tokens,
        }
    }

    pub fn len(&self) -> u32 {
        self.tokens.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn token(&self, at: u32) -> Token {
        self.tokens[at as usize]
    }

    pub fn kind(&self, at: u32) -> SyntaxKind {
        self.tokens[at as usize].kind
    }

    pub fn text(&self, at: u32) -> &'a str {
        self.tokens[at as usize].text(self.source)
    }

    pub fn id(&self, at: u32) -> TokenId {
        TokenId::new(self.file, at)
    }

    pub fn span(&self, range: Range<u32>) -> TokenSpan {
        TokenSpan::new(self.file, range.start, range.end)
    }
}
