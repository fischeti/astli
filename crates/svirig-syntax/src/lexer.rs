//! Turning source text into a flat, gapless token stream.
//!
//! Three things happen here that the `logos` rules in [`crate::kind`] cannot do
//! on their own:
//!
//! * An [`SyntaxKind::IDENT`] is looked up in [`crate::keyword`] and
//!   reclassified if it is a reserved word.
//! * A byte no rule matches becomes a [`SyntaxKind::LEX_ERROR`] token rather
//!   than a hole. Adjacent unlexable bytes are merged into one.
//! * The stream is terminated by an empty [`SyntaxKind::EOF`].
//!
//! # Gaplessness
//!
//! Every byte of the input belongs to exactly one token, in order, and no token
//! is empty except `EOF`. [`Lexer::tokenize`]'s output can therefore be
//! concatenated back into the input, which is the property everything
//! downstream leans on and which `tests/lexer.rs` checks over the whole corpus.

use logos::Logos;

use crate::keyword;
use crate::{KeywordVersion, SyntaxKind};

/// A token: a kind and the half-open byte range it covers.
///
/// The text is not carried. Tokens are only ever read alongside the source they
/// came from, and a range keeps them `Copy` and cheap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub start: u32,
    pub end: u32,
}

impl Token {
    /// The source text this token covers.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Splits source text into tokens.
pub struct Lexer<'a> {
    source: &'a str,
    version: KeywordVersion,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Lexer {
            source,
            version: KeywordVersion::default(),
        }
    }

    /// Reads the source with a specific reserved word set.
    ///
    /// Only one set is implemented; see `docs/limitations.md`.
    pub fn with_keyword_version(source: &'a str, version: KeywordVersion) -> Self {
        Lexer { source, version }
    }

    /// Lexes the whole input, `EOF` included.
    pub fn tokenize(&self) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut inner = SyntaxKind::lexer(self.source);

        while let Some(result) = inner.next() {
            let span = inner.span();
            let (start, end) = (span.start as u32, span.end as u32);

            let kind = match result {
                Ok(SyntaxKind::IDENT) => {
                    keyword::lookup(inner.slice(), self.version).unwrap_or(SyntaxKind::IDENT)
                }
                Ok(kind) => kind,
                Err(()) => {
                    // Merge into the previous error token if they touch, so a
                    // run of unlexable bytes is reported once rather than per
                    // byte.
                    if let Some(last) = tokens.last_mut()
                        && last.kind == SyntaxKind::LEX_ERROR
                        && last.end == start
                    {
                        last.end = end;
                        continue;
                    }
                    SyntaxKind::LEX_ERROR
                }
            };

            tokens.push(Token { kind, start, end });
        }

        let len = self.source.len() as u32;
        tokens.push(Token {
            kind: SyntaxKind::EOF,
            start: len,
            end: len,
        });
        tokens
    }
}

/// Lexes `source` with the default reserved word set.
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).tokenize()
}
