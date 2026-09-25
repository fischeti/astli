//! SystemVerilog expression parsing using operator precedence climbing.
//!
//! Binary operator precedence levels correspond to IEEE 1800-2023 Table 11-2.
//! Prefix and postfix expressions (including casts, member selections, index slices,
//! and function calls) bind tighter than binary operators and are handled directly
//! around primary expressions.

use super::event::Completed;
use super::source::{Position, Tokens};
use super::{Parser, preprocessor};
use astli_syntax::{SyntaxKind, SyntaxKind::*};

/// Returns the left and right binding powers for a binary operator, if recognised.
///
/// Precedence levels follow IEEE 1800-2023 Table 11-2:
/// Left-associative levels use `(2n, 2n + 1)` and right-associative use `(2n + 1, 2n)`.
fn binding(kind: SyntaxKind) -> Option<(u8, u8)> {
    let (level, right) = match kind {
        MINUS_GT | LT_MINUS_GT => (1, true),
        QUESTION => (2, true),
        PIPE_PIPE => (3, false),
        AMP_AMP => (4, false),
        PIPE => (5, false),
        CARET | TILDE_CARET | CARET_TILDE => (6, false),
        AMP => (7, false),
        EQ_EQ | BANG_EQ | EQ_EQ_EQ | BANG_EQ_EQ | EQ_EQ_QUESTION | BANG_EQ_QUESTION => (8, false),
        LT | LT_EQ | GT | GT_EQ | INSIDE_KW | DIST_KW => (9, false),
        LT_LT | GT_GT | LT_LT_LT | GT_GT_GT => (10, false),
        PLUS | MINUS => (11, false),
        STAR | SLASH | PERCENT => (12, false),
        STAR_STAR => (13, true),
        _ => return None,
    };
    Some(match right {
        true => (2 * level + 1, 2 * level),
        false => (2 * level, 2 * level + 1),
    })
}

/// Returns `true` if `kind` is a unary prefix operator (Sections 11.4.1, 11.4.9).
fn is_unary(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        PLUS | MINUS
            | BANG
            | TILDE
            | AMP
            | TILDE_AMP
            | PIPE
            | TILDE_PIPE
            | CARET
            | TILDE_CARET
            | CARET_TILDE
            | PLUS_PLUS
            | MINUS_MINUS
    )
}

/// Parses an expression at the cursor, returning the completed syntax node.
pub fn expr<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    binary(parser, 0)
}

/// Parses an lvalue (a primary and its postfixes, excluding binary operators).
pub(super) fn lvalue<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    unary(parser)
}

/// Parses binary expressions using operator precedence climbing with minimum binding power `min`.
fn binary<T: Tokens>(parser: &mut Parser<T>, min: u8) -> Option<Completed> {
    let mut lhs = unary(parser)?;

    loop {
        let kind = parser.kind(0);
        // Avoid treating an attribute terminator `*)` as a multiplication operator.
        if kind == STAR && parser.kind(1) == R_PAREN {
            break;
        }
        let Some((left, right)) = binding(kind) else {
            break;
        };
        if left < min {
            break;
        }

        let before = parser.snapshot();
        let marker = parser.precede(lhs);

        lhs = match kind {
            QUESTION => ternary(parser, marker, right),
            INSIDE_KW | DIST_KW => {
                parser.bump();
                range_list(parser, kind == DIST_KW);
                parser.complete(
                    marker,
                    match kind {
                        DIST_KW => DIST_EXPR,
                        _ => INSIDE_EXPR,
                    },
                )
            }
            _ => {
                parser.bump();
                attributes(parser);
                if binary(parser, right).is_none() {
                    parser.abandon(marker);
                    parser.rollback(before);
                    break;
                }
                parser.complete(marker, BIN_EXPR)
            }
        };
    }

    Some(lhs)
}

/// Parses a ternary conditional expression (`condition ? then_expr : else_expr`).
fn ternary<T: Tokens>(parser: &mut Parser<T>, marker: super::Marker, right: u8) -> Completed {
    parser.bump();
    attributes(parser);
    binary(parser, 0);
    if parser.at(COLON) {
        parser.bump();
        binary(parser, right);
    }
    parser.complete(marker, TERNARY_EXPR)
}

