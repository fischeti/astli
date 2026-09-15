//! Finding and parsing compiler directives in the token stream.
//!
//! The lexer gives every `` `name `` the same [`TICK_IDENT`] kind,
//! because which one it is comes from the text. This is where that text is
//! read, and where the answer splits two ways: a name in the closed set of
//! [`DirectiveType`] is a directive, and every other name is a reference to a
//! macro. The second case is much the more common, and it is deliberately *not*
//! handled here: a reference's arguments cannot be delimited without knowing
//! whether the macro takes any, which is the macro table's business.
//!
//! # Extents
//!
//! Most directives end where their operands do, and 1800-2023 22.2 allows
//! ordinary code to follow on the same line. Only `` `define `` and
//! `` `pragma `` run to the end of the line, and only a `` `define `` continues
//! past one, through a `\` immediately before the newline.
//!
//! [`Operands::Unparsed`] takes the end of the line as its extent whether or
//! not the directive's syntax has a defined end. That over-claims for
//! `` `timescale 1ns / 1ps `` followed by code on the same line, which no real
//! source does and which nothing yet reads.

use std::ops::Range;

use super::tokens::{Input, TokenId, TokenSpan};
use crate::{SyntaxKind, SyntaxKind::*, Token};

/// The compiler directives of 1800-2023 22.1.
///
/// The set is closed: a macro may not be named after a directive (22.5.1), so
/// any other `` `name `` is a macro reference. Tools define directives of their
/// own outside the standard, and those are read here as macro references --
/// harmless for a formatter, which reproduces them either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirectiveType {
    Define,
    Undef,
    UndefineAll,
    Ifdef,
    Ifndef,
    Elsif,
    Else,
    Endif,
    Include,
    Timescale,
    DefaultNettype,
    UnconnectedDrive,
    NoUnconnectedDrive,
    CellDefine,
    EndCellDefine,
    Resetall,
    Line,
    BeginKeywords,
    EndKeywords,
    Pragma,
    /// `` `__FILE__ ``, which expands to the current file name.
    FileName,
    /// `` `__LINE__ ``, which expands to the current line number.
    LineNumber,
}

impl DirectiveType {
    /// Reads a [`TICK_IDENT`] token's text, backtick included.
    ///
    /// `None` means the name is a macro reference.
    pub fn lookup(text: &str) -> Option<DirectiveType> {
        use DirectiveType::*;

        Some(match text {
            "`define" => Define,
            "`undef" => Undef,
            "`undefineall" => UndefineAll,
            "`ifdef" => Ifdef,
            "`ifndef" => Ifndef,
            "`elsif" => Elsif,
            "`else" => Else,
            "`endif" => Endif,
            "`include" => Include,
            "`timescale" => Timescale,
            "`default_nettype" => DefaultNettype,
            "`unconnected_drive" => UnconnectedDrive,
            "`nounconnected_drive" => NoUnconnectedDrive,
            "`celldefine" => CellDefine,
            "`endcelldefine" => EndCellDefine,
            "`resetall" => Resetall,
            "`line" => Line,
            "`begin_keywords" => BeginKeywords,
            "`end_keywords" => EndKeywords,
            "`pragma" => Pragma,
            "`__FILE__" => FileName,
            "`__LINE__" => LineNumber,
            _ => return None,
        })
    }
}

/// One directive occurrence.
///
/// Every span in here and below addresses *tokens*, not source bytes; byte
/// offsets live on [`Token`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub ty: DirectiveType,
    /// The tokens the directive covers, introducer included and the newline
    /// that ends it excluded.
    pub tokens: TokenSpan,
    pub operands: Operands,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operands {
    /// `` `define ``, the only directive whose operands anything expands.
    Define(MacroDef),
    /// A single macro name: `` `undef ``, `` `ifdef ``, `` `ifndef ``,
    /// `` `elsif ``.
    Name(TokenId),
    /// `` `include ``.
    Include(IncludePath),
    /// The introducer is the whole directive: `` `else ``, `` `endif ``,
    /// `` `resetall ``, `` `__LINE__ ``, and the rest of the bare ones.
    Bare,
    /// Operands nothing reads yet, left as tokens: `` `timescale ``,
    /// `` `pragma ``, `` `line ``, `` `default_nettype ``, and the two keyword
    /// directives. Empty when the directive has none.
    Unparsed(TokenSpan),
    /// A directive that does not have the operands it requires -- an
    /// `` `ifdef `` at end of file, a `` `define `` with no name. Recorded
    /// rather than dropped, so that the tokens still round-trip and a
    /// diagnostic can be hung on them once there is a diagnostics layer.
    Malformed,
}

