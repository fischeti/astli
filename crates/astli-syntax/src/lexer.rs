//! Source text tokenization into a contiguous token stream.
//!
//! The lexer produces a stream of tokens covering the input text without gaps:
//! - Identifiers are checked against the keyword table and reclassified.
//! - Unrecognized bytes are emitted as [`LEX_ERROR`] tokens (adjacent error bytes are merged).
//! - Macro definitions track line continuations: inside a `` `define ``, a trailing `\` on a
//!   line comment is emitted as a [`LINE_CONTINUATION`].
//! - In a `` `define ``'s stringified text, a `\` and the character after it are a
//!   [`MACRO_STRING_ESCAPE`], as in a string.
//! - Between `table` and `endtable`, each symbol of a UDP's table is a token of its own.
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
        let mut line = Line::Code;
        // `` `begin_keywords "…" `` pushes the version it names, which
        // `` `end_keywords `` pops; one it does not define is read as the
        // version in force.
        let mut versions = vec![self.version];
        let mut naming = false;
        // In a UDP's table, which a directive's line suspends.
        let mut table = false;
        // Between a `` `define `` body's `` `" `` and the one closing it.
        let mut stringified = false;

        loop {
            if table && line == Line::Code {
                let at = inner.span().end;
                if let Some(kind) = (self.source.as_bytes().get(at)).and_then(|&byte| symbol(byte))
                {
                    tokens.push(Token {
                        kind,
                        start: at as u32,
                        end: at as u32 + 1,
                    });
                    inner.bump(1);
                    continue;
                }
            }
            let Some(result) = inner.next() else {
                break;
            };
            let span = inner.span();
            let (start, end) = (span.start as u32, span.end as u32);

            let kind = match result {
                Ok(IDENT) => {
                    let version = *versions.last().expect("the outermost version");
                    keyword::lookup(inner.slice(), version).unwrap_or(IDENT)
                }
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
            if line == Line::Define
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

            // In stringified text a `\` escapes the character after it, as in
            // a string, where elsewhere it would start an escaped name and
            // swallow the closing `` `" `` and the newline ending the line.
            if stringified && kind == ESCAPED_IDENT {
                let escaped = self.source[start as usize + 1..].chars().next();
                let end = end.min(start + 1 + escaped.map_or(0, |ch| ch.len_utf8() as u32));
                tokens.push(Token {
                    kind: MACRO_STRING_ESCAPE,
                    start,
                    end,
                });
                inner = SyntaxKind::lexer(self.source);
                inner.bump(end as usize);
                continue;
            }

            let token = Token { kind, start, end };
            let text = token.text(self.source);
            if naming && !kind.is_trivia() {
                naming = false;
                let named = (kind == STRING_LITERAL)
                    .then(|| KeywordVersion::from_specifier(text.trim_matches('"')))
                    .flatten();
                let current = *versions.last().expect("the outermost version");
                versions.push(named.unwrap_or(current));
            }
            match (kind, text) {
                (TICK_IDENT, "`begin_keywords") => naming = true,
                (TICK_IDENT, "`end_keywords") if versions.len() > 1 => {
                    versions.pop();
                }
                _ => {}
            }
            // A table a macro writes is lexed as ordinary code, since its
            // `endtable` may be in another macro or nowhere at all.
            match kind {
                TABLE_KW if line != Line::Define => table = true,
                ENDTABLE_KW => table = false,
                _ => {}
            }
            stringified = match kind {
                MACRO_QUOTE if line == Line::Define => !stringified,
                WHITESPACE if text.contains('\n') => false,
                _ => stringified,
            };
            line = match kind {
                // A line ends at a newline no `\` continues.
                WHITESPACE if text.contains('\n') => Line::Code,
                TICK_IDENT if text == "`define" => Line::Define,
                TICK_IDENT if line == Line::Code => Line::Directive,
                _ => line,
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

/// What the line being lexed is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Code,
    /// After a `` ` ``-name: a directive's operands, or a macro call's.
    Directive,
    /// A `` `define ``'s, which a `\` continues.
    Define,
}

/// The kind of the table symbol `byte` spells, if it spells one.
fn symbol(byte: u8) -> Option<SyntaxKind> {
    match byte {
        b'0' | b'1' | b'x' | b'X' | b'b' | b'B' | b'r' | b'R' | b'f' | b'F' | b'p' | b'P'
        | b'n' | b'N' => Some(UDP_SYMBOL),
        b'?' => Some(QUESTION),
        b'*' => Some(STAR),
        b'-' => Some(MINUS),
        _ => None,
    }
}

/// Tokenizes `source` with the default reserved keyword set.
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).tokenize()
}
