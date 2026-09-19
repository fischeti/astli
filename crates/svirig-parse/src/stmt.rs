//! Parsing rules for procedural and generate statements.
//!
//! This module handles sequential and concurrent statement constructs, including:
//! - Block statements (`begin` … `end`, `fork` … `join`)
//! - Conditional branching (`if` … `else`, `case` / `casex` / `casez`)
//! - Loops (`for`, `foreach`, `while`, `repeat`, `do` … `while`, `forever`)
//! - Flow control (`return`, `break`, `continue`, `disable`)
//! - Procedural timing and event triggers (`#delay`, `@event`, `-> event`)
//! - Variable assignments and procedural expression statements
//!
//! Generate constructs share the same grammar rules as procedural statements,
//! with child elements resolved according to [`Parser::scope`](crate::Parser::scope).

use super::decl::{at_declarator_only, data_type, declaration_at, declarators, semicolon};
use super::event::{Completed, Marker};
use super::expr::{attributes, expr, lvalue};
use super::source::{Position, Tokens};
use super::verbatim::{Context, verbatim};
use super::{Parser, Snapshot, any, preprocessor};
use svirig_syntax::{SyntaxKind, SyntaxKind::*};

/// Parses a statement at the cursor, falling back to verbatim recovery if no rule matches.
pub fn statement<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && preprocessor::any(parser) {
        return;
    }
    if one(parser, limit).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated, limit);
}

/// Attempts to parse a single statement, rolling back on failure.
fn one<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();
    attributes(parser);
    statement_at(parser, marker, before, limit)
}

/// Parses a statement into `marker`, which has already consumed leading attributes.
pub(super) fn statement_at<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    match parser.kind(0) {
        BEGIN_KW => block(parser, marker, limit),
        FORK_KW => block(parser, marker, limit),

        UNIQUE_KW | UNIQUE0_KW | PRIORITY_KW => {
            parser.bump();
            match parser.kind(0) {
                IF_KW => if_stmt(parser, marker, limit),
                CASE_KW | CASEX_KW | CASEZ_KW => case_stmt(parser, marker, limit, before),
                _ => decline(parser, marker, before),
            }
        }
        IF_KW => if_stmt(parser, marker, limit),
        CASE_KW | CASEX_KW | CASEZ_KW => case_stmt(parser, marker, limit, before),

        FOR_KW => for_stmt(parser, marker, limit),
        FOREACH_KW => loop_stmt(parser, marker, limit, FOREACH_STMT),
        WHILE_KW => loop_stmt(parser, marker, limit, WHILE_STMT),
        REPEAT_KW => loop_stmt(parser, marker, limit, REPEAT_STMT),
        FOREVER_KW => {
            parser.bump();
            any(parser, limit);
            Some(parser.complete(marker, FOREVER_STMT))
        }
        DO_KW => do_while(parser, marker, limit, before),

        RETURN_KW => {
            parser.bump();
            expr(parser);
            terminated(parser, marker, before, RETURN_STMT)
        }
        BREAK_KW => {
            parser.bump();
            terminated(parser, marker, before, BREAK_STMT)
        }
        CONTINUE_KW => {
            parser.bump();
            terminated(parser, marker, before, CONTINUE_STMT)
        }
        DISABLE_KW => {
            parser.bump();
            if parser.at(FORK_KW) {
                parser.bump();
            } else {
                expr(parser);
            }
            terminated(parser, marker, before, DISABLE_STMT)
        }
        WAIT_KW => wait_stmt(parser, marker, limit, before),

        MINUS_GT | MINUS_GT_GT => {
            parser.bump();
            timing_control(parser);
            expr(parser);
            terminated(parser, marker, before, EVENT_TRIGGER)
        }

        AT | HASH | HASH_HASH => {
            timing_control(parser);
            if parser.at(SEMICOLON) {
                parser.bump();
            } else {
                any(parser, limit);
            }
            Some(parser.complete(marker, TIMING_STMT))
        }

        IDENT | ESCAPED_IDENT if parser.kind(1) == COLON => {
            parser.bump();
            parser.bump();
            any(parser, limit);
            Some(parser.complete(marker, LABELED_STMT))
        }

        _ => {
            if let Some(node) = declaration_at(parser, marker) {
                return Some(node);
            }
            parser.rollback(before);
            let marker = parser.start();
            attributes(parser);
            expr_stmt(parser, marker, before)
        }
    }
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

