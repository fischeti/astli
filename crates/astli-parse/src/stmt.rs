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

use super::decl::{Init, at_declarator_only, data_type, declaration_at, declarator, semicolon};
use super::delimiters;
use super::event::{Completed, Marker};
use super::expr::{attributes, delay_value, expr, foreach_array, lvalue, min_typ_max, value_range};
use super::source::Tokens;
use super::verbatim::{Context, rest_as_verbatim, verbatim};
use super::{Parser, Scope, Snapshot, any, entries, preprocessor};
use astli_syntax::{SyntaxKind, SyntaxKind::*};

/// Parses a statement at the cursor, falling back to verbatim recovery if no rule matches.
pub fn statement<T: Tokens>(parser: &mut Parser<T>) {
    if parser.at(TICK_IDENT) && !preprocessor::continued(parser) && preprocessor::any(parser) {
        return;
    }
    if one(parser).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated);
}

/// Attempts to parse a single statement, rolling back on failure.
fn one<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();
    attributes(parser);
    statement_at(parser, marker, before)
}

/// Parses a statement into `marker`, which has already consumed leading attributes.
pub(super) fn statement_at<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    match parser.kind(0) {
        BEGIN_KW => block(parser, marker),
        FORK_KW => block(parser, marker),

        UNIQUE_KW | UNIQUE0_KW | PRIORITY_KW => {
            parser.bump();
            match parser.kind(0) {
                IF_KW => if_stmt(parser, marker),
                CASE_KW | CASEX_KW | CASEZ_KW => case_stmt(parser, marker, before, false),
                _ => parser.decline(marker, before),
            }
        }
        IF_KW => if_stmt(parser, marker),
        CASE_KW | CASEX_KW | CASEZ_KW | RANDCASE_KW => case_stmt(parser, marker, before, false),
        RANDSEQUENCE_KW => randsequence(parser, marker, before),

        FOR_KW => for_stmt(parser, marker),
        FOREACH_KW => foreach_stmt(parser, marker),
        WHILE_KW => loop_stmt(parser, marker, WHILE_STMT),
        REPEAT_KW => loop_stmt(parser, marker, REPEAT_STMT),
        FOREVER_KW => {
            parser.bump();
            any(parser);
            Some(parser.complete(marker, FOREVER_STMT))
        }
        DO_KW => do_while(parser, marker, before),

        RETURN_KW => {
            parser.bump();
            expr(parser);
            parser.terminated(marker, before, RETURN_STMT)
        }
        BREAK_KW => {
            parser.bump();
            parser.terminated(marker, before, BREAK_STMT)
        }
        CONTINUE_KW => {
            parser.bump();
            parser.terminated(marker, before, CONTINUE_STMT)
        }
        DISABLE_KW => {
            parser.bump();
            if parser.at(FORK_KW) {
                parser.bump();
            } else {
                expr(parser);
            }
            parser.terminated(marker, before, DISABLE_STMT)
        }
        WAIT_KW => wait_stmt(parser, marker, before),
        WAIT_ORDER_KW => wait_order(parser, marker, before),
        // A function's or a task's ports, declared in its body.
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW => super::item::port_decl(parser, marker, before),
        CONST_KW if parser.kind(1) == REF_KW => super::item::port_decl(parser, marker, before),
        // A block, a function or a task may import into its own scope.
        IMPORT_KW if super::item::at_package_import(parser) => {
            super::item::import_decl(parser, marker, before)
        }
        // Among items, `assign` is a continuous assignment, taken before this.
        FORCE_KW | RELEASE_KW | ASSIGN_KW | DEASSIGN_KW => {
            parser.bump();
            if assignment(parser).is_none() {
                return parser.decline(marker, before);
            }
            parser.terminated(marker, before, PROCEDURAL_ASSIGN)
        }
        ASSERT_KW | ASSUME_KW | COVER_KW if is_immediate(parser, 0) => {
            immediate_assertion(parser, marker)
        }
        _ if super::property::is_concurrent(parser, 0) => {
            super::property::assertion(parser, marker, before)
        }

        MINUS_GT | MINUS_GT_GT => {
            parser.bump();
            delay_or_event_control(parser);
            expr(parser);
            parser.terminated(marker, before, EVENT_TRIGGER)
        }

        AT | HASH | HASH_HASH => {
            timing_control(parser);
            if parser.at(SEMICOLON) {
                parser.bump();
            } else {
                any(parser);
            }
            Some(parser.complete(marker, TIMING_STMT))
        }

        IDENT | ESCAPED_IDENT if parser.kind(1) == COLON => {
            parser.bump();
            parser.bump();
            any(parser);
            Some(parser.complete(marker, LABELED_STMT))
        }

        // A checker may be instantiated in a procedure, and checks there.
        _ if super::item::at_instantiation(parser, false) => {
            super::item::instantiation(parser, marker, before)
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

/// Parses a `begin` … `end` or `fork` … `join` block statement.
fn block<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> Option<Completed> {
    let closers = delimiters::closers(parser.kind(0));
    parser.bump();
    parser.label();

    entries(parser, parser.scope, closers);

    if closers.contains(&parser.kind(0)) {
        parser.bump();
        parser.label();
    }
    Some(parser.complete(marker, BLOCK))
}

/// Parses an `if (cond) stmt else stmt` conditional statement.
pub(super) fn if_stmt<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> Option<Completed> {
    parser.bump();
    condition(parser);
    any(parser);
    if parser.at(ELSE_KW) {
        parser.bump();
        any(parser);
    }
    Some(parser.complete(marker, IF_STMT))
}

/// Parses a `case` / `casex` / `casez` selection statement, or with
/// `generate` a generate `case`, whose arms are items.
pub(super) fn case_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    generate: bool,
) -> Option<Completed> {
    // A `randcase` has no expression; its arms' values are weights.
    let random = parser.at(RANDCASE_KW);
    parser.bump();
    if !random {
        condition(parser);
    }
    let patterns = parser.at(MATCHES_KW);
    if matches!(parser.kind(0), MATCHES_KW | INSIDE_KW) {
        parser.bump();
    }

    let scope = Scope::CaseItems { generate, patterns };
    entries(parser, scope, &[ENDCASE_KW]);

    if !parser.at(ENDCASE_KW) {
        return parser.decline(marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, CASE_STMT))
}

/// Parses `randsequence`, the production it starts from, and its productions.
fn randsequence<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    condition(parser);
    entries(parser, Scope::Productions, &[ENDSEQUENCE_KW]);
    if !parser.at(ENDSEQUENCE_KW) {
        return parser.decline(marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, RANDSEQUENCE_STMT))
}