/// Parses an operand: optional prefix operators followed by a primary expression and postfixes.
fn unary<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    if is_unary(parser.kind(0)) {
        let before = parser.snapshot();
        let marker = parser.start();
        parser.bump();
        attributes(parser);
        if unary(parser).is_none() {
            parser.abandon(marker);
            parser.rollback(before);
            return None;
        }
        return Some(parser.complete(marker, UNARY_EXPR));
    }

    let lhs = primary(parser)?;
    Some(postfixes(parser, lhs, None))
}

/// Parses the array a `foreach` walks: an operand whose postfixes stop at
/// `loop_variables`, where the brackets naming the loop's variables start.
pub(super) fn foreach_array<T: Tokens>(
    parser: &mut Parser<T>,
    loop_variables: Position,
) -> Option<Completed> {
    let lhs = primary(parser)?;
    Some(postfixes(parser, lhs, Some(loop_variables)))
}

/// Parses the member selections, indices, calls and casts after `lhs`, up to
/// `stop` if there is one.
fn postfixes<T: Tokens>(
    parser: &mut Parser<T>,
    mut lhs: Completed,
    stop: Option<Position>,
) -> Completed {
    loop {
        if stop == Some(parser.position()) {
            break;
        }
        lhs = match parser.kind(0) {
            DOT if parser.kind(1) != STAR => {
                let marker = parser.precede(lhs);
                parser.bump();
                name(parser);
                parser.complete(marker, FIELD_EXPR)
            }
            COLON_COLON => {
                let marker = parser.precede(lhs);
                parser.bump();
                name(parser);
                parser.complete(marker, SCOPE_EXPR)
            }
            L_BRACK => {
                let marker = parser.precede(lhs);
                index(parser);
                parser.complete(marker, INDEX_EXPR)
            }
            L_PAREN => {
                let marker = parser.precede(lhs);
                arguments(parser);
                parser.complete(marker, CALL_EXPR)
            }
            APOSTROPHE if parser.kind(1) == L_PAREN => {
                let marker = parser.precede(lhs);
                parser.bump();
                paren(parser);
                parser.complete(marker, CAST_EXPR)
            }
            APOSTROPHE_L_BRACE => {
                let marker = parser.precede(lhs);
                pattern_body(parser);
                parser.complete(marker, ASSIGNMENT_PATTERN)
            }
            PLUS_PLUS | MINUS_MINUS => {
                let marker = parser.precede(lhs);
                parser.bump();
                parser.complete(marker, POSTFIX_EXPR)
            }
            HASH if parser.kind(1) == L_PAREN => {
                let marker = parser.precede(lhs);
                parser.bump();
                arguments(parser);
                parser.complete(marker, CALL_EXPR)
            }
            WITH_KW if parser.kind(1) == L_PAREN => {
                let marker = parser.precede(lhs);
                with_clause(parser);
                parser.complete(marker, CALL_EXPR)
            }
            _ => break,
        };
    }
    lhs
}

/// Parses a primary expression atom.
fn primary<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let kind = parser.kind(0);

    if kind == TICK_IDENT {
        let marker = parser.start();
        return preprocessor::any(parser).then(|| parser.complete(marker, NAME_REF));
    }

    match kind {
        IDENT | ESCAPED_IDENT | SYSTEM_IDENT | THIS_KW | SUPER_KW | NULL_KW | DOLLAR
        | DEFAULT_KW | LOCAL_KW | NEW_KW => {
            let marker = parser.start();
            parser.bump();
            Some(parser.complete(marker, NAME_REF))
        }
        _ if kind.is_keyword() && parser.kind(1) == APOSTROPHE => {
            let marker = parser.start();
            parser.bump();
            Some(parser.complete(marker, NAME_REF))
        }
        L_PAREN => Some(paren(parser)),
        L_BRACE => Some(braced(parser)),
        APOSTROPHE_L_BRACE => Some(assignment_pattern(parser)),
        STRING_LITERAL | REAL_LITERAL | TIME_LITERAL | ONE_STEP_KW | UNBASED_UNSIZED_LITERAL => {
            let marker = parser.start();
            parser.bump();
            Some(parser.complete(marker, LITERAL_EXPR))
        }
        INT_LITERAL | BASED_LITERAL | INT_BASE => Some(number(parser)),
        _ => None,
    }
}