/// A macro definition, in the pieces expansion needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDef {
    /// The whole `` `define ``, introducer included. The table keeps this so
    /// that an expansion can point a message at the definition it substituted;
    /// the body alone would point inside it.
    pub tokens: TokenSpan,
    /// The token holding the macro's name.
    pub name: TokenId,
    /// The formal arguments. `None` when the macro takes no argument list,
    /// which is not the same as taking an empty one: `` `define A() `` may be
    /// invoked as `` `A() `` and `` `define A `` may not.
    pub formals: Option<Vec<Formal>>,
    /// The substitution text, which is text and must never be reformatted.
    pub body: TokenSpan,
}

/// One formal argument of a macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formal {
    pub name: TokenId,
    /// The default's tokens, `1` in `` `define A(x = 1) ``. `Some` but empty
    /// where the default is explicitly nothing, as in `` `define A(x =) ``,
    /// which 22.5.1 allows and which differs from having no default at all.
    pub default: Option<TokenSpan>,
}

/// Where an `` `include `` gets its file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludePath {
    /// `` `include "f.svh" `` -- searched for relative to the including file
    /// and then along the include path.
    Quoted(TokenId),
    /// `` `include <f.svh> `` -- searched for only where the implementation
    /// keeps the files the standard defines. `<` and `>` are ordinary
    /// operators to the lexer, so the name is however many tokens lie between
    /// them.
    Angle(TokenSpan),
    /// `` `include `PATH `` -- the name arrives by expansion (22.2), so it
    /// cannot be resolved before the macro table is built. Illegal by 22.4's
    /// syntax and used in the wild anyway. This is also how an `` `include ``
    /// written inside a macro body names its file, since there the name is a
    /// formal until the body is substituted.
    Expanded(TokenSpan),
}

/// The first token in `range` that carries meaning: not whitespace, not a
/// comment, and not a line continuation, which is trivia for reading purposes
/// even though the tree keeps it.
///
/// A directive's operands are always searched for within one line. No directive
/// takes an operand from the line below, and searching past one would turn a
/// directive missing its operand into a directive that stole the next
/// statement's. A macro reference is under no such restriction: its argument
/// list may span as many lines as it likes.
pub(crate) fn significant(tokens: &[Token], range: Range<u32>) -> Option<u32> {
    range.into_iter().find(|&at| {
        let kind = tokens[at as usize].kind;
        !kind.is_trivia() && kind != LINE_CONTINUATION && kind != EOF
    })
}

/// Whether a token ends the line, and so a directive.
///
/// A line continuation does not, and neither does a newline inside a block
/// comment or a string -- both of which are one token, so they never reach the
/// whitespace test (22.5.1).
fn ends_line(token: Token, source: &str) -> bool {
    match token.kind {
        WHITESPACE => token.text(source).contains('\n'),
        EOF => true,
        _ => false,
    }
}

/// `range` with leading and trailing trivia dropped.
///
/// Operand spans are reported trimmed. Trivia around an operand is not part of
/// it -- a comment after a macro body is not substituted -- and the untrimmed
/// bytes are still reachable through the directive's own token range.
pub(crate) fn trim(tokens: &[Token], range: Range<u32>) -> Range<u32> {
    let Some(start) = significant(tokens, range.clone()) else {
        return range.start..range.start;
    };
    let last = range
        .rev()
        .find(|&at| significant(tokens, at..at + 1).is_some())
        .unwrap_or(start);
    start..last + 1
}

/// The index of the token that ends the line `from` is on.
pub(crate) fn end_of_line(input: &Input, from: u32) -> u32 {
    (from..input.len())
        .find(|&at| ends_line(input.token(at), input.source))
        .unwrap_or(input.len())
}

/// Reads the directive introduced at `at`, whose name has already been looked
/// up.
pub(crate) fn parse(name: DirectiveType, input: &Input, at: u32) -> Directive {
    use DirectiveType::*;

    let (operands, end) = match name {
        Define => parse_define(input, at),
        Undef | Ifdef | Ifndef | Elsif => {
            let line = end_of_line(input, at + 1);
            match significant(input.tokens, at + 1..line) {
                Some(name) => (Operands::Name(input.id(name)), name + 1),
                None => (Operands::Malformed, line),
            }
        }
        Include => parse_include(input, at),
        UndefineAll | Else | Endif | CellDefine | EndCellDefine | Resetall | EndKeywords
        | FileName | LineNumber => (Operands::Bare, at + 1),
        Timescale | DefaultNettype | UnconnectedDrive | NoUnconnectedDrive | Line
        | BeginKeywords | Pragma => {
            let end = end_of_line(input, at + 1);
            (
                Operands::Unparsed(input.span(trim(input.tokens, at + 1..end))),
                end,
            )
        }
    };

    Directive {
        ty: name,
        tokens: input.span(at..end),
        operands,
    }
}