/// Parses a production: its return type and arguments if it takes them, and
/// the rules after its `:`, `|` between them; or a directive or conditional
/// region among them.
pub(super) fn production<T: Tokens>(parser: &mut Parser<T>) {
    // A macro there is more likely the production's type than a production.
    if parser.at(TICK_IDENT) && parser.macro_call().is_none() && preprocessor::any(parser) {
        return;
    }
    let marker = parser.start();
    // A name straight before its `:` or `(` has no return type.
    let named = matches!(parser.kind(0), IDENT | ESCAPED_IDENT)
        && matches!(parser.kind(1), COLON | L_PAREN);
    if !named {
        data_type(parser);
    }
    if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        parser.bump();
    }
    if parser.at(L_PAREN) {
        super::item::port_list(parser);
    }
    if parser.at(COLON) {
        parser.bump();
    }
    loop {
        production_rule(parser);
        if !parser.at(PIPE) {
            break;
        }
        parser.bump();
    }
    semicolon(parser);
    parser.complete(marker, PRODUCTION);
}

/// Parses one rule of a production: `rand join` if its items interleave, the
/// items, then `:=`, its weight and the block that runs when it is chosen.
fn production_rule<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    if parser.at(RAND_KW) && parser.kind(1) == JOIN_KW {
        parser.bump();
        parser.bump();
        condition(parser);
    }
    while production_item(parser) {}
    if parser.at(COLON_EQ) {
        parser.bump();
        // One operand, not an expression: a `|` after it starts the next rule.
        lvalue(parser);
        if parser.at(L_BRACE) {
            production_block(parser);
        }
    }
    parser.complete(marker, PRODUCTION_RULE);
}

/// Parses one item of a rule, and says whether there was one.
fn production_item<T: Tokens>(parser: &mut Parser<T>) -> bool {
    match parser.kind(0) {
        L_BRACE => production_block(parser),
        IF_KW => {
            let marker = parser.start();
            parser.bump();
            condition(parser);
            production_call(parser);
            if parser.at(ELSE_KW) {
                parser.bump();
                production_call(parser);
            }
            parser.complete(marker, PRODUCTION_IF);
        }
        REPEAT_KW => {
            let marker = parser.start();
            parser.bump();
            condition(parser);
            production_call(parser);
            parser.complete(marker, PRODUCTION_REPEAT);
        }
        CASE_KW => production_case(parser),
        IDENT | ESCAPED_IDENT => production_call(parser),
        _ => return false,
    }
    true
}