/// Parses numeric literals, including multi-token sized and based numbers.
fn number<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();

    if parser.at(INT_LITERAL) {
        parser.bump();
    }
    if parser.at(BASED_LITERAL) {
        parser.bump();
    } else if parser.at(INT_BASE) {
        parser.bump();
        if parser.at(TICK_IDENT) {
            preprocessor::any(parser);
        } else if is_digits(parser.kind(0)) {
            parser.bump();
            while is_digits(parser.kind(0)) && parser.adjacent(0) {
                parser.bump();
            }
        }
    }

    parser.complete(marker, LITERAL_EXPR)
}

/// Returns `true` if `kind` can form part of a based numeric literal's digits.
fn is_digits(kind: SyntaxKind) -> bool {
    matches!(kind, INT_LITERAL | IDENT | REAL_LITERAL)
}

/// Consumes an identifier following a member access `.` or scope `::`.
fn name<T: Tokens>(parser: &mut Parser<T>) {
    if matches!(
        parser.kind(0),
        IDENT | ESCAPED_IDENT | SYSTEM_IDENT | NEW_KW | THIS_KW | SUPER_KW
    ) || parser.kind(0).is_keyword()
    {
        parser.bump();
    }
}

/// Parses parenthesized expressions `(expr)` or min:typ:max expressions `(min : typ : max)`.
fn paren<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    parser.bump();
    expr(parser);
    while parser.at(COLON) {
        parser.bump();
        expr(parser);
    }
    if parser.at(R_PAREN) {
        parser.bump();
    }
    parser.complete(marker, PAREN_EXPR)
}

/// Parses braced expressions: concatenations, replications, or streaming expressions.
fn braced<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    parser.bump();

    if matches!(parser.kind(0), LT_LT | GT_GT) {
        parser.bump();
        if !parser.at(L_BRACE) && expr(parser).is_none() {
            super::decl::data_type(parser);
        }
        if parser.at(L_BRACE) {
            braced(parser);
        }
        if parser.at(R_BRACE) {
            parser.bump();
        }
        return parser.complete(marker, STREAM_EXPR);
    }

    let first = expr(parser);

    if first.is_some() && parser.at(L_BRACE) {
        braced(parser);
        if parser.at(R_BRACE) {
            parser.bump();
        }
        return parser.complete(marker, REPLICATION_EXPR);
    }

    while parser.at(COMMA) {
        parser.bump();
        if expr(parser).is_none() {
            break;
        }
    }
    if parser.at(R_BRACE) {
        parser.bump();
    }
    parser.complete(marker, CONCAT_EXPR)
}

/// Parses an untyped assignment pattern `'{ ... }`.
fn assignment_pattern<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    pattern_body(parser);
    parser.complete(marker, ASSIGNMENT_PATTERN)
}

