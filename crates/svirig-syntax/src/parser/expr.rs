//! Expressions, by precedence climbing over the table in 1800-2023 11.3.2.
//!
//! # Why not the grammar as written
//!
//! Annex A gives `expression ::= primary | expression binary_operator {
//! attribute_instance } expression | …`, which is ambiguous on purpose: it
//! explains the language and defers precedence to a table elsewhere. Reading
//! the table instead produces a shape the standard never names -- [`BIN_EXPR`],
//! [`UNARY_EXPR`], [`TERNARY_EXPR`] -- and none of `expression`,
//! `binary_operator` or the whole `constant_expression` layer becomes a node.
//! That is `docs/plan.md`'s D11 in miniature: where an expression may appear
//! is said by which function calls this one, not by a node wrapping it.
//!
//! # A primary is always a node
//!
//! Even a bare `42`. Postfix -- `.b`, `::b`, `[i]`, `(args)`, `'(cast)` --
//! reopens what precedes it with [`Completed::precede`], so every primary has
//! to be something that can be reopened. It also gives the formatter one thing
//! to match on rather than two.
//!
//! [`LITERAL_EXPR`] earns the node twice over, because a number may be **lexed
//! in pieces**: `8 'h FF` is three tokens and one value, and the node is what
//! makes "do not come between these" representable rather than a rule the
//! formatter has to remember.
//!
//! # Stopping, rather than failing
//!
//! There is no diagnostics layer, so [`expr`] answers `None` when nothing at
//! the cursor can start an expression, and otherwise parses as far as it can
//! and stops at the first token it cannot use. It never leaves a half-built
//! node behind.
//!
//! A caller that needs the *whole* of something therefore checks where it
//! stopped and rolls back, which is what [`Parser::snapshot`] is for. Handing
//! back a partial expression rather than an error node is the same bargain the
//! [verbatim fallback](mod@super::verbatim) strikes: degrade to leaving the bytes
//! alone rather than failing the file.
//!
//! # What is deliberately not here
//!
//! **Assignment is not a binary operator.** `a = b` is a statement, and
//! letting `=` bind here would make step 9's job harder for the sake of the
//! parenthesised form nobody writes. The precedence table's bottom row is
//! therefore missing on purpose.

use super::event::Completed;
use super::source::Tokens;
use super::{Parser, preprocessor};
use crate::{SyntaxKind, SyntaxKind::*};

/// How tightly an operator binds on each side.
///
/// Left-associative levels are `(2n, 2n + 1)` and right-associative ones
/// `(2n + 1, 2n)`, which is the whole of associativity: the right-hand parse
/// is entered with the power the loop will next compare against, so an equal
/// operator is taken by whichever side has the lower number.
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

/// Whether `kind` is a prefix operator (11.4.1, 11.4.9), which binds tighter
/// than every binary one.
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

/// Parses one expression, or answers `None` without consuming anything.
pub fn expr<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    binary(parser, 0)
}

/// Parses what may be assigned to: a primary and its postfixes, and nothing
/// binary.
///
/// A.8.5's `variable_lvalue` is a name with its selects, a concatenation, or
/// an assignment pattern -- exactly [`unary`]'s reach and nothing beyond it.
/// Asking for an expression instead would read the `<=` of a nonblocking
/// assignment as the relational operator it also is, and hand the statement
/// rule a [`BIN_EXPR`] with the assignment nowhere in it.
pub(super) fn lvalue<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    unary(parser)
}

