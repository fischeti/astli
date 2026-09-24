//! Source text tokenization into a contiguous token stream.
//!
//! The lexer produces a stream of tokens covering the input text without gaps:
//! - Identifiers are checked against the keyword table and reclassified.
//! - Unrecognized bytes are emitted as [`LEX_ERROR`] tokens (adjacent error bytes are merged).
//! - Macro definitions track line continuations: inside a `` `define ``, a trailing `\` on a
//!   line comment is emitted as a [`LINE_CONTINUATION`].
//! - The stream terminates with an empty [`EOF`] token.

use logos::Logos;

use crate::keyword;
use crate::{KeywordVersion, SyntaxKind, SyntaxKind::*};

/// A source token consisting of a syntax kind and half-open byte range `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub start: u32,
    pub end: u32,
}

impl Token {
    /// Returns the source text slice corresponding to this token.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }

    /// Returns the length in bytes of this token.
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` if this token covers zero bytes (e.g. `EOF`).
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Lexer for tokenizing SystemVerilog source text.
pub struct Lexer<'a> {
    source: &'a str,
    version: KeywordVersion,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer for `source` with default keyword version settings.
    pub fn new(source: &'a str) -> Self {
        Lexer {
            source,
            version: KeywordVersion::default(),
        }
    }

    /// Creates a lexer with a specific [`KeywordVersion`].
    pub fn with_keyword_version(source: &'a str, version: KeywordVersion) -> Self {
        Lexer { source, version }
    }

    /// Returns the byte length of the newline sequence starting at byte offset `at`, if any.
    fn newline_at(&self, at: u32) -> Option<u32> {
        let rest = &self.source[at as usize..];
        if rest.starts_with("\r\n") {
            Some(2)
        } else if rest.starts_with('\n') {
            Some(1)
        } else {
            None
        }
    }

    /// Tokenizes the source text and returns all tokens including the terminating `EOF`.
    pub fn tokenize(&self) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut inner = SyntaxKind::lexer(self.source);
        let mut in_define = false;

        while let Some(result) = inner.next() {
            let span = inner.span();
            let (start, end) = (span.start as u32, span.end as u32);

            let kind = match result {
                Ok(IDENT) => keyword::lookup(inner.slice(), self.version).unwrap_or(IDENT),
                Ok(kind) => kind,
                Err(()) => {
                    // Merge adjacent unlexable bytes into a single error token.
                    if let Some(last) = tokens.last_mut()
                        && last.kind == LEX_ERROR
                        && last.end == start
                    {
                        last.end = end;
                        continue;
                    }
                    LEX_ERROR
                }
            };

            // In a `define` body, a trailing backslash on a line comment is treated
            // as a line continuation rather than comment text.
            if in_define
                && kind == LINE_COMMENT
                && self.source.as_bytes()[end as usize - 1] == b'\\'
                && let Some(newline) = self.newline_at(end)
            {
                tokens.push(Token {
                    kind: LINE_COMMENT,
                    start,
                    end: end - 1,
                });
                tokens.push(Token {
                    kind: LINE_CONTINUATION,
                    start: end - 1,
                    end: end + newline,
                });
                inner.bump(newline as usize);
                continue;
            }

            let token = Token { kind, start, end };
            in_define = match kind {
                // A macro definition ends at an uncontinued newline.
                WHITESPACE if token.text(self.source).contains('\n') => false,
                TICK_IDENT if token.text(self.source) == "`define" => true,
                _ => in_define,
            };
            tokens.push(token);
        }

        let len = self.source.len() as u32;
        tokens.push(Token {
            kind: EOF,
            start: len,
            end: len,
        });
        tokens
    }
}

/// Tokenizes `source` with the default reserved keyword set.
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).tokenize()
}
