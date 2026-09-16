//! Statements: the blocks, loops and conditionals a procedural block holds.
//!
//! # The same shapes serve `generate`
//!
//! `if`, `case`, `for` and `begin` … `end` are written identically in a module
//! and in an `always` block, and differ only in what their bodies may contain.
//! So there is one rule for each and no generate-specific kinds: the body is
//! parsed through [`super::any`], which asks the
//! [scope](super::Scope) what the text is made of. A generate `for` is a
//! [`FOR_STMT`] over module items, which is the truth -- the same syntax over
//! a different body -- rather than a second node kind saying the same thing.
//!
//! # A left-hand side is not an expression
//!
//! `a <= b;` is a nonblocking assignment, and `<=` is also the relational
//! operator sitting at level 9 of the precedence table. Handing the statement
//! rule an [`expr`] would therefore hand it a [`BIN_EXPR`] over the whole
//! line, with the assignment nowhere in the tree.
//!
//! So the left-hand side is parsed as an *lvalue* -- a primary and its
//! postfixes, and nothing binary -- which is exactly what A.8.5 admits there.
//! The operator is then whatever follows, and [`ASSIGNMENT`] is built by
//! reopening the lvalue from the outside. This is the other half of the
//! bargain [`mod@super::expr`] struck by leaving `=` out of its table.
//!
//! # Stopping, rather than failing
//!
//! [`statement`] always makes progress: what no rule can shape becomes a
//! [`VERBATIM`] run. A rule that cannot finish what it started gives its
//! tokens back first, so the run begins where the statement began rather than
//! in the middle of what a rule half understood.

use super::decl::{at_declarator_only, data_type, declaration_at, declarators, semicolon};
use super::event::{Completed, Marker};
use super::expr::{attributes, expr, lvalue};
use super::source::{Position, Tokens};
use super::verbatim::{Context, verbatim};
use super::{Parser, Snapshot, any, preprocessor};
use crate::{SyntaxKind, SyntaxKind::*};

/// Parses one statement at the cursor, falling back where no rule fits.
///
/// Always takes at least one token unless the cursor is at the end or already
/// at `limit`.
pub fn statement<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && preprocessor::any(parser) {
        return;
    }
    if one(parser, limit).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated, limit);
}

/// One statement, or `None` with the cursor and the events put back.
fn one<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();
    attributes(parser);
    statement_at(parser, marker, before, limit)
}

/// The same, into a node the caller has already opened and taken the
/// attributes into.
///
/// What [`item`](super::item) calls for a generate loop or conditional, which
/// is this rule over an item body and nothing else.
pub(super) fn statement_at<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    match parser.kind(0) {
        BEGIN_KW => block(parser, marker, limit),
        FORK_KW => block(parser, marker, limit),

        // `unique`, `unique0` and `priority` qualify the `if` or `case` they
        // are written in front of, and belong inside its node.
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
        // `disable fork;` names no block, and `fork` is a keyword rather than
        // the label the other form writes.
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

        // `-> ev;` triggers a named event; `->>` is the nonblocking form,
        // which may carry a delay of its own.
        MINUS_GT | MINUS_GT_GT => {
            parser.bump();
            timing_control(parser);
            expr(parser);
            terminated(parser, marker, before, EVENT_TRIGGER)
        }

        AT | HASH | HASH_HASH => {
            timing_control(parser);
            // `@(posedge clk);` waits and does nothing else.
            if parser.at(SEMICOLON) {
                parser.bump();
            } else {
                any(parser, limit);
            }
            Some(parser.complete(marker, TIMING_STMT))
        }

        // `name : statement`. The label belongs to what follows it, which is
        // why it is a node rather than two tokens beside one.
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
            // `declaration_at` abandons the marker it was given, so what is
            // rolled back here is whatever it read before deciding.
            parser.rollback(before);
            let marker = parser.start();
            attributes(parser);
            expr_stmt(parser, marker, before)
        }
    }
}

/// Gives back the tokens and the events, for a rule that could not finish.
fn decline<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.abandon(marker);
    parser.rollback(before);
    None
}

/// Closes `kind` if the `;` that ends it is there, and declines if it is not.
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

/// `begin` … `end` or `fork` … `join`, with the labels either may carry.
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

    // Not all or nothing, unlike the constructs with a name: a block that
    // never finds its `end` has swallowed the rest of what encloses it
    // anyway, and giving the tokens back would only move where that shows.
    body(parser, |kind| closers.contains(&kind), limit, any);

    if closers.contains(&parser.kind(0)) {
        parser.bump();
        label(parser);
    }
    Some(parser.complete(marker, BLOCK))
}

