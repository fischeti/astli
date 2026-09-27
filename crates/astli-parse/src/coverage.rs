//! Parsing rules for functional coverage: `covergroup`, its cover points and
//! crosses, and their bins.

use super::decl::data_type;
use super::decl::semicolon;
use super::event::{Completed, Marker};
use super::expr::{attributes, expr, range_list, with_clause};
use super::source::{Position, Tokens};
use super::stmt::{condition, statement, timing_control};
use super::verbatim::{Context, verbatim};
use super::{Parser, Scope, Snapshot, preprocessor};
use astli_syntax::SyntaxKind::*;

/// Parses `covergroup`, its header, what it covers, and `endgroup`; at the
/// cursor, with `marker` open.
pub(super) fn covergroup<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    // `covergroup extends base;` adds to the covergroup of a base class.
    if parser.at(EXTENDS_KW) {
        parser.bump();
    }
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return decline(parser, marker, before);
    }
    parser.bump();
    if parser.at(L_PAREN) {
        super::item::port_list(parser);
    }
    if parser.at(AT) {
        timing_control(parser);
    } else if parser.at(AT_AT) {
        // `@@(begin f)`: sampled as a block or a method starts or ends.
        parser.bump();
        condition(parser);
    } else if parser.at(WITH_KW) && parser.kind(1) == FUNCTION_KW {
        parser.bump();
        parser.bump();
        if parser.at(IDENT) {
            parser.bump();
        }
        if parser.at(L_PAREN) {
            super::item::port_list(parser);
        }
    }
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }

    let scope = parser.set_scope(Scope::Coverage);
    while !parser.at_end() && !parser.at(ENDGROUP_KW) {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }
        let at = parser.position();
        item(parser, limit);
        if parser.position() == at {
            break;
        }
    }
    parser.set_scope(scope);

    if !parser.at(ENDGROUP_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    super::stmt::label(parser);
    Some(parser.complete(marker, COVERGROUP_DECL))
}

/// Parses one cover point, cross, set of bins or option, falling back to
/// verbatim recovery if no rule matches.
pub(super) fn item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && !preprocessor::continued(parser) && preprocessor::any(parser) {
        return;
    }
    // A cross may declare functions its bins select with.
    if parser.at(FUNCTION_KW) {
        super::item::item(parser, limit);
        return;
    }
    let before = parser.snapshot();
    let marker = parser.start();
    attributes(parser);
    let taken = match parser.kind(0) {
        WILDCARD_KW | BINS_KW | ILLEGAL_BINS_KW | IGNORE_BINS_KW => bins(parser, marker, before),
        _ if is_point_or_cross(parser) => point_or_cross(parser, marker, before, limit),
        _ => {
            parser.abandon(marker);
            parser.rollback(before);
            // Options are assignments, as `option.weight = 2;`.
            statement(parser, limit);
            return;
        }
    };
    if taken.is_none() {
        verbatim(parser, Context::Terminated, limit);
    }
}

/// Whether a cover point or a cross starts at the cursor: `coverpoint` or
/// `cross` comes before any `;`, `{` or `=`, after a label and, for a cover
/// point, the label's type.
fn is_point_or_cross<T: Tokens>(parser: &Parser<T>) -> bool {
    let mut ahead = 0;
    loop {
        match parser.kind(ahead) {
            COVERPOINT_KW | CROSS_KW => return true,
            SEMICOLON | L_BRACE | EQ | EOF => return false,
            _ => ahead += 1,
        }
    }
}

/// Parses a cover point or a cross: its label, what it covers, the condition
/// it is sampled under, and its bins.
fn point_or_cross<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    if !matches!(parser.kind(0), COVERPOINT_KW | CROSS_KW) && parser.kind(1) != COLON {
        data_type(parser);
    }
    if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) && parser.kind(1) == COLON {
        parser.bump();
        parser.bump();
    }
    if !matches!(parser.kind(0), COVERPOINT_KW | CROSS_KW) {
        return decline(parser, marker, before);
    }
    let kind = match parser.kind(0) {
        CROSS_KW => CROSS,
        _ => COVERPOINT,
    };
    parser.bump();
    while expr(parser).is_some() && kind == CROSS && parser.at(COMMA) {
        parser.bump();
    }
    if parser.at(IFF_KW) {
        parser.bump();
        condition(parser);
    }
    if parser.at(L_BRACE) {
        block(parser, limit);
    } else if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, kind))
}