/// Parses a production's name, and its arguments if it takes them.
fn production_call<T: Tokens>(parser: &mut Parser<T>) {
    if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        lvalue(parser);
    }
}

/// Parses the braced declarations and statements a production runs.
fn production_block<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    // What a production runs is statements.
    entries(parser, Scope::Statement, &[R_BRACE]);
    if parser.at(R_BRACE) {
        parser.bump();
    }
    parser.complete(marker, PRODUCTION_BLOCK);
}

/// Parses a `case` among a rule's items, each arm a production and its `;`.
fn production_case<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    condition(parser);
    while !parser.at_end() && !parser.at(ENDCASE_KW) {
        let item = parser.start();
        let at = parser.position();
        if parser.at(DEFAULT_KW) {
            parser.bump();
        } else {
            while expr(parser).is_some() && parser.at(COMMA) {
                parser.bump();
            }
        }
        if parser.at(COLON) {
            parser.bump();
        }
        production_call(parser);
        semicolon(parser);
        parser.complete(item, PRODUCTION_CASE_ITEM);
        if parser.position() == at {
            break;
        }
    }
    if parser.at(ENDCASE_KW) {
        parser.bump();
    }
    parser.complete(marker, PRODUCTION_CASE);
}

/// Parses a single arm within a `case` statement, or a conditional region
/// of them: an item in a generate `case`, and patterns its values in a
/// `case … matches`.
pub(super) fn case_item<T: Tokens>(parser: &mut Parser<T>, generate: bool, patterns: bool) {
    // A macro before a `:` is the arm's value. One followed by no `:`, `,` or
    // operator cannot be a value, so it writes whole arms, as a
    // `` `DEFAULT_ITEM `` does.
    let arms = parser.macro_call().is_some_and(|len| {
        !matches!(parser.kind(len as usize), COLON | COMMA) && !preprocessor::continued(parser)
    });
    if parser.at(TICK_IDENT) && (parser.macro_call().is_none() || arms) && preprocessor::any(parser)
    {
        return;
    }
    let marker = parser.start();

    let outer = std::mem::replace(&mut parser.pattern, patterns);
    if parser.at(DEFAULT_KW) {
        parser.bump();
    } else {
        loop {
            // A branch among the values holds values and the commas after
            // them, as one among a call's arguments does, so the value after
            // the region follows no comma.
            let region = parser.region().is_some();
            // `[lo:hi]`, in a `case … inside`.
            if parser.at(L_BRACK) {
                value_range(parser);
            } else if region {
                let scope = parser.set_scope(Scope::Arguments { values: false });
                expr(parser);
                parser.set_scope(scope);
            } else if expr(parser).is_none() {
                break;
            }
            if parser.at(COMMA) {
                parser.bump();
            } else if !region {
                break;
            }
        }
    }

    parser.pattern = outer;

    if parser.at(COLON) {
        parser.bump();
    }
    let arm = match generate {
        true => Scope::Item,
        false => Scope::Statement,
    };
    let scope = parser.set_scope(arm);
    any(parser);
    parser.set_scope(scope);

    parser.complete(marker, CASE_ITEM);
}

/// Parses a `for (init; cond; step)` loop statement.
fn for_stmt<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> Option<Completed> {
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
        rest_as_verbatim(parser, header, R_PAREN, PAREN_EXPR);
    }

    any(parser);
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

    // Each declaration has a type of its own: `int i = 0, state_e s = s.first()`
    // declares two, where `int i = 0, j = 1` declares one with two names.
    loop {
        let marker = parser.start();
        if parser.at(VAR_KW) {
            parser.bump();
        }
        if parser.at(GENVAR_KW) {
            parser.bump();
        } else {
            data_type(parser);
        }
        while declarator(parser, Init::Value).is_some() && parser.at(COMMA) && names_next(parser) {
            parser.bump();
        }
        parser.complete(marker, VAR_DECL);

        if !parser.at(COMMA) {
            break;
        }
        parser.bump();
    }
}

/// Whether the `,` at the cursor is followed by another name of the same
/// declaration, where every name has a value, rather than the next one's type.
fn names_next<T: Tokens>(parser: &Parser<T>) -> bool {
    matches!(parser.kind(1), IDENT | ESCAPED_IDENT) && matches!(parser.kind(2), EQ | L_BRACK)
}

