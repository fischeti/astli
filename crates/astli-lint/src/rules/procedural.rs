//! Procedural blocks: how they are opened, and the assignments inside.

use astli_syntax::SyntaxKind::{
    ALWAYS_COMB_KW, ALWAYS_FF_KW, ALWAYS_KW, AT, BANG, BEGIN_KW, COMMA, DELAY_CONTROL, EDGE_KW, EQ,
    EVENT_CONTROL, IDENT, L_PAREN, LT_EQ, MINUS, MINUS_MINUS, NEGEDGE_KW, OR_KW, PAREN_EXPR,
    PLUS_PLUS, POSEDGE_KW, PROCEDURAL_BLOCK, R_PAREN, STAR, SYSTEM_IDENT, TILDE, WAIT_ORDER_STMT,
    WAIT_STMT,
};
use astli_syntax::ast::{
    Arg, Assignment, AstNode, Block, CaseStmt, Declarator, DisableStmt, EventControl, Expr,
    ExprStmt, IfStmt, LabeledStmt, ParenExpr, ProceduralBlock, TypeOrExpr,
};
use astli_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, significant_tokens};
use rustc_hash::FxHashSet;

use super::expressions::one_bit_comparison_of;
use super::generate::label;
use crate::rule::Cx;

/// In sequential logic, a blocking assignment to a variable another process
/// reads races with that process: which value it sees depends on the order
/// the simulator runs them in. A variable declared inside the block is read
/// by no other process, so it may be assigned either way.
pub(crate) fn always_ff_non_blocking(cx: &mut Cx) {
    for block in blocks(cx, ALWAYS_FF_KW) {
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
    for block in blocks(cx, ALWAYS_COMB_KW) {
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

/// A register's next value computed in `always_comb` leaves `always_ff` a
/// list of flops, each loaded from a name, under a reset and an enable at
/// most, and puts all of the logic where `always_comb` holds it to the rules
/// of combinational logic. Which signal is a reset or an enable is not
/// known from the tree, so a condition of one signal passes whatever it is.
pub(crate) fn always_ff_registers_only(cx: &mut Cx) {
    for block in blocks(cx, ALWAYS_FF_KW) {
        for node in block.syntax().descendants() {
            if let Some(statement) = IfStmt::cast(node.clone()) {
                let Some(condition) = statement.condition() else {
                    continue;
                };
                if !paren(&condition, signal) {
                    let diagnostic = cx
                        .diagnostic(
                            Cx::range(condition.syntax()),
                            "condition in `always_ff` is more than one signal",
                        )
                        .pointing("compute the enable in `always_comb`");
                    cx.report(diagnostic);
                }
            } else if let Some(statement) = CaseStmt::cast(node.clone()) {
                let Some(keyword) = statement.keyword() else {
                    continue;
                };
                let diagnostic = cx
                    .diagnostic(
                        keyword.text_range(),
                        format!("`{}` in `always_ff`", keyword.text()),
                    )
                    .pointing("select in `always_comb`, and register the result");
                cx.report(diagnostic);
            } else if let Some(statement) = ExprStmt::cast(node.clone()) {
                let Some((op, _)) = writes_of(&statement) else {
                    continue;
                };
                if !matches!(op.kind(), EQ | LT_EQ) {
                    let diagnostic = cx
                        .diagnostic(op.text_range(), format!("`{}` in `always_ff`", op.text()))
                        .pointing("compute it in `always_comb`, and register the result");
                    cx.report(diagnostic);
                }
                let value = statement.assignment().and_then(|it| it.rhs());
                report_logic(cx, value);
            } else if let Some(declarator) = Declarator::cast(node) {
                // A loop's header declares its variable inside parentheses;
                // the header is not judged.
                let header = (declarator.syntax().parent())
                    .and_then(|it| it.parent())
                    .is_some_and(|it| it.kind() == PAREN_EXPR);
                if header {
                    continue;
                }
                let value = match declarator.init() {
                    Some(TypeOrExpr::Expr(value)) => Some(value),
                    _ => None,
                };
                report_logic(cx, value);
            }
        }
    }
}

/// Reports `value` if it is more than wiring.
fn report_logic(cx: &mut Cx, value: Option<Expr>) {
    let Some(value) = value.filter(|it| !wiring(it)) else {
        return;
    };
    let diagnostic = cx
        .diagnostic(Cx::range(value.syntax()), "logic in `always_ff`")
        .pointing("compute it in `always_comb`, and register the result");
    cx.report(diagnostic);
}

/// Whether `expr` only connects bits, computing nothing: a name, a member, a
/// select, a constant, or a concatenation, pattern or cast of such. A
/// select's index is not judged: picking a register by address, or writing
/// one, is what a register file does. A macro call may expand to anything,
/// and what the parser could not read is unknown; both pass.
fn wiring(expr: &Expr) -> bool {
    match expr {
        Expr::NameRef(_) | Expr::ScopeExpr(_) | Expr::LiteralExpr(_) => true,
        Expr::ParenExpr(inner) => paren(inner, wiring),
        Expr::FieldExpr(field) => field.expr().is_none_or(|it| wiring(&it)),
        Expr::IndexExpr(select) => select.base().is_none_or(|it| wiring(&it)),
        Expr::CastExpr(cast) => cast.operand().is_none_or(|it| paren(&it, wiring)),
        Expr::ConcatExpr(concat) => all_wiring(concat.exprs()),
        Expr::ReplicationExpr(replication) => {
            (replication.concat()).is_none_or(|concat| all_wiring(concat.exprs()))
        }
        Expr::AssignmentPattern(pattern) => {
            all_wiring(pattern.pattern_items().filter_map(|item| item.value()))
        }
        // A negative constant, as a reset value.
        Expr::UnaryExpr(unary) => {
            unary.op().is_some_and(|op| op.kind() == MINUS)
                && matches!(unary.expr(), Some(Expr::LiteralExpr(_)))
        }
        Expr::CallExpr(call) => match call.expr() {
            Some(Expr::NameRef(name)) if name.preproc().is_some() => true,
            Some(Expr::NameRef(name)) => {
                let cast = name.name().is_some_and(|it| {
                    it.kind() == SYSTEM_IDENT && matches!(it.text(), "$signed" | "$unsigned")
                });
                cast && call
                    .arg_list()
                    .is_none_or(|list| list.args().all(|it| arg(&it)))
            }
            _ => false,
        },
        _ => false,
    }
}

fn all_wiring(mut exprs: impl Iterator<Item = Expr>) -> bool {
    exprs.all(|it| wiring(&it))
}

/// Whether a call's argument is wiring; a type is.
fn arg(arg: &Arg) -> bool {
    match arg.type_or_expr() {
        Some(TypeOrExpr::Expr(value)) => wiring(&value),
        Some(TypeOrExpr::DataType(_)) => true,
        None => arg.exprs().all(|it| wiring(&it)),
    }
}

/// Whether `expr` is one signal, or its negation: a name, a member or a
/// select, under at most one `!` or `~`, or compared with `1'b1` or `1'b0`,
/// which `one-bit-comparison` reports.
fn signal(expr: &Expr) -> bool {
    fn plain(expr: &Expr) -> bool {
        match expr {
            Expr::NameRef(_) | Expr::ScopeExpr(_) | Expr::LiteralExpr(_) => true,
            Expr::ParenExpr(inner) => paren(inner, plain),
            Expr::FieldExpr(field) => field.expr().is_none_or(|it| plain(&it)),
            Expr::IndexExpr(select) => select.base().is_none_or(|it| plain(&it)),
            _ => false,
        }
    }
    match expr {
        Expr::UnaryExpr(unary) => {
            unary
                .op()
                .is_some_and(|op| matches!(op.kind(), BANG | TILDE))
                && unary.expr().is_none_or(|it| plain(&it))
        }
        Expr::ParenExpr(inner) => paren(inner, signal),
        Expr::BinExpr(comparison) => {
            one_bit_comparison_of(comparison).is_some_and(|(other, ..)| plain(&other))
        }
        _ => plain(expr),
    }
}

/// Whether what `parens` hold is one expression `holds` accepts; empty
/// parentheses are a parse error, and pass.
fn paren(parens: &ParenExpr, holds: fn(&Expr) -> bool) -> bool {
    if parens.assignments().next().is_some() || parens.var_decls().next().is_some() {
        return false;
    }
    let mut exprs = parens.exprs();
    match (exprs.next(), exprs.next()) {
        (Some(only), None) => holds(&only),
        (None, _) => true,
        _ => false,
    }
}

/// `always @*` is combinational logic by intent, which `always_comb` says
/// outright: it also runs once at time zero, and a tool can check that the
/// block infers no latch and that nothing else drives what it assigns.
pub(crate) fn always_comb(cx: &mut Cx) {
    for block in blocks(cx, ALWAYS_KW) {
        let control = (block.body())
            .and_then(|body| body.syntax().first_child())
            .and_then(EventControl::cast);
        let starred = control.is_some_and(|control| {
            let kinds: Vec<_> = significant_tokens(control.syntax())
                .map(|token| token.kind())
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

/// An `always` that waits only for edges is sequential logic by intent,
/// which `always_ff` says outright: a tool can then check that nothing else
/// writes what it assigns. A block that waits again inside, as a
/// testbench's may, cannot be `always_ff`, and is left alone.
pub(crate) fn always_ff(cx: &mut Cx) {
    for block in blocks(cx, ALWAYS_KW) {
        let Some(body) = block.body() else {
            continue;
        };
        let control = body.syntax().first_child().and_then(EventControl::cast);
        let sequential = control.is_some_and(|control| {
            edges(&control) && !waits_again(body.syntax(), control.syntax())
        });
        let Some(keyword) = block.keyword().filter(|_| sequential) else {
            continue;
        };
        let diagnostic = cx
            .diagnostic(
                keyword.text_range(),
                "edge-triggered `always` instead of `always_ff`",
            )
            .pointing("write `always_ff`");
        cx.report(diagnostic);
    }
}

/// Whether `control` waits for edges alone: each event it lists, between
/// `or` and `,`, is a `posedge`, `negedge` or `edge`.
pub(super) fn edges(control: &EventControl) -> bool {
    let Some(list) = (control.syntax().first_child()).filter(|it| it.kind() == PAREN_EXPR) else {
        return false;
    };
    // The parentheses around a nested expression are inside its node, so
    // the only ones among the list's own children are its own.
    let elements: Vec<_> = (list.children_with_tokens())
        .filter(|it| !it.kind().is_trivia() && !matches!(it.kind(), L_PAREN | R_PAREN))
        .collect();
    !elements.is_empty()
        && (elements.split(|it| matches!(it.kind(), OR_KW | COMMA))).all(|event| {
            (event.first()).is_some_and(|it| matches!(it.kind(), POSEDGE_KW | NEGEDGE_KW | EDGE_KW))
        })
}

/// Whether `body` holds a timing control that blocks, besides the `control`
/// it starts with: a `wait`, or a delay or event control before a statement
/// or in a blocking assignment. One in a non-blocking assignment only delays
/// the update, which `always_ff` allows.
pub(super) fn waits_again(body: &SyntaxNode, control: &SyntaxNode) -> bool {
    body.descendants().any(|node| match node.kind() {
        WAIT_STMT | WAIT_ORDER_STMT => true,
        DELAY_CONTROL | EVENT_CONTROL if node != *control => !(node.ancestors())
            .find_map(Assignment::cast)
            .and_then(|assignment| assignment.op())
            .is_some_and(|op| op.kind() == LT_EQ),
        _ => false,
    })
}

/// The procedural blocks in the file that `keyword` opens.
fn blocks<'a>(cx: &Cx<'a>, keyword: SyntaxKind) -> impl Iterator<Item = ProceduralBlock> + use<'a> {
    cx.nodes::<ProceduralBlock>()
        .filter(move |block| block.keyword().is_some_and(|it| it.kind() == keyword))
}

/// Each statement under `body` that writes a variable: its operator, and the
/// expression written. An increment's operator is its `++` or `--`.
///
/// Only statements count, so a `for` header's `i = 0` and `i++` do not.
fn writes(body: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, Option<Expr>)> {
    body.descendants()
        .filter_map(ExprStmt::cast)
        .filter_map(|statement| writes_of(&statement))
}

/// The write `statement` makes, if any: its operator, and the expression
/// written.
fn writes_of(statement: &ExprStmt) -> Option<(SyntaxToken, Option<Expr>)> {
    match (statement.assignment(), statement.expr()) {
        (Some(assignment), _) => Some((assignment.op()?, assignment.lhs())),
        (None, Some(Expr::PostfixExpr(step))) => step_of(step.op()?, step.expr()),
        (None, Some(Expr::UnaryExpr(step))) => step_of(step.op()?, step.expr()),
        _ => None,
    }
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

/// `disable name` stops the block it names, which reads plainly only when
/// that is a `begin` block around the statement: to stop what a `fork`
/// started, `disable fork` says so, and a block elsewhere or a task is
/// stopped from afar. A block that is a process's whole body is not named
/// either: a `begin` inside it is.
pub(crate) fn disable_statement(cx: &mut Cx) {
    for stmt in cx.nodes::<DisableStmt>() {
        let name = match stmt.expr() {
            Some(Expr::NameRef(name)) if stmt.fork_token().is_none() => name.name(),
            _ => None,
        };
        let Some(name) = name.filter(|it| it.kind() == IDENT) else {
            continue;
        };
        let around = stmt.syntax().ancestors().filter_map(Block::cast);
        let mut whole_process = false;
        let mut found = false;
        for block in around.filter(|block| block.open().is_some_and(|it| it.kind() == BEGIN_KW)) {
            let Some(label) = label(&block) else {
                continue;
            };
            let mut parent = block.syntax().parent();
            if let Some(labelled) = parent.clone().and_then(LabeledStmt::cast) {
                parent = labelled.syntax().parent();
            }
            if parent.is_some_and(|it| it.kind() == PROCEDURAL_BLOCK) {
                whole_process = true;
                break;
            }
            if label.text() == name.text() {
                found = true;
                break;
            }
        }
        if found {
            continue;
        }
        let range = Cx::range(stmt.syntax());
        let diagnostic = match whole_process {
            true => cx
                .diagnostic(
                    range,
                    format!("`disable {}` names a process's whole body", name.text()),
                )
                .note("label a `begin` block inside the process, and disable that"),
            false => cx
                .diagnostic(
                    range,
                    format!("`disable {}` names no `begin` block around it", name.text()),
                )
                .pointing("to stop what a `fork` started, use `disable fork`"),
        };
        cx.report(diagnostic);
    }
}