/// Completes `marker` as `kind` if followed by a semicolon; declines otherwise.
fn terminated<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    kind: SyntaxKind,
) -> Option<Completed> {
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, kind))
}

/// Parses a `begin` … `end` or `fork` … `join` block statement.
fn block<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
) -> Option<Completed> {
    let closers: &[SyntaxKind] = match parser.kind(0) {
        FORK_KW => &[JOIN_KW, JOIN_ANY_KW, JOIN_NONE_KW],
        _ => &[END_KW],
    };
    parser.bump();
    label(parser);

    body(parser, |kind| closers.contains(&kind), limit, any);

    if closers.contains(&parser.kind(0)) {
        parser.bump();
        label(parser);
    }
    Some(parser.complete(marker, BLOCK))
}

/// Parses an `if (cond) stmt else stmt` conditional statement.
fn if_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    condition(parser);
    any(parser, limit);
    if parser.at(ELSE_KW) {
        parser.bump();
        any(parser, limit);
    }
    Some(parser.complete(marker, IF_STMT))
}

/// Parses a `case` / `casex` / `casez` selection statement.
fn case_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    condition(parser);
    if matches!(parser.kind(0), MATCHES_KW | INSIDE_KW) {
        parser.bump();
    }

    body(parser, |kind| kind == ENDCASE_KW, limit, case_item);

    if !parser.at(ENDCASE_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, CASE_STMT))
}

/// Parses a single arm within a `case` statement.
fn case_item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    let marker = parser.start();

    if parser.at(DEFAULT_KW) {
        parser.bump();
    } else {
        loop {
            if expr(parser).is_none() {
                break;
            }
            if parser.at(COMMA) {
                parser.bump();
            } else {
                break;
            }
        }
    }

    if parser.at(COLON) {
        parser.bump();
    }
    any(parser, limit);

    parser.complete(marker, CASE_ITEM);
}

/// Parses a `for (init; cond; step)` loop statement.
fn for_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();

    if parser.at(L_PAREN) {
        let header = parser.start();
        parser.bump();
        if !parser.at(SEMICOLON) {
            initialiser(parser);
        }
        if parser.at(SEMICOLON) {
            parser.bump();
        }
        expr(parser);
        if parser.at(SEMICOLON) {
            parser.bump();
        }
        loop {
            if assignment(parser).is_none() {
                break;
            }
            if parser.at(COMMA) {
                parser.bump();
            } else {
                break;
            }
        }
        close(parser, header);
    }

    any(parser, limit);
    Some(parser.complete(marker, FOR_STMT))
}

/// Parses the initialization clause of a `for` loop header.
fn initialiser<T: Tokens>(parser: &mut Parser<T>) {
    let declares = parser.at(GENVAR_KW) || !at_declarator_only(parser);
    if !declares {
        loop {
            if assignment(parser).is_none() {
                break;
            }
            if parser.at(COMMA) {
                parser.bump();
            } else {
                break;
            }
        }
        return;
    }

    let marker = parser.start();
    if parser.at(GENVAR_KW) {
        parser.bump();
    } else {
        data_type(parser);
    }
    declarators(parser, false);
    parser.complete(marker, VAR_DECL);
}

/// Parses single-condition loop headers (`foreach`, `while`, `repeat`).
fn loop_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
    kind: SyntaxKind,
) -> Option<Completed> {
    parser.bump();
    condition(parser);
    any(parser, limit);
    Some(parser.complete(marker, kind))
}

/// Parses a `do statement while (condition);` loop statement.
fn do_while<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    any(parser, limit);
    if !parser.at(WHILE_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    condition(parser);
    terminated(parser, marker, before, DO_WHILE_STMT)
}

/// Parses a `wait (cond)` or `wait fork;` statement.
fn wait_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    if parser.at(FORK_KW) {
        parser.bump();
        return terminated(parser, marker, before, WAIT_STMT);
    }
    if !parser.at(L_PAREN) {
        return decline(parser, marker, before);
    }
    condition(parser);
    if parser.at(SEMICOLON) {
        parser.bump();
    } else {
        any(parser, limit);
    }
    Some(parser.complete(marker, WAIT_STMT))
}