/// Parses a `foreach (array[i, j])` loop statement.
pub(super) fn foreach_stmt<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> Option<Completed> {
    parser.bump();
    if parser.at(L_PAREN) {
        foreach_header(parser);
    }
    any(parser);
    Some(parser.complete(marker, FOREACH_STMT))
}

/// Parses a `foreach` header: the array, then in brackets the variables the
/// loop declares, any of which may be left out, as in `a[, j]`.
///
/// Those brackets are not an index, since they hold names separated by
/// commas; they are the last brackets before the `)`, and the array is
/// everything before them.
fn foreach_header<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    match loop_variables(parser) {
        Some(ahead) => {
            let at = parser.ahead(ahead);
            foreach_array(parser, at);
            if parser.position() == at {
                parser.bump();
                while matches!(parser.kind(0), IDENT | ESCAPED_IDENT | COMMA) {
                    parser.bump();
                }
                if parser.at(R_BRACK) {
                    parser.bump();
                }
            }
        }
        None => {
            expr(parser);
        }
    }
    rest_as_verbatim(parser, marker, R_PAREN, FOREACH_HEADER);
}

/// How far ahead the brackets naming a `foreach`'s variables start: the last
/// brackets before the `)` that closes the header.
fn loop_variables<T: Tokens>(parser: &Parser<T>) -> Option<u32> {
    let mut ahead = 0;
    loop {
        match parser.kind(ahead) {
            L_BRACK => {
                let past = parser.past_group(ahead);
                if parser.kind(past) == R_PAREN {
                    return u32::try_from(ahead).ok();
                }
                ahead = past;
            }
            L_PAREN => ahead = parser.past_group(ahead),
            R_PAREN | SEMICOLON | EOF => return None,
            _ => ahead += 1,
        }
    }
}

/// Parses single-condition loop headers (`while`, `repeat`).
fn loop_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    kind: SyntaxKind,
) -> Option<Completed> {
    parser.bump();
    condition(parser);
    any(parser);
    Some(parser.complete(marker, kind))
}

/// Parses a `do statement while (condition);` loop statement.
fn do_while<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    any(parser);
    if !parser.at(WHILE_KW) {
        return parser.decline(marker, before);
    }
    parser.bump();
    condition(parser);
    parser.terminated(marker, before, DO_WHILE_STMT)
}

/// Parses a `wait (cond)` or `wait fork;` statement.
fn wait_stmt<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    if parser.at(FORK_KW) {
        parser.bump();
        return parser.terminated(marker, before, WAIT_STMT);
    }
    if !parser.at(L_PAREN) {
        return parser.decline(marker, before);
    }
    condition(parser);
    if parser.at(SEMICOLON) {
        parser.bump();
    } else {
        any(parser);
    }
    Some(parser.complete(marker, WAIT_STMT))
}

/// Parses `wait_order`, the events it waits for in turn, and what it runs
/// when they come in that order and when they do not.
fn wait_order<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    if !parser.at(L_PAREN) {
        return parser.decline(marker, before);
    }
    let events = parser.start();
    parser.bump();
    while lvalue(parser).is_some() && parser.at(COMMA) {
        parser.bump();
    }
    rest_as_verbatim(parser, events, R_PAREN, PAREN_EXPR);
    if !parser.at(ELSE_KW) {
        any(parser);
    }
    if parser.at(ELSE_KW) {
        parser.bump();
        any(parser);
    }
    Some(parser.complete(marker, WAIT_ORDER_STMT))
}

/// Returns `true` if an `assert`, `assume` or `cover` `ahead` of the cursor
/// checks a condition once, rather than a property or a sequence throughout.
pub(super) fn is_immediate<T: Tokens>(parser: &Parser<T>, ahead: usize) -> bool {
    if !matches!(parser.kind(ahead), ASSERT_KW | ASSUME_KW | COVER_KW) {
        return false;
    }
    match parser.kind(ahead + 1) {
        L_PAREN | FINAL_KW => true,
        HASH => parser.kind(ahead + 2) == INT_LITERAL && parser.kind(ahead + 3) == L_PAREN,
        _ => false,
    }
}

