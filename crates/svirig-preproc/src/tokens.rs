//! Token addressing across multiple files.
//!
//! When directives such as `` `include `` are evaluated, tokens originate from
//! multiple files. [`TokenId`] and [`TokenSpan`] provide file-qualified token
//! coordinates corresponding to [`Span`] in byte space.

use std::ops::Range;

use svirig_text::{SourceId, Span};

use svirig_syntax::{SyntaxKind, Token};

/// Unique identifier for a token within a specific file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenId {
    pub file: SourceId,
    pub index: u32,
}

impl TokenId {
    /// Creates a new token identifier for `index` in `file`.
    pub fn new(file: SourceId, index: u32) -> TokenId {
        TokenId { file, index }
    }

    /// Returns a [`TokenSpan`] covering only this token.
    pub fn span(self) -> TokenSpan {
        TokenSpan::new(self.file, self.index, self.index + 1)
    }

    /// Returns the byte [`Span`] of this token within its file's `tokens` slice.
    pub fn bytes(self, tokens: &[Token]) -> Span {
        let token = tokens[self.index as usize];
        Span::new(self.file, token.start, token.end)
    }
}

/// Half-open range of token indices `[start, end)` within a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenSpan {
    pub file: SourceId,
    pub start: u32,
    pub end: u32,
}

impl TokenSpan {
    /// Creates a new token span.
    pub fn new(file: SourceId, start: u32, end: u32) -> TokenSpan {
        debug_assert!(start <= end, "a span may not run backwards");
        TokenSpan { file, start, end }
    }

    /// Creates an empty token span at token index `at`.
    pub fn empty(file: SourceId, at: u32) -> TokenSpan {
        TokenSpan::new(file, at, at)
    }

    /// Returns a span in the same file narrowed to `range`.
    pub fn with(self, range: Range<u32>) -> TokenSpan {
        TokenSpan::new(self.file, range.start, range.end)
    }

    /// Returns the number of tokens in this span.
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` if this span covers zero tokens.
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns the half-open index range of this span.
    pub fn range(self) -> Range<u32> {
        self.start..self.end
    }

    /// Returns the [`TokenId`] at relative or absolute index `index`.
    pub fn at(self, index: u32) -> TokenId {
        TokenId::new(self.file, index)
    }

    /// Iterates over all [`TokenId`]s in this span.
    pub fn iter(self) -> impl Iterator<Item = TokenId> {
        self.range().map(move |at| self.at(at))
    }

    /// Returns the byte [`Span`] covering all tokens in this range.
    ///
    /// For empty ranges, returns a zero-width point at the start token offset.
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

/// View of a single file combining its [`SourceId`], source text, and token stream.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    pub file: SourceId,
    pub source: &'a str,
    pub tokens: &'a [Token],
}

impl<'a> Input<'a> {
    pub(crate) fn new(file: SourceId, source: &'a str, tokens: &'a [Token]) -> Input<'a> {
        Input {
            file,
            source,
            tokens,
        }
    }

    /// Returns the total number of tokens in the file.
    pub fn len(&self) -> u32 {
        self.tokens.len() as u32
    }

    /// Returns `true` if the token stream is empty.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Returns the token at index `at`.
    pub fn token(&self, at: u32) -> Token {
        self.tokens[at as usize]
    }

    /// Returns the syntax kind of the token at index `at`.
    pub fn kind(&self, at: u32) -> SyntaxKind {
        self.tokens[at as usize].kind
    }

    /// Returns the source text slice of the token at index `at`.
    pub fn text(&self, at: u32) -> &'a str {
        self.tokens[at as usize].text(self.source)
    }

    /// Returns the [`TokenId`] for index `at`.
    pub fn id(&self, at: u32) -> TokenId {
        TokenId::new(self.file, at)
    }

    /// Returns a [`TokenSpan`] for the given token range.
    pub fn span(&self, range: Range<u32>) -> TokenSpan {
        TokenSpan::new(self.file, range.start, range.end)
    }
}