/// `` `define text_macro_name macro_text `` (22.5.1).
fn parse_define(input: &Input, at: u32) -> (Operands, u32) {
    let end = end_of_line(input, at + 1);

    let Some(name) = significant(input.tokens, at + 1..end) else {
        return (Operands::Malformed, end);
    };

    // The argument list must touch the name. For a simple identifier that means
    // no space at all; for an escaped one, exactly the single space that
    // terminates it -- and since that space is part of the token, one test
    // covers both.
    let after_name = name + 1;
    let (formals, body) = match input.tokens.get(after_name as usize) {
        Some(token) if token.kind == L_PAREN && token.start == input.token(name).end => {
            let (formals, after) = parse_formals(input, after_name, end);
            (Some(formals), after)
        }
        _ => (None, after_name),
    };

    let define = MacroDef {
        tokens: input.span(at..end),
        name: input.id(name),
        formals,
        body: input.span(trim(input.tokens, body.min(end)..end)),
    };
    (Operands::Define(define), end)
}

/// The formal argument list, `at` being its `(`. Returns the arguments and the
/// index just past the closing `)`.
fn parse_formals(input: &Input, at: u32, end: u32) -> (Vec<Formal>, u32) {
    let mut formals = Vec::new();
    let mut cursor = at + 1;
    // A default may hold parentheses of its own -- `ARGS = ()` occurs in the
    // corpus -- so the list ends at a balanced `)`, not the first one.
    let mut depth = 1u32;
    let mut current: Option<Formal> = None;
    let mut default_from = None;

    while cursor < end {
        let kind = input.kind(cursor);
        let outermost = depth == 1;

        match kind {
            L_PAREN => depth += 1,
            R_PAREN if outermost => break,
            R_PAREN => depth -= 1,
            COMMA if outermost => {
                if let Some(mut formal) = current.take() {
                    if let Some(from) = default_from.take() {
                        formal.default = Some(input.span(trim(input.tokens, from..cursor)));
                    }
                    formals.push(formal);
                }
            }
            EQ if outermost && current.is_some() && default_from.is_none() => {
                default_from = Some(cursor + 1);
            }
            _ if kind.is_trivia() || kind == LINE_CONTINUATION => {}
            // The first name at this depth opens an argument; anything after it
            // is part of a default, which 22.5.1 leaves as arbitrary text.
            _ if outermost && current.is_none() => {
                current = Some(Formal {
                    name: input.id(cursor),
                    default: None,
                });
            }
            _ => {}
        }
        cursor += 1;
    }

    if let Some(mut formal) = current.take() {
        if let Some(from) = default_from {
            formal.default = Some(input.span(trim(input.tokens, from..cursor)));
        }
        formals.push(formal);
    }

    // Past the `)`, or past the end of the line if there is not one.
    (formals, (cursor + 1).min(end))
}

/// `` `include " filename " `` or `` `include < filename > `` (22.4), plus the
/// macro-valued form the standard does not describe.
fn parse_include(input: &Input, at: u32) -> (Operands, u32) {
    let line = end_of_line(input, at + 1);
    let Some(first) = significant(input.tokens, at + 1..line) else {
        return (Operands::Malformed, line);
    };
    match input.kind(first) {
        STRING_LITERAL => (
            Operands::Include(IncludePath::Quoted(input.id(first))),
            first + 1,
        ),
        LT => {
            let close = (first + 1..line).find(|&at| input.kind(at) == GT);
            match close {
                Some(close) => (
                    Operands::Include(IncludePath::Angle(
                        input.span(trim(input.tokens, first + 1..close)),
                    )),
                    close + 1,
                ),
                None => (Operands::Malformed, line),
            }
        }
        // Anything else has to expand before the include can resolve.
        kind => match expanded_name(input, first, kind, line) {
            Some(end) => (
                Operands::Include(IncludePath::Expanded(input.span(first..end))),
                end,
            ),
            None => (Operands::Malformed, line),
        },
    }
}

/// Where an `` `include `` name that arrives by expansion ends, or `None` if
/// the operand cannot become a name at all.
///
/// One token, except for a stringification, which runs to its closing quote.
/// Reading further would swallow whatever the text around it does next: a macro
/// body that puts an `` `include `` between an `` `ifdef `` and an `` `endif ``
/// is the ordinary way of writing a conditional include, and the `` `endif ``
/// is not part of the file name.
///
/// A macro reference with an argument list is therefore read as its name alone.
/// Delimiting the list needs the table, which is not here, and no such include
/// occurs in the corpus.
fn expanded_name(input: &Input, first: u32, kind: SyntaxKind, line: u32) -> Option<u32> {
    match kind {
        MACRO_QUOTE => Some(
            (first + 1..line)
                .find(|&at| input.kind(at) == MACRO_QUOTE)
                .map_or(line, |close| close + 1),
        ),
        // A macro standing in for the name, or -- inside a macro body -- a
        // formal, which is an identifier until the body is substituted. A
        // formal may be spelled as a keyword, which the lexer has already
        // reclassified.
        TICK_IDENT | IDENT | ESCAPED_IDENT => Some(first + 1),
        kind if kind.is_keyword() => Some(first + 1),
        _ => None,
    }
}