/// `if ( … ) … else …`, nesting to the right through an `else if`.
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

/// `case ( … ) … endcase`, and the `casex`, `casez`, `inside` and `matches`
/// forms, which differ in how an arm is compared rather than in shape.
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

    // All or nothing: a `case` whose `endcase` never came was misread, and a
    // node over half of one would put the fallback inside it.
    if !parser.at(ENDCASE_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, CASE_STMT))
}

/// One arm of a [`CASE_STMT`]: the values it matches, and what it runs.
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

/// `for ( init ; condition ; step ) …`.
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

/// A `for` header's first clause, which either declares its variables or
/// assigns to ones already declared.
fn initialiser<T: Tokens>(parser: &mut Parser<T>) {
    // `genvar` says a declaration outright; otherwise the same question a
    // declaration asks anywhere -- is the first name the type, or the thing
    // being named?
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

/// `foreach ( … ) …`, `while ( … ) …`, `repeat ( … ) …`: a parenthesised
/// header and one statement.
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

/// `do … while ( … );`.
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

/// `wait ( … ) …`, `wait fork;`, and the `wait_order` form left to the
/// fallback.
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

/// An assignment, a call, or the null statement, and the `;` that ends it.
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

/// A left-hand side and what is assigned to it, or just the left-hand side.
///
/// Answers the lvalue itself where no operator follows, because that is what
/// a call statement and a `for` step both look like.
pub(super) fn assignment<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let lhs = lvalue(parser)?;
    if !is_assignment(parser.kind(0)) {
        return Some(lhs);
    }

    let marker = parser.precede(lhs);
    parser.bump();
    // `a <= #1 b;` and `a = @(posedge clk) b;` delay the assignment itself.
    timing_control(parser);
    expr(parser);
    Some(parser.complete(marker, ASSIGNMENT))
}

/// Whether `kind` assigns rather than compares.
///
/// `<=` is here and also in the precedence table, because the same two bytes
/// are the nonblocking assignment and the relational operator. Which one it
/// is, is where it is written -- the lexer does not know and does not
/// pretend to.
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

/// A timing control, if one is written at the cursor.
///
/// `true` when it took one, so that a caller can tell a delayed assignment
/// from an undelayed one without looking twice.
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

/// `@(posedge clk or negedge rst_n)`, `@*`, `@(*)`, `@ev`.
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
        // `@*` -- every signal the block reads, written as a wildcard.
        STAR => parser.bump(),
        _ => {
            expr(parser);
        }
    }

    parser.complete(marker, EVENT_CONTROL);
}

/// A sensitivity list: edges, names, and the `iff` that guards one.
///
/// Separated by `,` or by `or`, which mean the same thing here -- `or` is a
/// keyword rather than the binary operator, which is why [`expr`] stops at it
/// and this loop can take it.
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

/// `#5`, `#1ns`, `#(1:2:3)`, `##2`.
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

/// A parenthesised header, taken whole whatever is inside it.
fn condition<T: Tokens>(parser: &mut Parser<T>) {
    if !parser.at(L_PAREN) {
        return;
    }
    let marker = parser.start();
    parser.bump();
    expr(parser);
    close(parser, marker);
}

/// Takes whatever is left of an open `(` and closes `marker` over the pair.
///
/// A header the expression rule did not finish -- a sensitivity list, a
/// `foreach`'s index list, a `for`'s three clauses -- still ends where it was
/// written to, and the node covers its own parentheses either way. Bounded by
/// the nesting rather than by the first `)`, so an inner call does not end it.
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

/// Everything up to whatever closes the construct, one `element` at a time.
///
/// The `element` a block takes is [`any`], which asks the
/// [scope](super::Scope) what the text is made of -- which is how a generate
/// block and a statement block share this rule. A `case` takes arms instead.
///
/// The progress check is the loop's own safety net rather than a claim about
/// the rules: one that took nothing would spin here forever, and a `break` is
/// a run of verbatim rather than a hang.
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

/// `: name`, as a block writes one at either end.
pub(super) fn label<T: Tokens>(parser: &mut Parser<T>) {
    if parser.at(COLON) && matches!(parser.kind(1), IDENT | ESCAPED_IDENT | NEW_KW) {
        parser.bump();
        parser.bump();
    }
}