/// Parses the body elements within an assignment pattern `'{ ... }`.
fn pattern_body<T: Tokens>(parser: &mut Parser<T>) {
    parser.bump();

    while !parser.at_end() && !parser.at(R_BRACE) {
        let item = parser.start();
        let first = expr(parser);

        if parser.at(COLON) {
            parser.bump();
            expr(parser);
        } else if let Some(count) = first
            && parser.at(L_BRACE)
        {
            let marker = parser.precede(count);
            braced(parser);
            parser.complete(marker, REPLICATION_EXPR);
        }

        parser.complete(item, PATTERN_ITEM);

        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if parser.at(R_BRACE) {
        parser.bump();
    }
}

/// Parses bracketed index or part-select expressions `[i]`, `[hi:lo]`, `[base +: width]`.
fn index<T: Tokens>(parser: &mut Parser<T>) {
    parser.bump();
    expr(parser);
    if matches!(parser.kind(0), COLON | PLUS_COLON | MINUS_COLON) {
        parser.bump();
        expr(parser);
    }
    if parser.at(R_BRACK) {
        parser.bump();
    }
}

/// Parses parenthesized subroutine or port arguments `(arg1, .port(expr), ...)`.
pub(super) fn arguments<T: Tokens>(parser: &mut Parser<T>) {
    let list = parser.start();
    parser.bump();

    loop {
        let argument = parser.start();
        if parser.at(DOT) {
            parser.bump();
            if parser.at(STAR) {
                parser.bump();
            } else {
                name(parser);
                if parser.at(L_PAREN) {
                    paren(parser);
                }
            }
        } else if expr(parser).is_none() {
            super::decl::data_type(parser);
        }
        parser.complete(argument, ARG);

        if parser.at(COMMA) {
            parser.bump();
            continue;
        }
        break;
    }

    if parser.at(R_PAREN) {
        parser.bump();
    }
    parser.complete(list, ARG_LIST);
}

/// Parses the braced list of an `inside` or `dist` expression.
fn range_list<T: Tokens>(parser: &mut Parser<T>, weighted: bool) {
    if !parser.at(L_BRACE) {
        return;
    }
    let list = parser.start();
    parser.bump();

    while !parser.at_end() && !parser.at(R_BRACE) {
        let item = weighted.then(|| parser.start());

        if parser.at(L_BRACK) {
            index(parser);
        } else if expr(parser).is_none() {
            if let Some(item) = item {
                parser.abandon(item);
            }
            break;
        }

        if let Some(item) = item {
            if matches!(parser.kind(0), COLON_EQ | COLON_SLASH) {
                parser.bump();
                expr(parser);
            }
            parser.complete(item, DIST_ITEM);
        }

        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if parser.at(R_BRACE) {
        parser.bump();
    }
    parser.complete(list, RANGE_LIST);
}

/// Parses an array method `with (expr)` clause.
fn with_clause<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    paren(parser);
    parser.complete(marker, WITH_CLAUSE);
}

/// Parses every attribute instance at the cursor, `(* name *)` or
/// `(* name = value *)`, each its own node.
pub fn attributes<T: Tokens>(parser: &mut Parser<T>) {
    while parser.at(L_PAREN) && parser.kind(1) == STAR {
        attribute(parser);
    }
}

/// Parses one attribute instance.
fn attribute<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    parser.bump();

    while !parser.at_end() {
        if parser.at(STAR) && parser.kind(1) == R_PAREN {
            parser.bump();
            parser.bump();
            break;
        }

        let spec = parser.start();
        name(parser);
        if parser.at(EQ) {
            parser.bump();
            expr(parser);
        }
        parser.complete(spec, ATTRIBUTE_SPEC);

        if parser.at(COMMA) {
            parser.bump();
        } else if !(parser.at(STAR) && parser.kind(1) == R_PAREN) {
            break;
        }
    }

    parser.complete(marker, ATTRIBUTES);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::one;

    fn parse(text: &str) -> (Option<String>, String) {
        one(text, |parser| expr(parser).is_some())
    }

    #[test]
    fn nothing_that_starts_an_expression_means_nothing_taken() {
        assert_eq!(parse("; a"), (None, "; a".to_string()));
    }

    #[test]
    fn a_prefix_operator_with_no_operand_is_given_back_whole() {
        // `None` has to mean nothing was consumed, or a caller cannot fall
        // back: the tokens it would hand to the fallback would be gone.
        assert_eq!(parse("- ;"), (None, "- ;".to_string()));
    }

    #[test]
    fn an_expression_stops_at_the_first_token_it_cannot_use() {
        assert_eq!(parse("a + b; c").1, "; c");
    }

    #[test]
    fn a_trailing_operator_leaves_the_operator_behind() {
        // There is no error node to put a missing operand in, so the rule
        // takes what it can and hands the rest back.
        assert_eq!(parse("a +"), (Some("a".to_string()), " +".to_string()));
    }
}