/// Parses an immediate or deferred assertion: `assert`, `assume` or `cover`,
/// `#0` or `final`, the condition, and what runs when it holds and when it
/// does not.
fn immediate_assertion<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> Option<Completed> {
    // A `cover` runs a statement when the condition holds, and nothing when
    // it does not.
    let cover = parser.at(COVER_KW);
    parser.bump();
    if parser.at(FINAL_KW) {
        parser.bump();
    } else if parser.at(HASH) {
        parser.bump();
        parser.bump();
    }
    condition(parser);
    // What it runs is a statement, even when it stands among items.
    let scope = parser.set_scope(Scope::Statement);
    if cover || !parser.at(ELSE_KW) {
        any(parser);
    }
    if !cover && parser.at(ELSE_KW) {
        parser.bump();
        any(parser);
    }
    parser.set_scope(scope);
    Some(parser.complete(marker, IMMEDIATE_ASSERTION))
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
        return parser.decline(marker, before);
    }
    parser.terminated(marker, before, EXPR_STMT)
}

/// Parses an lvalue followed by an optional assignment operator and expression.
///
/// Declines, consuming nothing, if the operator has no expression after it.
pub(super) fn assignment<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let before = parser.snapshot();
    let lhs = lvalue(parser)?;
    if !is_assignment(parser.kind(0)) {
        return Some(lhs);
    }

    let marker = parser.precede(lhs);
    parser.bump();
    delay_or_event_control(parser);
    if expr(parser).is_none() {
        return parser.decline(marker, before);
    }
    Some(parser.complete(marker, ASSIGNMENT))
}

/// Returns `true` if `kind` is an assignment operator.
pub(super) fn is_assignment(kind: SyntaxKind) -> bool {
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

/// Parses what may delay an assignment's value or a nonblocking trigger: a
/// delay, an event, or an event waited for a number of times,
/// `repeat (3) @(posedge clk)`.
fn delay_or_event_control<T: Tokens>(parser: &mut Parser<T>) {
    if !(parser.at(REPEAT_KW) && parser.kind(1) == L_PAREN) {
        timing_control(parser);
        return;
    }
    let marker = parser.start();
    parser.bump();
    condition(parser);
    if parser.at(AT) {
        event_control(parser);
    }
    parser.complete(marker, REPEAT_CONTROL);
}

/// Parses an `@(...)` event control expression.
fn event_control<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();

    match parser.kind(0) {
        L_PAREN => {
            let list = parser.start();
            parser.bump();
            // `@(*)`: every signal the statement reads.
            if parser.at(STAR) && parser.kind(1) == R_PAREN {
                parser.bump();
            } else {
                event_expr(parser, true);
            }
            rest_as_verbatim(parser, list, R_PAREN, PAREN_EXPR);
        }
        STAR => parser.bump(),
        _ => {
            expr(parser);
        }
    }

    parser.complete(marker, EVENT_CONTROL);
}

/// Parses event expressions within a sensitivity list, joined by `or`, and
/// by `,` where `commas` says a `,` does not end the list.
pub(super) fn event_expr<T: Tokens>(parser: &mut Parser<T>, commas: bool) {
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
        if parser.at(OR_KW) || (commas && parser.at(COMMA)) {
            parser.bump();
        } else {
            break;
        }
    }
}

/// Parses a `#` delay or a `##` cycle delay: a value, or a list of them in
/// parentheses.
pub(super) fn delay_control<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();

    if parser.at(L_PAREN) {
        delay_values(parser);
    } else if delay_value(parser).is_none() && !parser.at_end() {
        parser.bump();
    }

    parser.complete(marker, DELAY_CONTROL);
}

/// Parses a list of delays in parentheses, each `min:typ:max` or one value.
/// A net's or a gate's delay may give up to three, for rise, fall and
/// turn-off; a path's, up to twelve.
pub(super) fn delay_values<T: Tokens>(parser: &mut Parser<T>) {
    let list = parser.start();
    parser.bump();
    loop {
        if let Some(min) = expr(parser) {
            min_typ_max(parser, min);
        }
        if !parser.at(COMMA) {
            break;
        }
        parser.bump();
    }
    rest_as_verbatim(parser, list, R_PAREN, PAREN_EXPR);
}

/// Parses a parenthesized expression or condition header.
pub(super) fn condition<T: Tokens>(parser: &mut Parser<T>) {
    if !parser.at(L_PAREN) {
        return;
    }
    let marker = parser.start();
    parser.bump();
    expr(parser);
    rest_as_verbatim(parser, marker, R_PAREN, PAREN_EXPR);
}
