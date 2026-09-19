//! Parsing of SystemVerilog compiler directives.
//!
//! Identifiers beginning with a backtick (`` ` ``) are distinguished as either
//! compiler directives (matching one of the standard [`DirectiveType`] variants)
//! or user macro references. This module handles parsing directives and extracting
//! their operands.

use std::ops::Range;

use super::tokens::{Input, TokenId, TokenSpan};
use svirig_syntax::{SyntaxKind, SyntaxKind::*, Token};

/// Standard compiler directives defined in IEEE 1800-2023 §22.1.
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
    /// `` `__FILE__ ``: expands to the current file path.
    FileName,
    /// `` `__LINE__ ``: expands to the current line number.
    LineNumber,
}

impl DirectiveType {
    /// Matches directive text (including the leading backtick) to a known [`DirectiveType`].
    ///
    /// Returns `None` if the text does not match any recognized compiler directive,
    /// indicating it is a macro reference.
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

/// A parsed compiler directive instance in a token stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// The type of compiler directive.
    pub ty: DirectiveType,
    /// Token span covering the directive introducer and its operands (excluding trailing newline).
    pub tokens: TokenSpan,
    /// Parsed operands of the directive.
    pub operands: Operands,
}

/// Operands associated with a compiler directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operands {
    /// Operand for `` `define ``: macro definition with name, optional formals, and body.
    Define(MacroDef),
    /// Macro name operand for `` `undef ``, `` `ifdef ``, `` `ifndef ``, or `` `elsif ``.
    Name(TokenId),
    /// File path operand for `` `include ``.
    Include(IncludePath),
    /// Directives that take no operands (e.g. `` `else ``, `` `endif ``, `` `resetall ``).
    Bare,
    /// Directives whose operands are retained as unparsed token spans (e.g. `` `timescale ``).
    Unparsed(TokenSpan),
    /// Malformed directive missing required operands or with invalid syntax.
    Malformed,
}

/// Definition structure for a macro introduced by `` `define ``.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDef {
    /// Token span covering the entire `` `define `` directive.
    pub tokens: TokenSpan,
    /// Token identifier of the macro name.
    pub name: TokenId,
    /// Optional formal parameter list. `None` indicates an object-like macro with no parameter list.
    pub formals: Option<Vec<Formal>>,
    /// Token span of the macro substitution body.
    pub body: TokenSpan,
}

/// A formal parameter in a macro definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formal {
    /// Parameter name token.
    pub name: TokenId,
    /// Optional default value token span.
    pub default: Option<TokenSpan>,
}

/// Path representation for an `` `include `` directive target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludePath {
    /// Quoted file path: `` `include "filename" ``.
    Quoted(TokenId),
    /// Angle-bracket file path: `` `include <filename> ``.
    Angle(TokenSpan),
    /// Dynamic file path produced via macro expansion: `` `include `HEADER ``.
    Expanded(TokenSpan),
}

/// Returns the index of the first non-trivia token in `range`.
pub(crate) fn significant(tokens: &[Token], range: Range<u32>) -> Option<u32> {
    range.into_iter().find(|&at| {
        let kind = tokens[at as usize].kind;
        !kind.is_trivia() && kind != LINE_CONTINUATION && kind != EOF
    })
}

/// Checks whether `token` represents a newline or end-of-file terminating a directive line.
fn ends_line(token: Token, source: &str) -> bool {
    match token.kind {
        WHITESPACE => token.text(source).contains('\n'),
        EOF => true,
        _ => false,
    }
}

/// Trims leading and trailing trivia tokens from `range`.
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

/// Finds the token index ending the line starting from `from`.
pub(crate) fn end_of_line(input: &Input, from: u32) -> u32 {
    (from..input.len())
        .find(|&at| ends_line(input.token(at), input.source))
        .unwrap_or(input.len())
}

/// Parses the directive at token index `at`.
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

/// Parses a `` `define `` directive at `at`.
fn parse_define(input: &Input, at: u32) -> (Operands, u32) {
    let end = end_of_line(input, at + 1);

    let Some(name) = significant(input.tokens, at + 1..end) else {
        return (Operands::Malformed, end);
    };

    // An opening parenthesis must immediately follow the macro name without intervening
    // whitespace to be treated as a formal parameter list (IEEE 1800-2023 §22.5.1).
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

/// Parses the formal parameter list of a macro definition starting at `at` (the opening `(`).
fn parse_formals(input: &Input, at: u32, end: u32) -> (Vec<Formal>, u32) {
    let mut formals = Vec::new();
    let mut cursor = at + 1;
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

    (formals, (cursor + 1).min(end))
}

/// Parses an `` `include `` directive operand at `at`.
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
        kind => match expanded_name(input, first, kind, line) {
            Some(end) => (
                Operands::Include(IncludePath::Expanded(input.span(first..end))),
                end,
            ),
            None => (Operands::Malformed, line),
        },
    }
}

/// Resolves the token span for an included filename generated via macro expansion.
fn expanded_name(input: &Input, first: u32, kind: SyntaxKind, line: u32) -> Option<u32> {
    match kind {
        MACRO_QUOTE => Some(
            (first + 1..line)
                .find(|&at| input.kind(at) == MACRO_QUOTE)
                .map_or(line, |close| close + 1),
        ),
        TICK_IDENT | IDENT | ESCAPED_IDENT => Some(first + 1),
        kind if kind.is_keyword() => Some(first + 1),
        _ => None,
    }
}
