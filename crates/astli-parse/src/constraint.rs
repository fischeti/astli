//! Parsing rules for constraints: the body of a `constraint`, and the block
//! after `randomize() with`.
//!
//! An `if` or a `foreach` among them is the statement's rule, run in
//! [`Scope::Constraint`] so that its arms are constraints too.

use super::decl::semicolon;
use super::event::{Completed, Marker};
use super::expr::{antecedent, expr, range_list};
use super::source::{Position, Tokens};
use super::stmt::{foreach_stmt, if_stmt};
use super::verbatim::{Context, verbatim};
use super::{Parser, Scope, Snapshot, any, preprocessor};
use astli_syntax::SyntaxKind::*;

/// Parses `{`, the constraints inside, and `}`.
pub(super) fn block<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) -> Completed {
    let marker = parser.start();
    // Where the `}` is, so that a constraint no rule takes cannot run past it.
    let close = parser.past_group(0, L_BRACE, R_BRACE).saturating_sub(1);
    let mut end = parser.ahead(close as u32);
    if let Some(limit) = limit {
        end = end.min(limit);
    }
    parser.bump();

    let scope = parser.set_scope(Scope::Constraint);
    while !parser.at_end() && parser.position() < end {
        let at = parser.position();
        item(parser, Some(end));
        if parser.position() == at {
            break;
        }
    }
    parser.set_scope(scope);

    if parser.at(R_BRACE) {
        parser.bump();
    }
    parser.complete(marker, CONSTRAINT_BLOCK)
}

/// Parses one constraint at the cursor, falling back to verbatim recovery if
/// no rule matches.
pub(super) fn item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && !preprocessor::continued(parser) && preprocessor::any(parser) {
        return;
    }
    if one(parser, limit).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated, limit);
}

/// Attempts to parse a constraint, rolling back on failure.
fn one<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();

    match parser.kind(0) {
        L_BRACE if is_block(parser) => {
            parser.abandon(marker);
            Some(block(parser, limit))
        }
        IF_KW => if_stmt(parser, marker, limit),
        FOREACH_KW => foreach_stmt(parser, marker, limit),
        SOLVE_KW => solve_before(parser, marker, before),
        SOFT_KW => {
            parser.bump();
            expr(parser);
            terminated(parser, marker, before)
        }
        DISABLE_KW if parser.kind(1) == SOFT_KW => {
            parser.bump();
            parser.bump();
            expr(parser);
            terminated(parser, marker, before)
        }
        // Most often after a macro that stands for a constraint.
        SEMICOLON => terminated(parser, marker, before),
        UNIQUE_KW if parser.kind(1) == L_BRACE => {
            parser.bump();
            range_list(parser, false);
            terminated(parser, marker, before)
        }
        _ => {
            if antecedent(parser).is_none() {
                return decline(parser, marker, before);
            }
            match parser.kind(0) {
                MINUS_GT => {
                    parser.bump();
                    any(parser, limit);
                    Some(parser.complete(marker, IMPLICATION))
                }
                // `<->` is an expression all the same, and rarely written.
                LT_MINUS_GT => {
                    parser.abandon(marker);
                    parser.rollback(before);
                    let marker = parser.start();
                    expr(parser);
                    terminated(parser, marker, before)
                }
                _ => terminated(parser, marker, before),
            }
        }
    }
}

/// Whether the `{` at the cursor opens constraints rather than a
/// concatenation: they are empty, or hold a `;`, which no expression does.
fn is_block<T: Tokens>(parser: &Parser<T>) -> bool {
    let end = parser.past_group(0, L_BRACE, R_BRACE);
    parser.kind(1) == R_BRACE || (1..end).any(|ahead| parser.kind(ahead) == SEMICOLON)
}

/// Parses `solve`, the variables solved first, `before`, the ones after, and
/// `;`.
fn solve_before<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    variables(parser);
    if !parser.at(BEFORE_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    variables(parser);
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, SOLVE_BEFORE))
}

/// Parses the variables on one side of `before`, between commas.
fn variables<T: Tokens>(parser: &mut Parser<T>) {
    while expr(parser).is_some() && parser.at(COMMA) {
        parser.bump();
    }
}

/// Completes `marker` as a [`CONSTRAINT_EXPR`] if a `;` follows; declines
/// otherwise.
fn terminated<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, CONSTRAINT_EXPR))
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
