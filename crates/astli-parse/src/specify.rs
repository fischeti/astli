//! Parsing rules for a `specify` block: the delays of the paths through a
//! module, and the timing checks on its signals.

use super::decl::semicolon;
use super::event::{Completed, Marker};
use super::expr::{expr, lvalue, min_typ_max};
use super::source::{Position, Tokens};
use super::stmt::{condition, delay_values};
use super::verbatim::{Context, verbatim};
use super::{Parser, Scope, Snapshot, preprocessor};
use astli_syntax::SyntaxKind::*;

/// Parses `specify`, its items, and `endspecify`; at the cursor, with
/// `marker` open.
pub(super) fn block<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    let scope = parser.set_scope(Scope::Specify);
    while !parser.at_end() && !parser.at(ENDSPECIFY_KW) {
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

    if !parser.at(ENDSPECIFY_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, SPECIFY_BLOCK))
}

/// Parses one path, timing check, pulse declaration or `specparam`, falling
/// back to verbatim recovery if no rule matches.
pub(super) fn item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && !preprocessor::continued(parser) && preprocessor::any(parser) {
        return;
    }
    let before = parser.snapshot();
    let marker = parser.start();
    let taken = match parser.kind(0) {
        L_PAREN | IF_KW | IFNONE_KW => path(parser, marker, before),
        SYSTEM_IDENT => timing_check(parser, marker, before),
        PULSESTYLE_ONEVENT_KW | PULSESTYLE_ONDETECT_KW | SHOWCANCELLED_KW | NOSHOWCANCELLED_KW => {
            pulse(parser, marker, before)
        }
        _ => {
            parser.abandon(marker);
            parser.rollback(before);
            super::item::item(parser, limit);
            return;
        }
    };
    if taken.is_none() {
        verbatim(parser, Context::Terminated, limit);
    }
}

/// Parses a path and its delays, `(a, b *> y) = (1, 2);`: the state it
/// holds in, the edge of its input, its polarity, and what its outputs take.
fn path<T: Tokens>(parser: &mut Parser<T>, marker: Marker, before: Snapshot) -> Option<Completed> {
    match parser.kind(0) {
        IF_KW => {
            parser.bump();
            condition(parser);
        }
        IFNONE_KW => parser.bump(),
        _ => {}
    }
    if !parser.at(L_PAREN) {
        return decline(parser, marker, before);
    }
    parser.bump();
    if matches!(parser.kind(0), POSEDGE_KW | NEGEDGE_KW | EDGE_KW) {
        parser.bump();
    }
    if !terminals(parser) || !arrow(parser) {
        return decline(parser, marker, before);
    }
    // `(q +: d)`: the outputs and the data that reaches them.
    if parser.at(L_PAREN) {
        parser.bump();
        if !terminals(parser) {
            return decline(parser, marker, before);
        }
        match parser.kind(0) {
            PLUS_COLON | MINUS_COLON | COLON => parser.bump(),
            PLUS | MINUS if parser.kind(1) == COLON => {
                parser.bump();
                parser.bump();
            }
            _ => return decline(parser, marker, before),
        }
        if expr(parser).is_none() || !close(parser) {
            return decline(parser, marker, before);
        }
    } else if !terminals(parser) {
        return decline(parser, marker, before);
    }
    if !close(parser) || !parser.at(EQ) {
        return decline(parser, marker, before);
    }
    parser.bump();
    // The delays may be in parentheses or not.
    if parser.at(L_PAREN) {
        delay_values(parser);
    } else {
        loop {
            let Some(min) = expr(parser) else {
                return decline(parser, marker, before);
            };
            min_typ_max(parser, min);
            if !parser.at(COMMA) {
                break;
            }
            parser.bump();
        }
    }
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, SPECIFY_PATH))
}

/// Consumes `=>` or `*>`, a parallel or a full path, and the polarity before
/// it. `+=>` reads as `+=` and `>`, which only a path writes together.
fn arrow<T: Tokens>(parser: &mut Parser<T>) -> bool {
    let tokens = match (parser.kind(0), parser.kind(1)) {
        (PLUS_EQ | MINUS_EQ, GT) if parser.adjacent(1) => 2,
        (PLUS | MINUS, EQ_GT | STAR_GT) => 2,
        (EQ_GT | STAR_GT, _) => 1,
        _ => return false,
    };
    for _ in 0..tokens {
        parser.bump();
    }
    true
}

/// Parses the terminals of a path, each a port, a select of one, or an
/// interface's port, between commas.
fn terminals<T: Tokens>(parser: &mut Parser<T>) -> bool {
    loop {
        if lvalue(parser).is_none() {
            return false;
        }
        if !parser.at(COMMA) {
            return true;
        }
        parser.bump();
    }
}

/// Consumes the `)` that closes a path or its outputs.
fn close<T: Tokens>(parser: &mut Parser<T>) -> bool {
    let found = parser.at(R_PAREN);
    if found {
        parser.bump();
    }
    found
}

/// Parses a timing check, `$setup(d, posedge clk &&& en, 1, notifier);`.
/// An argument may be an event, with an edge, or left out.
fn timing_check<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    if !parser.at(L_PAREN) {
        return decline(parser, marker, before);
    }
    let list = parser.start();
    parser.bump();
    loop {
        let argument = parser.start();
        edge(parser);
        if let Some(min) = expr(parser) {
            min_typ_max(parser, min);
        }
        parser.complete(argument, ARG);
        if !parser.at(COMMA) {
            break;
        }
        parser.bump();
    }
    if !close(parser) {
        parser.abandon(list);
        return decline(parser, marker, before);
    }
    parser.complete(list, ARG_LIST);
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, TIMING_CHECK))
}

/// Consumes the edge of an event in a timing check: `posedge`, `negedge`,
/// `edge`, or the transitions `edge [01, 1x]` names. A transition is lexed
/// as whatever its digits and letters make, so its tokens are taken as
/// they come.
fn edge<T: Tokens>(parser: &mut Parser<T>) {
    match parser.kind(0) {
        POSEDGE_KW | NEGEDGE_KW => parser.bump(),
        EDGE_KW => {
            parser.bump();
            if parser.at(L_BRACK) {
                while !parser.at_end() && !matches!(parser.kind(0), R_BRACK | SEMICOLON) {
                    parser.bump();
                }
                if parser.at(R_BRACK) {
                    parser.bump();
                }
            }
        }
        _ => {}
    }
}

/// Parses `pulsestyle_onevent`, `pulsestyle_ondetect`, `showcancelled` or
/// `noshowcancelled`, and the outputs it applies to.
fn pulse<T: Tokens>(parser: &mut Parser<T>, marker: Marker, before: Snapshot) -> Option<Completed> {
    parser.bump();
    if !terminals(parser) || !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, PULSE_DECL))
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
