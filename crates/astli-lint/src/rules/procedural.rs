//! Procedural blocks: how they are opened, and the assignments inside.

use astli_syntax::SyntaxKind::{
    ALWAYS_COMB_KW, ALWAYS_FF_KW, ALWAYS_KW, AT, L_PAREN, LT_EQ, MINUS_MINUS, PLUS_PLUS, R_PAREN,
    STAR,
};
use astli_syntax::ast::{AstNode, Declarator, EventControl, Expr, ExprStmt, ProceduralBlock};
use astli_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use rustc_hash::FxHashSet;

use crate::rule::Cx;

/// In sequential logic, a blocking assignment to a variable another process
/// reads races with that process: which value it sees depends on the order
/// the simulator runs them in. A variable declared inside the block is read
/// by no other process, so it may be assigned either way.
pub(crate) fn always_ff_non_blocking(cx: &mut Cx) {
    for block in blocks(cx.root(), ALWAYS_FF_KW) {
        let locals = declared(block.syntax());
        for (op, target) in writes(block.syntax()) {
            if op.kind() == LT_EQ || target.is_some_and(|target| local(&target, &locals)) {
                continue;
            }
            let diagnostic = cx
                .diagnostic(op.text_range(), "blocking assignment in `always_ff`")
                .pointing("use `<=`")
                .note("another process reading this variable sees the old or the new value, depending on the order the two run in");
            cx.report(diagnostic);
        }
    }
}

/// In combinational logic, a non-blocking assignment takes effect only after
/// the block, so a read later in it sees the old value, and the block runs
/// again when the new one lands.
pub(crate) fn always_comb_blocking(cx: &mut Cx) {
    for block in blocks(cx.root(), ALWAYS_COMB_KW) {
        for (op, _) in writes(block.syntax()) {
            if op.kind() != LT_EQ {
                continue;
            }
            let diagnostic = cx
                .diagnostic(op.text_range(), "non-blocking assignment in `always_comb`")
                .pointing("use `=`")
                .note("a read later in the block sees the old value, not this one");
            cx.report(diagnostic);
        }
    }
}

/// `always @*` is combinational logic by intent, which `always_comb` says
/// outright: it also runs once at time zero, and a tool can check that the
/// block infers no latch and that nothing else drives what it assigns.
pub(crate) fn always_comb(cx: &mut Cx) {
    for block in blocks(cx.root(), ALWAYS_KW) {
        let control = (block.body())
            .and_then(|body| body.syntax().first_child())
            .and_then(EventControl::cast);
        let starred = control.is_some_and(|control| {
            let kinds: Vec<_> = (control.syntax().descendants_with_tokens())
                .filter_map(|element| element.into_token())
                .map(|token| token.kind())
                .filter(|kind| !kind.is_trivia())
                .collect();
            kinds == [AT, STAR] || kinds == [AT, L_PAREN, STAR, R_PAREN]
        });
        let Some(keyword) = block.keyword().filter(|_| starred) else {
            continue;
        };
        let diagnostic = cx
            .diagnostic(keyword.text_range(), "`always @*` instead of `always_comb`")
            .pointing("write `always_comb`, without the `@*`");
        cx.report(diagnostic);
    }
}

/// The procedural blocks under `root` that `keyword` opens.
fn blocks(root: &SyntaxNode, keyword: SyntaxKind) -> impl Iterator<Item = ProceduralBlock> {
    root.descendants()
        .filter_map(ProceduralBlock::cast)
        .filter(move |block| block.keyword().is_some_and(|it| it.kind() == keyword))
}

/// Each statement under `body` that writes a variable: its operator, and the
/// expression written. An increment's operator is its `++` or `--`.
///
/// Only statements count, so a `for` header's `i = 0` and `i++` do not.
fn writes(body: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, Option<Expr>)> {
    body.descendants()
        .filter_map(ExprStmt::cast)
        .filter_map(
            |statement| match (statement.assignment(), statement.expr()) {
                (Some(assignment), _) => Some((assignment.op()?, assignment.lhs())),
                (None, Some(Expr::PostfixExpr(step))) => step_of(step.op()?, step.expr()),
                (None, Some(Expr::UnaryExpr(step))) => step_of(step.op()?, step.expr()),
                _ => None,
            },
        )
}

fn step_of(op: SyntaxToken, operand: Option<Expr>) -> Option<(SyntaxToken, Option<Expr>)> {
    matches!(op.kind(), PLUS_PLUS | MINUS_MINUS).then_some((op, operand))
}

/// The names of the variables declared under `body`, loop variables among
/// them.
///
/// Names are not scoped: one declared in one `begin`-`end` counts across the
/// block. Shadowing a module's signal with a local of the same name, in a
/// sibling block, is rare enough not to pay for scopes here.
fn declared(body: &SyntaxNode) -> FxHashSet<String> {
    body.descendants()
        .filter_map(Declarator::cast)
        .filter_map(|declarator| declarator.name())
        .map(|name| name.text().to_string())
        .collect()
}

/// Whether `target` writes only variables in `locals`: a name, a part of
/// one, or a concatenation of such.
fn local(target: &Expr, locals: &FxHashSet<String>) -> bool {
    match target {
        Expr::NameRef(name) => name.name().is_some_and(|it| locals.contains(it.text())),
        Expr::IndexExpr(select) => select.base().is_some_and(|it| local(&it, locals)),
        Expr::FieldExpr(field) => field.expr().is_some_and(|it| local(&it, locals)),
        Expr::ConcatExpr(concat) => concat.exprs().all(|it| local(&it, locals)),
        _ => false,
    }
}
