//! Parsing rules for preprocessor constructs: directives, macro references, and conditional blocks.
//!
//! Preprocessor tokens are classified and shaped by `astli-preproc` during lexing,
//! and are transformed into concrete syntax nodes here.
//!
//! Macro calls and compiler directives can appear at any syntactic boundary
//! (items, statements, expressions, types). Balanced conditional regions are parsed
//! according to their enclosing syntactic context, whereas ragged regions (where delimiters
//! cross branch boundaries) are handled with bounded fallback rules.

use super::Parser;
use super::decl::declaration;
use super::source::{DirectiveShape, Position, RegionShape, Tokens};
use super::verbatim::{Context, verbatim};
use astli_preproc::DirectiveType;
use astli_syntax::SyntaxKind::*;

/// Parses a preprocessor construct at the cursor if one is present.
///
/// Returns `true` if a macro reference, directive, or conditional region was consumed.
/// Returns `false` without consuming tokens if the cursor is not on a preprocessor construct.
pub fn any<T: Tokens>(parser: &mut Parser<T>) -> bool {
    if let Some(len) = parser.macro_call() {
        macro_call(parser, len);
        return true;
    }

    let Some(shape) = parser.directive() else {
        return false;
    };

    if matches!(shape.ty, DirectiveType::Ifdef | DirectiveType::Ifndef)
        && let Some(shape) = parser.region()
    {
        region(parser, shape);
        return true;
    }

    directive(parser, shape);
    true
}

/// Parses a macro reference and its optional argument list.
fn macro_call<T: Tokens>(parser: &mut Parser<T>, len: u32) {
    let marker = parser.start();
    let end = parser.ahead(len);

    parser.bump();
    if parser.position() < end && parser.at(L_PAREN) {
        arguments(parser, end);
    }

    parser.complete(marker, MACRO_CALL);
}

/// Parses macro call arguments up to position `end`.
fn arguments<T: Tokens>(parser: &mut Parser<T>, end: Position) {
    let list = parser.start();
    parser.bump();

    let mut depth = 0u32;
    let mut argument = parser.start();

    while parser.position() < end {
        match parser.kind(0) {
            L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE => depth += 1,
            R_PAREN if depth == 0 => break,
            R_PAREN | R_BRACK | R_BRACE => depth = depth.saturating_sub(1),
            COMMA if depth == 0 => {
                parser.complete(argument, MACRO_ARG);
                parser.bump();
                argument = parser.start();
                continue;
            }
            _ => {}
        }
        parser.bump();
    }

    parser.complete(argument, MACRO_ARG);

    if parser.position() < end {
        parser.bump();
    }
    parser.complete(list, MACRO_ARG_LIST);
}

/// Parses a compiler directive and its operand tokens.
fn directive<T: Tokens>(parser: &mut Parser<T>, shape: DirectiveShape) {
    let marker = parser.start();
    let body = shape.body.unwrap_or(shape.len..shape.len);

    take(parser, body.start);
    if body.start < body.end {
        let text = parser.start();
        take(parser, body.end - body.start);
        parser.complete(text, MACRO_BODY);
    }
    take(parser, shape.len.saturating_sub(body.end));

    parser.complete(marker, DIRECTIVE);
}

/// Parses a conditional compilation region and each of its constituent branches.
fn region<T: Tokens>(parser: &mut Parser<T>, shape: RegionShape) {
    let marker = parser.start();
    let mut taken = 0;

    for branch in &shape.branches {
        let open = parser.start();
        take(parser, branch.directive);
        body(parser, branch.body, shape.live);
        parser.complete(open, CONDITIONAL_BRANCH);
        taken += branch.directive + branch.body;
    }

    take(parser, shape.len.saturating_sub(taken));

    parser.complete(marker, CONDITIONAL_REGION);
}

/// Parses the body of a conditional branch up to `len` tokens.
fn body<T: Tokens>(parser: &mut Parser<T>, len: u32, live: bool) {
    let end = parser.ahead(len);
    while !parser.at_end() && parser.position() < end {
        let at = parser.position();
        match live {
            true => super::any(parser, Some(end)),
            false => ragged(parser, Some(end)),
        }
        if parser.position() == at {
            break;
        }
    }
}

/// Fallback parser for ragged conditional branches whose delimiters do not balance locally.
fn ragged<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && any(parser) {
        return;
    }
    if declaration(parser).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated, limit);
}

/// Consumes `len` tokens verbatim.
fn take<T: Tokens>(parser: &mut Parser<T>, len: u32) {
    for _ in 0..len {
        parser.bump();
    }
}
