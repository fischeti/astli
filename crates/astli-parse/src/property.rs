//! Parsing rules for concurrent assertions: `assert property` and the like,
//! `property` and `sequence` declarations, and the expressions they hold.
//!
//! A sequence is a property, so one precedence climb reads both. Its operands
//! are expressions, whose operators all bind tighter than any of these, and
//! none of which is spelled like one of these; so an expression stops where a
//! sequence or property operator starts.

use super::decl::{declaration, declarators, semicolon};
use super::event::{Completed, Marker};
use super::expr::{self, expr};
use super::source::{Position, Tokens};
use super::stmt::{assignment, condition, label, timing_control};
use super::verbatim::{Context, verbatim};
use super::{Parser, Scope, Snapshot, any};
use astli_syntax::{SyntaxKind, SyntaxKind::*};

/// Returns `true` if an `assert`, `assume`, `cover`, `restrict` or `expect`
/// `ahead` of the cursor checks a property or a sequence.
pub(super) fn is_concurrent<T: Tokens>(parser: &Parser<T>, ahead: usize) -> bool {
    match parser.kind(ahead) {
        ASSERT_KW | ASSUME_KW | COVER_KW | RESTRICT_KW => {
            matches!(parser.kind(ahead + 1), PROPERTY_KW | SEQUENCE_KW)
        }
        EXPECT_KW => parser.kind(ahead + 1) == L_PAREN,
        _ => false,
    }
}

/// Parses a concurrent assertion: its keywords, the property in parentheses,
/// and what it runs when the property holds and when it does not.
pub(super) fn assertion<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    let cover = parser.at(COVER_KW);
    parser.bump();
    if matches!(parser.kind(0), PROPERTY_KW | SEQUENCE_KW) {
        parser.bump();
    }
    if !parser.at(L_PAREN) {
        return decline(parser, marker, before);
    }
    parser.bump();
    spec(parser);
    if !parser.at(R_PAREN) {
        return decline(parser, marker, before);
    }
    parser.bump();

    // What it runs is a statement, even when it stands among items.
    let scope = parser.set_scope(Scope::Statement);
    if cover || !parser.at(ELSE_KW) {
        any(parser, limit);
    }
    if !cover && parser.at(ELSE_KW) {
        parser.bump();
        any(parser, limit);
    }
    parser.set_scope(scope);
    Some(parser.complete(marker, CONCURRENT_ASSERTION))
}

/// Parses `default disable iff`, the reset, and `;`. Unlike the one in a
/// property, the reset needs no parentheses.
pub(super) fn default_disable<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    parser.bump();
    parser.bump();
    expr(parser);
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, DEFAULT_DISABLE))
}

/// Parses a `property` or `sequence` declaration: its name and ports, the
/// variables it declares, what it is, and its closer.
pub(super) fn declaration_of<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    let (kind, closer) = match parser.kind(0) {
        PROPERTY_KW => (PROPERTY_DECL, ENDPROPERTY_KW),
        _ => (SEQUENCE_DECL, ENDSEQUENCE_KW),
    };
    parser.bump();
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return decline(parser, marker, before);
    }
    parser.bump();
    if parser.at(L_PAREN) {
        super::item::port_list(parser);
    }
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    while !parser.at(closer) && declaration(parser).is_some() {}
    spec(parser);
    semicolon(parser);
    if !parser.at(closer) {
        return decline(parser, marker, before);
    }
    parser.bump();
    label(parser);
    Some(parser.complete(marker, kind))
}

/// Parses a clocking block: `default` or `global`, its name, its clock, and
/// what it samples and drives; or `default clocking name;`, which makes a
/// block declared elsewhere the default.
pub(super) fn clocking<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    if matches!(parser.kind(0), DEFAULT_KW | GLOBAL_KW) {
        parser.bump();
    }
    parser.bump();
    if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        parser.bump();
    }
    if semicolon(parser) {
        return Some(parser.complete(marker, CLOCKING_DECL));
    }
    if !parser.at(AT) {
        return decline(parser, marker, before);
    }
    timing_control(parser);
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    while !parser.at_end() && !parser.at(ENDCLOCKING_KW) {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }
        let at = parser.position();
        clocking_item(parser, limit);
        if parser.position() == at {
            break;
        }
    }
    if !parser.at(ENDCLOCKING_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    label(parser);
    Some(parser.complete(marker, CLOCKING_DECL))
}

