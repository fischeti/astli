//! Expressions, wherever they stand.

use astli_sema::{Literal, literal};
use astli_syntax::SyntaxKind::{self, *};
use astli_syntax::ast::{AstNode, BinExpr, Expr};
use astli_syntax::significant_tokens;
use astli_syntax::{SyntaxNode, SyntaxToken};
use rowan::TextRange;

use crate::rule::Cx;

/// `a == 1'b1` is `a`, and `a == 1'b0` is `!a`, when `a` is one bit; when
/// it is wider, the literal is extended to its width, and comparing with a
/// literal of that width says so. `===` and `!==` are left alone: with an
/// `x` on `a` they answer where `a` alone would not. So is a constraint's,
/// as `a == 1'b0;`, which states the value the solver is to pick.
pub(crate) fn one_bit_comparison(cx: &mut Cx) {
    for comparison in cx.nodes::<BinExpr>() {
        let Some((literal, truth)) = one_bit_comparison_of(&comparison) else {
            continue;
        };
        if (comparison.syntax().ancestors()).any(|it| it.kind() == CONSTRAINT_BLOCK) {
            continue;
        }
        let pointing = match truth {
            true => "for one bit, write the signal alone",
            false => "for one bit, write `!` and the signal",
        };
        let diagnostic = cx
            .diagnostic(
                Cx::range(comparison.syntax()),
                format!("comparison with `{literal}`"),
            )
            .pointing(pointing)
            .note("on a wider signal, compare with a literal of its width");
        cx.report(diagnostic);
    }
}

/// `comparison`, if it compares an expression with a one-bit literal by
/// `==` or `!=`: the literal as written, and whether the comparison holds
/// when the expression is 1.
fn one_bit_comparison_of(comparison: &BinExpr) -> Option<(String, bool)> {
    let op = comparison.op()?.kind();
    if !matches!(op, EQ_EQ | BANG_EQ) {
        return None;
    }
    let (lhs, rhs) = (comparison.lhs()?, comparison.rhs()?);
    let (text, value) = match (one_bit(&lhs), one_bit(&rhs)) {
        (Some(it), None) | (None, Some(it)) => it,
        _ => return None,
    };
    Some((text, value == (op == EQ_EQ)))
}

/// `expr` as written and its value, if it is a one-bit literal of 0 or 1,
/// as `1'b1` or `1'h0`; an `x` or a `z` is not.
fn one_bit(expr: &Expr) -> Option<(String, bool)> {
    let Expr::LiteralExpr(it) = expr else {
        return None;
    };
    let text: String = (significant_tokens(it.syntax()))
        .map(|token| token.text().to_string())
        .collect();
    match literal(&text) {
        Literal::Bits(bits) if bits.width() == 1 && !bits.has_unknown() => {
            Some((text, bits.to_u64()? == 1))
        }
        _ => None,
    }
}

/// `a || b && c` is `a || (b && c)`, `a | b & c` is `a | (b & c)`,
/// `a & b == c` is `a & (b == c)` and `a << b + c` is `a << (b + c)`, but
/// only a reader who knows the table sees it at a glance. Where `&` is a
/// one-bit and, as RTL writes it, that grouping is the one meant, so the fix
/// writes the parentheses the parser already put there. Arithmetic among
/// itself, and a comparison inside `&&` or `||`, are read the same way by
/// almost everyone; `comparison-parentheses` asks for that one.
pub(crate) fn mixed_operator_parentheses(cx: &mut Cx) {
    parenthesize(cx, mixed);
}

/// `a == b && c < d` is `(a == b) && (c < d)`, as most read it; a project
/// that wants every condition's grouping written out asks for this.
pub(crate) fn comparison_parentheses(cx: &mut Cx) {
    parenthesize(cx, |outer, inner| {
        matches!(outer, AMP_AMP | PIPE_PIPE) && comparison(inner)
    });
}

/// Reports each operand of a binary operator that is itself one `mixed`
/// pairs with it, written without parentheses, with a fix that writes the
/// parentheses the parser already put there.
fn parenthesize(cx: &mut Cx, mixed: impl Fn(SyntaxKind, SyntaxKind) -> bool) {
    for outer in cx.nodes::<BinExpr>() {
        let Some(outer_op) = outer.op() else {
            continue;
        };
        // Only a direct operand: inside a call's or an index's brackets, the
        // grouping is plain.
        for operand in [outer.lhs(), outer.rhs()] {
            let Some((inner, inner_op)) = operand.as_ref().and_then(operator) else {
                continue;
            };
            if !mixed(outer_op.kind(), inner_op.kind()) {
                continue;
            }
            let range = Cx::range(inner);
            let (outer_text, inner_text) = (outer_op.text(), inner_op.text());
            let diagnostic = cx
                .diagnostic(
                    range,
                    format!("`{inner_text}` inside `{outer_text}` without parentheses"),
                )
                .pointing(format!("`{inner_text}` binds tighter than `{outer_text}`"))
                .fix(cx.span(TextRange::empty(range.start())), "(")
                .fix(cx.span(TextRange::empty(range.end())), ")");
            cx.report(diagnostic);
        }
    }
}

/// An operand's node and its operator. `inside` has a node of its own, but
/// it binds as a relational operator does, and is read with no more
/// certainty.
fn operator(operand: &Expr) -> Option<(&SyntaxNode, SyntaxToken)> {
    match operand {
        Expr::BinExpr(it) => Some((it.syntax(), it.op()?)),
        Expr::InsideExpr(it) => Some((it.syntax(), it.inside_token()?)),
        _ => None,
    }
}

/// Whether `inner`, an operand of `outer` written without parentheses, is
/// easily read as binding looser than it does.
fn mixed(outer: SyntaxKind, inner: SyntaxKind) -> bool {
    let xor = |kind| matches!(kind, CARET | TILDE_CARET | CARET_TILDE);
    let comparison = comparison(inner);
    match outer {
        PIPE_PIPE => inner == AMP_AMP,
        PIPE => inner == AMP || xor(inner) || comparison,
        _ if xor(outer) => inner == AMP || comparison,
        AMP => comparison,
        LT_LT | GT_GT | LT_LT_LT | GT_GT_GT => matches!(inner, PLUS | MINUS),
        _ => false,
    }
}

fn comparison(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        EQ_EQ
            | BANG_EQ
            | EQ_EQ_EQ
            | BANG_EQ_EQ
            | EQ_EQ_QUESTION
            | BANG_EQ_QUESTION
            | LT
            | LT_EQ
            | GT
            | GT_EQ
            | INSIDE_KW
    )
}