/// The climb: an operand, then every operator that binds at least `min`.
fn binary<T: Tokens>(parser: &mut Parser<T>, min: u8) -> Option<Completed> {
    let mut lhs = unary(parser)?;

    loop {
        let kind = parser.kind(0);
        // `a * )` is no multiplication anywhere, and reading it as one is how
        // an attribute's `*)` would otherwise be swallowed by the value
        // before it.
        if kind == STAR && parser.kind(1) == R_PAREN {
            break;
        }
        let Some((left, right)) = binding(kind) else {
            break;
        };
        if left < min {
            break;
        }

        // Taken before the node is reopened, so that an operator with
        // nothing to its right can be given back rather than left inside a
        // `BIN_EXPR` with one operand.
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
                // 11.3.2 allows an attribute between a binary operator and
                // its right operand, which is the one place in an expression
                // they appear.
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

/// `c ? a : b`, once the `?` has been recognised.
fn ternary<T: Tokens>(parser: &mut Parser<T>, marker: super::Marker, right: u8) -> Completed {
    parser.bump();
    attributes(parser);
    binary(parser, 0);
    if parser.at(COLON) {
        parser.bump();
        // The false arm is entered at the `?:` level's own power, which is
        // what makes `a ? b : c ? d : e` nest to the right.
        binary(parser, right);
    }
    parser.complete(marker, TERNARY_EXPR)
}

/// A prefix operator and its operand, or a postfixed primary.
fn unary<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    if is_unary(parser.kind(0)) {
        let before = parser.snapshot();
        let marker = parser.start();
        parser.bump();
        attributes(parser);
        // Tighter than `**`, which is the one place SystemVerilog differs
        // from most languages: `-2 ** 2` is `(-2) ** 2`.
        if unary(parser).is_none() {
            // `None` has to mean nothing was taken, or the caller's fallback
            // starts after the tokens it was meant to fall back on.
            parser.abandon(marker);
            parser.rollback(before);
            return None;
        }
        return Some(parser.complete(marker, UNARY_EXPR));
    }

    let mut lhs = primary(parser)?;
    loop {
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
            // `int'(x)`, `8'(x)`. The lexer keeps `'` apart from `'h` and
            // `'{`, so a bare one before `(` can only be a cast.
            APOSTROPHE if parser.kind(1) == L_PAREN => {
                let marker = parser.precede(lhs);
                parser.bump();
                paren(parser);
                parser.complete(marker, CAST_EXPR)
            }
            // `T'{...}` -- a pattern that names the type it builds. The
            // lexer gives `'{` as one token, so this is not the cast above
            // with a brace after it.
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
            // `C#(W)` -- a parameterised class, which is a name applied to
            // arguments like any other. The `#` is what says which.
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
    Some(lhs)
}

/// One operand, before anything postfixed to it.
fn primary<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let kind = parser.kind(0);

    // A macro reference is an atom that may stand for a value, a name, or a
    // whole subexpression, and raw mode cannot know which. Taking it as a
    // primary is what lets `x = `WIDTH - 1` parse at all.
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
        // A type name used as a value, which is what the left of a cast is:
        // `int'(x)`, `logic'(y)`.
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

/// A number, which may be written in as many as three tokens.
///
/// 5.7.1 lets whitespace and even a comment separate a size from its base and
/// a base from its digits, so `8'hFF`, `8 'h FF` and `'h FF` are all one
/// value. The digits of `'h FF` lex as an identifier, which is why they are
/// taken by position rather than by kind.
fn number<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();

    if parser.at(INT_LITERAL) {
        parser.bump();
    }
    if parser.at(BASED_LITERAL) {
        parser.bump();
    } else if parser.at(INT_BASE) {
        parser.bump();
        // The digits may come out as more than one token, and as almost any
        // kind. `'h FF` is an identifier; `'h 4a43_f880` is an integer *and*
        // an identifier; `'h 1e0` is a **real**, because those digits are also
        // how scientific notation is written. Nothing about them is a number
        // to the lexer -- what makes them one is the base in front and that
        // nothing separates them. The first group may be held off by
        // whitespace, as 5.7.1 allows; the rest may not.
        if parser.at(TICK_IDENT) {
            // A macro may supply the digits: `32'h`DM_ADDR`. It stays a call
            // inside the literal rather than becoming an opaque token,
            // because it is still a macro reference and the tree should say
            // so.
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

/// Whether `kind` is something the digits of a based literal can lex as.
fn is_digits(kind: SyntaxKind) -> bool {
    matches!(kind, INT_LITERAL | IDENT | REAL_LITERAL)
}

/// An identifier after a `.` or a `::`, which may be any of several kinds and
/// is taken whatever it is.
fn name<T: Tokens>(parser: &mut Parser<T>) {
    if matches!(
        parser.kind(0),
        IDENT | ESCAPED_IDENT | SYSTEM_IDENT | NEW_KW | THIS_KW | SUPER_KW
    ) || parser.kind(0).is_keyword()
    {
        parser.bump();
    }
}

/// `( a )`, or the mintypmax form `( a : b : c )` that a delay may use.
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

/// What a `{` opens: a concatenation, a replication, or a stream.
fn braced<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    parser.bump();

    // `{<<{a}}` and `{>>4{a}}`: the direction comes first, then an optional
    // slice size, then the concatenation being streamed.
    if matches!(parser.kind(0), LT_LT | GT_GT) {
        parser.bump();
        // The slice size may be a count or a type: `{<<8{x}}` and
        // `{<<byte{x}}` both say how wide a slice is.
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

    // `{n{a}}`: what looked like the first element was the count.
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

/// `'{a, b}`, `'{default: 0}`, `'{key: value}`, `'{n{a}}`.
fn assignment_pattern<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    pattern_body(parser);
    parser.complete(marker, ASSIGNMENT_PATTERN)
}

/// The `'{` through the `}`, for a pattern that names its type and one that
/// does not alike.
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
            // `'{n{a}}`: what was read as the first element was the count.
            // The same shape a concatenation takes, and the same node.
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

/// `[i]`, `[hi:lo]`, `[base+:width]`, `[base-:width]`.
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

/// A call's arguments, which may be named and may be skipped.
pub(super) fn arguments<T: Tokens>(parser: &mut Parser<T>) {
    let list = parser.start();
    parser.bump();

    loop {
        let argument = parser.start();
        // `.port(value)`, the named form -- and `.*`, which an instance
        // writes to connect every port whose name matches a signal.
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
            // A parameter value may be a type rather than a value, and
            // `$bits(logic [7:0])` passes one to a system function.
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

/// The braced list of an `inside` or a `dist`, whose elements may be values,
/// `[low:high]` ranges, or -- for `dist` -- weighted.
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

/// `with (expr)`, as an array method takes it.
fn with_clause<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();
    paren(parser);
    parser.complete(marker, WITH_CLAUSE);
}

/// `(* name *)` or `(* name = value *)`, where the grammar admits one.
///
/// Distinguished from a parenthesised expression by lookahead rather than by
/// the lexer, which has no `(*` token: `*` is not a prefix operator, so a `(`
/// followed by one can only open an attribute.
///
/// Public because attributes attach to items, ports and statements too, and
/// the rule is the same wherever they appear.
pub fn attributes<T: Tokens>(parser: &mut Parser<T>) {
    if !(parser.at(L_PAREN) && parser.kind(1) == STAR) {
        return;
    }

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
            // Nothing recognisable, and no diagnostics layer to say so.
            // Leaving the rest to the caller beats spinning here.
            break;
        }
    }

    parser.complete(marker, ATTRIBUTES);
}