/// Parses the signals a clocking block samples or drives one way, with their
/// direction and skews, or its default skews; anything else in it is an
/// item, such as a property it declares.
fn clocking_item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    let signals = matches!(parser.kind(0), INPUT_KW | OUTPUT_KW | INOUT_KW)
        || (parser.at(DEFAULT_KW) && matches!(parser.kind(1), INPUT_KW | OUTPUT_KW));
    if !signals {
        super::item::item(parser, limit);
        return;
    }
    let before = parser.snapshot();
    let marker = parser.start();
    if parser.at(DEFAULT_KW) {
        parser.bump();
    }
    loop {
        match parser.kind(0) {
            INPUT_KW | OUTPUT_KW | INOUT_KW | POSEDGE_KW | NEGEDGE_KW | EDGE_KW => parser.bump(),
            HASH => {
                timing_control(parser);
            }
            _ => break,
        }
    }
    declarators(parser, false);
    if !semicolon(parser) {
        parser.abandon(marker);
        parser.rollback(before);
        verbatim(parser, Context::Terminated, limit);
        return;
    }
    parser.complete(marker, CLOCKING_ITEM);
}

/// Parses a clock, `disable iff (…)`, and a property, any of the first two
/// left out.
fn spec<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    if parser.at(AT) {
        timing_control(parser);
    }
    if parser.at(DISABLE_KW) && parser.kind(1) == IFF_KW {
        parser.bump();
        parser.bump();
        condition(parser);
    }
    property(parser, 0);
    parser.complete(marker, PROPERTY_SPEC);
}

/// Returns the binding powers of a binary sequence or property operator,
/// lowest first, as the standard ranks them.
fn binding(kind: SyntaxKind) -> Option<(u8, u8)> {
    let (level, right) = match kind {
        PIPE_MINUS_GT | PIPE_EQ_GT | HASH_MINUS_HASH | HASH_EQ_HASH => (1, true),
        UNTIL_KW | S_UNTIL_KW | UNTIL_WITH_KW | S_UNTIL_WITH_KW | IMPLIES_KW => (2, true),
        IFF_KW => (3, true),
        OR_KW => (4, false),
        AND_KW => (5, false),
        INTERSECT_KW => (7, false),
        WITHIN_KW => (8, false),
        THROUGHOUT_KW => (9, true),
        HASH_HASH => (10, false),
        _ => return None,
    };
    Some(match right {
        true => (2 * level + 1, 2 * level),
        false => (2 * level, 2 * level + 1),
    })
}

/// What `not`, `nexttime` and `s_nexttime` take: everything that binds
/// tighter than `and`, the level they sit at.
const ABOVE_AND: u8 = 13;

/// What a prefix `##` takes: an operand, and none of the `##` after it.
const ABOVE_DELAY: u8 = 21;

/// Parses a sequence or property whose operators bind at least as tightly as
/// `min`.
fn property<T: Tokens>(parser: &mut Parser<T>, min: u8) -> Option<Completed> {
    let mut lhs = unary(parser)?;
    loop {
        let kind = parser.kind(0);
        let Some((left, right)) = binding(kind) else {
            break;
        };
        if left < min {
            break;
        }
        let before = parser.snapshot();
        let marker = parser.precede(lhs);
        let (node, taken) = match kind {
            HASH_HASH => {
                cycle_delay(parser);
                (SEQUENCE_DELAY, property(parser, right))
            }
            _ => {
                parser.bump();
                (PROPERTY_BIN_EXPR, property(parser, right))
            }
        };
        if taken.is_none() {
            parser.abandon(marker);
            parser.rollback(before);
            break;
        }
        lhs = parser.complete(marker, node);
    }
    Some(lhs)
}

