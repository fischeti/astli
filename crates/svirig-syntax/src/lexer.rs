//! Turning source text into a flat, gapless token stream.
//!
//! Four things happen here that the `logos` rules in [`crate::kind`] cannot do
//! on their own:
//!
//! * An [`IDENT`] is looked up in [`crate::keyword`] and
//!   reclassified if it is a reserved word.
//! * A byte no rule matches becomes a [`LEX_ERROR`] token rather
//!   than a hole. Adjacent unlexable bytes are merged into one.
//! * Inside a `` `define ``, a line comment gives up a trailing `\` to a
//!   [`LINE_CONTINUATION`]. See [Macro bodies](#macro-bodies).
//! * The stream is terminated by an empty [`EOF`].
//!
//! # Gaplessness
//!
//! Every byte of the input belongs to exactly one token, in order, and no token
//! is empty except `EOF`. [`Lexer::tokenize`]'s output can therefore be
//! concatenated back into the input, which is the property everything
//! downstream leans on and which `tests/lexer.rs` checks over the whole corpus.
//!
//! # Macro bodies
//!
//! A `` `define `` body is substitution text rather than SystemVerilog, but
//! nearly all of it lexes the same either way, and its tokens are what the
//! preprocessor substitutes into. So it is lexed like anything else rather than
//! held as one opaque span.
//!
//! One rule genuinely differs. A definition ends at the first newline not
//! continued with `\`, and a line comment runs to the end of its line -- so a
//! comment swallows the `\` that was there to continue the definition, and it
//! ends a line early. That is not a corner case: it is how a long macro gets
//! commented, and real code does it. The continuation wins, and the comment
//! stops in front of it.
//!
//! Applying that rule needs the *extent* of a definition and nothing else, so
//! that is all the lexer tracks: from a `` `define `` to the first newline it
//! does not continue. Where the name ends and whether a `(` opens a formal list
//! decides a macro's arity, not how anything lexes, and belongs to the macro
//! table. A newline inside a block comment does not end a definition (22.5.1),
//! which falls out for free: a comment is one token, so it is not the
//! whitespace the extent stops at.

use logos::Logos;

use crate::keyword;
use crate::{KeywordVersion, SyntaxKind, SyntaxKind::*};

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

    /// The length of the newline directly after `at`, if there is one.
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

    /// Lexes the whole input, `EOF` included.
    pub fn tokenize(&self) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut inner = SyntaxKind::lexer(self.source);
        // Whether we are between a `` `define `` and the newline that ends it.
        let mut in_define = false;

        while let Some(result) = inner.next() {
            let span = inner.span();
            let (start, end) = (span.start as u32, span.end as u32);

            let kind = match result {
                Ok(IDENT) => keyword::lookup(inner.slice(), self.version).unwrap_or(IDENT),
                Ok(kind) => kind,
                Err(()) => {
                    // Merge into the previous error token if they touch, so a
                    // run of unlexable bytes is reported once rather than per
                    // byte.
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

            // A `\` ending a comment inside a `define` belongs to the
            // continuation, not to the comment. Hand it back, and take the
            // newline with it so the definition carries on to the next line.
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
                // A definition runs to the first newline it does not continue,
                // and a continuation is its own token rather than whitespace.
                WHITESPACE if token.text(self.source).contains('\n') => false,
                DIRECTIVE if token.text(self.source) == "`define" => true,
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

/// Lexes `source` with the default reserved word set.
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).tokenize()
}