/// Parses an expression or assignment statement terminated by a semicolon.
fn expr_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    if parser.at(SEMICOLON) {
        parser.bump();
        return Some(parser.complete(marker, EXPR_STMT));
    }
    if assignment(parser).is_none() {
        return decline(parser, marker, before);
    }
    terminated(parser, marker, before, EXPR_STMT)
}

/// Parses an lvalue followed by an optional assignment operator and expression.
pub(super) fn assignment<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let lhs = lvalue(parser)?;
    if !is_assignment(parser.kind(0)) {
        return Some(lhs);
    }

    let marker = parser.precede(lhs);
    parser.bump();
    timing_control(parser);
    expr(parser);
    Some(parser.complete(marker, ASSIGNMENT))
}

/// Returns `true` if `kind` is an assignment operator.
fn is_assignment(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        EQ | LT_EQ
            | PLUS_EQ
            | MINUS_EQ
            | STAR_EQ
            | SLASH_EQ
            | PERCENT_EQ
            | AMP_EQ
            | PIPE_EQ
            | CARET_EQ
            | LT_LT_EQ
            | GT_GT_EQ
            | LT_LT_LT_EQ
            | GT_GT_GT_EQ
    )
}

/// Parses an optional timing control (`@` event control or `#` delay control).
pub(super) fn timing_control<T: Tokens>(parser: &mut Parser<T>) -> bool {
    match parser.kind(0) {
        AT => {
            event_control(parser);
            true
        }
        HASH | HASH_HASH => {
            delay_control(parser);
            true
        }
        _ => false,
    }
}

/// Parses an `@(...)` event control expression.
fn event_control<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();

    match parser.kind(0) {
        L_PAREN => {
            let list = parser.start();
            parser.bump();
            event_expr(parser);
            close(parser, list);
        }
        STAR => parser.bump(),
        _ => {
            expr(parser);
        }
    }

    parser.complete(marker, EVENT_CONTROL);
}

/// Parses event expressions within a sensitivity list.
fn event_expr<T: Tokens>(parser: &mut Parser<T>) {
    loop {
        if matches!(parser.kind(0), POSEDGE_KW | NEGEDGE_KW | EDGE_KW) {
            parser.bump();
        }
        if expr(parser).is_none() {
            break;
        }
        if parser.at(IFF_KW) {
            parser.bump();
            expr(parser);
        }
        if matches!(parser.kind(0), COMMA | OR_KW) {
            parser.bump();
        } else {
            break;
        }
    }
}

/// Parses a `#delay` or `##cycle` delay expression.
fn delay_control<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();

    if parser.at(L_PAREN) {
        condition(parser);
    } else if expr(parser).is_none() && !parser.at_end() {
        parser.bump();
    }

    parser.complete(marker, DELAY_CONTROL);
}

/// Parses a parenthesized expression or condition header.
fn condition<T: Tokens>(parser: &mut Parser<T>) {
    if !parser.at(L_PAREN) {
        return;
    }
    let marker = parser.start();
    parser.bump();
    expr(parser);
    close(parser, marker);
}

/// Closes a parenthesized construct, skipping unparsed tokens until matching `)`.
fn close<T: Tokens>(parser: &mut Parser<T>, marker: Marker) {
    let mut depth = 0u32;
    while !parser.at_end() {
        match parser.kind(0) {
            L_PAREN => depth += 1,
            R_PAREN if depth == 0 => break,
            R_PAREN => depth -= 1,
            _ => {}
        }
        parser.bump();
    }
    if parser.at(R_PAREN) {
        parser.bump();
    }
    parser.complete(marker, PAREN_EXPR);
}

/// Repeatedly executes `element` until reaching a token satisfying `closes` or `limit`.
fn body<T: Tokens>(
    parser: &mut Parser<T>,
    closes: impl Fn(SyntaxKind) -> bool,
    limit: Option<Position>,
    element: impl Fn(&mut Parser<T>, Option<Position>),
) {
    while !parser.at_end() && !closes(parser.kind(0)) {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }
        let before = parser.position();
        element(parser, limit);
        if parser.position() == before {
            break;
        }
    }
}

/// Consumes an optional `: label` clause.
pub(super) fn label<T: Tokens>(parser: &mut Parser<T>) {
    if parser.at(COLON) && matches!(parser.kind(1), IDENT | ESCAPED_IDENT | NEW_KW) {
        parser.bump();
        parser.bump();
    }
}