/// Parses `{`, the bins and options inside, and `}`.
fn block<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    let marker = parser.start();
    // Where the `}` is, so that a bin no rule takes cannot run past it.
    let close = parser.past_group(0, L_BRACE, R_BRACE).saturating_sub(1);
    let mut end = parser.ahead(close as u32);
    if let Some(limit) = limit {
        end = end.min(limit);
    }
    parser.bump();
    while !parser.at_end() && parser.position() < end {
        let at = parser.position();
        item(parser, Some(end));
        if parser.position() == at {
            break;
        }
    }
    if parser.at(R_BRACE) {
        parser.bump();
    }
    parser.complete(marker, BINS_BLOCK);
}

/// Parses a set of bins: its kind, its name and how many there are, what it
/// counts, the condition it counts under, and `;`.
fn bins<T: Tokens>(parser: &mut Parser<T>, marker: Marker, before: Snapshot) -> Option<Completed> {
    if parser.at(WILDCARD_KW) {
        parser.bump();
    }
    if !matches!(parser.kind(0), BINS_KW | ILLEGAL_BINS_KW | IGNORE_BINS_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return decline(parser, marker, before);
    }
    parser.bump();
    if parser.at(L_BRACK) {
        parser.bump();
        expr(parser);
        if !parser.at(R_BRACK) {
            return decline(parser, marker, before);
        }
        parser.bump();
    }
    if !parser.at(EQ) {
        return decline(parser, marker, before);
    }
    parser.bump();

    match parser.kind(0) {
        L_BRACE => {
            range_list(parser, false);
            if parser.at(WITH_KW) && parser.kind(1) == L_PAREN {
                with_clause(parser);
            }
        }
        L_PAREN if is_transitions(parser) => {
            trans_set(parser);
            while parser.at(COMMA) && parser.kind(1) == L_PAREN {
                parser.bump();
                trans_set(parser);
            }
        }
        DEFAULT_KW => {
            parser.bump();
            if parser.at(SEQUENCE_KW) {
                parser.bump();
            }
        }
        _ => {
            expr(parser);
            // How many of the selected bins a cross bin needs.
            if parser.at(MATCHES_KW) {
                parser.bump();
                expr(parser);
            }
        }
    }

    if parser.at(IFF_KW) {
        parser.bump();
        condition(parser);
    }
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, BINS))
}

/// Whether the `(` at the cursor holds transitions rather than an
/// expression: it has a `=>`, or a repetition such as `[*2]`.
fn is_transitions<T: Tokens>(parser: &Parser<T>) -> bool {
    let end = parser.past_group(0, L_PAREN, R_PAREN);
    (1..end).any(|ahead| {
        parser.kind(ahead) == EQ_GT
            || (parser.kind(ahead) == L_BRACK
                && matches!(parser.kind(ahead + 1), STAR | MINUS_GT | EQ))
    })
}

/// Parses `(`, the steps of a transition between `=>`, each a list of values
/// and ranges with an optional repetition, and `)`.
fn trans_set<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    while !parser.at_end() && !parser.at(R_PAREN) {
        match parser.kind(0) {
            COMMA | EQ_GT => parser.bump(),
            L_BRACK => {
                parser.bump();
                if matches!(parser.kind(0), STAR | MINUS_GT | EQ) {
                    parser.bump();
                }
                expr(parser);
                if parser.at(COLON) {
                    parser.bump();
                    expr(parser);
                }
                if !parser.at(R_BRACK) {
                    break;
                }
                parser.bump();
            }
            _ => {
                if expr(parser).is_none() {
                    break;
                }
            }
        }
    }
    if parser.at(R_PAREN) {
        parser.bump();
    }
    parser.complete(marker, TRANS_SET);
}

/// Abandons the open marker and rolls back parser state to `before`.
fn decline<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.abandon(marker);
    parser.rollback(before);
    None
}
