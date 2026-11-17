//! Expressions, wherever they stand.

use astli_syntax::SyntaxKind::{BANG_EQ, BASED_LITERAL, CONSTRAINT_BLOCK, EQ_EQ, INT_LITERAL};
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
        let constrained = (comparison.syntax().ancestors()).any(|it| it.kind() == CONSTRAINT_BLOCK);
        if constrained {
            continue;
        }
        let Some((_, literal, truth)) = one_bit_comparison_of(&comparison) else {
            continue;
        };
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
/// `==` or `!=`: that expression, the literal as written, and whether the
/// comparison holds when the expression is 1.
pub(crate) fn one_bit_comparison_of(comparison: &BinExpr) -> Option<(Expr, String, bool)> {
    let op = comparison.op()?.kind();
    if !matches!(op, EQ_EQ | BANG_EQ) {
        return None;
    }
    let (lhs, rhs) = (comparison.lhs()?, comparison.rhs()?);
    let (other, literal, value) = match (one_bit(&lhs), one_bit(&rhs)) {
        (None, Some(value)) => (lhs, rhs, value),
        (Some(value), None) => (rhs, lhs, value),
        _ => return None,
    };
    let text = significant_tokens(literal.syntax())
        .map(|token| token.text().to_string())
        .collect();
    Some((other, text, value == (op == EQ_EQ)))
}

/// The value of `expr`, if it is a one-bit literal of 0 or 1, as `1'b1` or
/// `1'h0`; an `x` or a `z` is not.
fn one_bit(expr: &Expr) -> Option<bool> {
    let Expr::LiteralExpr(literal) = expr else {
        return None;
    };
    let mut tokens = significant_tokens(literal.syntax());
    let (size, based) = (tokens.next()?, tokens.next()?);
    if tokens.next().is_some()
        || size.kind() != INT_LITERAL
        || size.text() != "1"
        || based.kind() != BASED_LITERAL
    {
        return None;
    }
    // `'`, an optional `s`, the base, then the digits.
    let digits = based
        .text()
        .get(1..)?
        .trim_start_matches(['s', 'S'])
        .get(1..)?;
    let digits: String = digits.chars().filter(|it| *it != '_').collect();
    match digits.trim_start_matches('0') {
        "" if !digits.is_empty() => Some(false),
        "1" => Some(true),
        _ => None,
    }
}
