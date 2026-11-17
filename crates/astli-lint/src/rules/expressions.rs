//! Expressions, wherever they stand.

use astli_sema::{Literal, literal};
use astli_syntax::SyntaxKind::{BANG_EQ, CONSTRAINT_BLOCK, EQ_EQ};
use astli_syntax::ast::{AstNode, BinExpr, Expr};
use astli_syntax::significant_tokens;

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