/// Parses an operand, with the prefix operators before it and the
/// repetitions after it.
fn unary<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    if parser.too_deep() {
        return super::verbatim::too_deep_operand(parser);
    }
    let before = parser.snapshot();
    let marker = parser.start();
    let node = match parser.kind(0) {
        NOT_KW | NEXTTIME_KW | S_NEXTTIME_KW => {
            parser.bump();
            range(parser);
            property(parser, ABOVE_AND).map(|_| PROPERTY_UNARY_EXPR)
        }
        ALWAYS_KW | S_ALWAYS_KW | EVENTUALLY_KW | S_EVENTUALLY_KW => {
            parser.bump();
            range(parser);
            property(parser, 0).map(|_| PROPERTY_UNARY_EXPR)
        }
        ACCEPT_ON_KW | REJECT_ON_KW | SYNC_ACCEPT_ON_KW | SYNC_REJECT_ON_KW => {
            parser.bump();
            condition(parser);
            property(parser, 0).map(|_| PROPERTY_UNARY_EXPR)
        }
        STRONG_KW | WEAK_KW | FIRST_MATCH_KW if parser.kind(1) == L_PAREN => {
            parser.bump();
            paren(parser);
            Some(PROPERTY_UNARY_EXPR)
        }
        IF_KW => {
            parser.bump();
            condition(parser);
            let then = property(parser, 0);
            if then.is_some() && parser.at(ELSE_KW) {
                parser.bump();
                property(parser, 0);
            }
            then.map(|_| PROPERTY_IF)
        }
        AT => {
            timing_control(parser);
            property(parser, 0).map(|_| CLOCKED_PROPERTY)
        }
        HASH_HASH => {
            cycle_delay(parser);
            property(parser, ABOVE_DELAY).map(|_| SEQUENCE_DELAY)
        }
        L_PAREN if is_property_group(parser) => {
            parser.abandon(marker);
            let operand = paren(parser);
            return Some(repetitions(parser, operand));
        }
        _ => {
            parser.abandon(marker);
            let operand = expr(parser)?;
            return Some(repetitions(parser, operand));
        }
    };
    let Some(node) = node else {
        parser.abandon(marker);
        parser.rollback(before);
        return None;
    };
    let operand = parser.complete(marker, node);
    Some(repetitions(parser, operand))
}

/// Parses the repetitions after `operand`, as in `a [*2] [->1]`.
fn repetitions<T: Tokens>(parser: &mut Parser<T>, mut operand: Completed) -> Completed {
    while expr::is_repetition(parser, 0) {
        let marker = parser.precede(operand);
        parser.bump();
        parser.bump();
        if !parser.at(R_BRACK) {
            expr(parser);
            if parser.at(COLON) {
                parser.bump();
                expr(parser);
            }
        }
        if parser.at(R_BRACK) {
            parser.bump();
        }
        operand = parser.complete(marker, REPETITION);
    }
    operand
}

/// Parses `##` and the delay after it: a number or a name, or a range in
/// brackets, `[*]` or `[+]`.
fn cycle_delay<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    if parser.at(L_BRACK) {
        parser.bump();
        if matches!(parser.kind(0), STAR | PLUS) && parser.kind(1) == R_BRACK {
            parser.bump();
        } else {
            expr(parser);
            if parser.at(COLON) {
                parser.bump();
                expr(parser);
            }
        }
        if parser.at(R_BRACK) {
            parser.bump();
        }
    } else {
        // Only a primary: in `##1 (a)` the parentheses are the sequence's.
        expr::primary(parser);
    }
    parser.complete(marker, CYCLE_DELAY);
}

/// Parses an optional range in brackets, as `always [2:5]` takes.
fn range<T: Tokens>(parser: &mut Parser<T>) {
    if !parser.at(L_BRACK) {
        return;
    }
    parser.bump();
    expr(parser);
    if parser.at(COLON) {
        parser.bump();
        expr(parser);
    }
    if parser.at(R_BRACK) {
        parser.bump();
    }
}

/// Parses `(`, a sequence or property, what a match assigns, and `)`.
fn paren<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    parser.bump();
    property(parser, 0);
    while parser.at(COMMA) {
        parser.bump();
        if assignment(parser).is_none() {
            break;
        }
    }
    if parser.at(R_PAREN) {
        parser.bump();
    }
    parser.complete(marker, PROPERTY_PAREN)
}

/// Whether the `(` at the cursor holds a sequence or a property rather than
/// an expression: an operator of theirs stands directly inside it.
fn is_property_group<T: Tokens>(parser: &Parser<T>) -> bool {
    let end = parser.past_group(0, L_PAREN, R_PAREN);
    let mut depth = 0u32;
    for ahead in 1..end.saturating_sub(1) {
        let kind = parser.kind(ahead);
        match kind {
            L_PAREN | L_BRACE | APOSTROPHE_L_BRACE => depth += 1,
            R_PAREN | R_BRACE => depth = depth.saturating_sub(1),
            L_BRACK if depth == 0 && expr::is_repetition(parser, ahead) => return true,
            L_BRACK => depth += 1,
            R_BRACK => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            COMMA | AT => return true,
            _ if binding(kind).is_some() => return true,
            NOT_KW | NEXTTIME_KW | S_NEXTTIME_KW | ALWAYS_KW | S_ALWAYS_KW | EVENTUALLY_KW
            | S_EVENTUALLY_KW | STRONG_KW | WEAK_KW | FIRST_MATCH_KW | ACCEPT_ON_KW
            | REJECT_ON_KW | SYNC_ACCEPT_ON_KW | SYNC_REJECT_ON_KW => return true,
            _ => {}
        }
    }
    false
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
