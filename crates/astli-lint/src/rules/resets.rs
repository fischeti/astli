//! Asynchronous resets: an `always_ff` that waits for the edges of more
//! than one signal, its clock and its reset.
//!
//! Which of them is the reset is known from the process alone, as synthesis
//! infers it: the signal its `if` tests. No name says so. Read in the
//! design, so a flop a macro writes, `` `FF(q, d, '0) ``, is judged as a
//! written one, and reported at the call.

use astli_sema::{
    Event, ExprId, ExprKind, Hir, Literal, Member, Process, StmtId, StmtKind, Timing, literal,
};
use astli_syntax::SyntaxKind::{ALWAYS_FF_KW, BANG, BANG_EQ, EQ_EQ, NEGEDGE_KW, POSEDGE_KW, TILDE};

use crate::rule::DesignCx;

/// A flop that waits for its reset's edge takes the reset as the whole of
/// its process: an `if` on the reset alone, testing the level the edge
/// leads to, whose `else` is the flop's clocked logic. A flop beside that
/// `if` waits for the reset without taking it, and simulates as a latch on
/// the reset where synthesis builds a flop; an `if` on the wrong level
/// resets on the reset's release.
pub(crate) fn async_reset_style(cx: &mut DesignCx) {
    let hir = cx.hir();
    for (process, events, body) in asynchronous(hir) {
        let mut ifs = Vec::new();
        reset_ifs(hir, body, true, events, &mut ifs);
        if ifs.is_empty() {
            cx.report(
                process.span,
                "process waits for a reset's edge, but tests no reset",
            )
            .label = Some("make an `if` on the reset the whole of its body".to_string());
        }
        for test in ifs {
            let at = hir[test.signal].span;
            let name = cx.origins.slice(at).to_string();
            if let Some(falling) = test.falling.filter(|it| *it != test.low) {
                let (level, edge) = match falling {
                    true => ("high", "falling"),
                    false => ("low", "rising"),
                };
                let message =
                    format!("`{name}` tested {level}, but the process resets on its {edge} edge");
                cx.report(at, message).label = Some("test the level the edge leads to".to_string());
            }
            if !test.alone {
                let diagnostic =
                    cx.report(at, format!("process resets on `{name}`, but not all of it"));
                diagnostic.label =
                    Some("give what this `if` leaves out a process of its own".to_string());
                (diagnostic.notes).push(
                    "a flop that waits for a reset it does not take holds its value badly"
                        .to_string(),
                );
            }
        }
    }
}

/// A project whose flops reset synchronously waits for the clock alone.
pub(crate) fn forbid_async_reset(cx: &mut DesignCx) {
    let hir = cx.hir();
    for (process, ..) in asynchronous(hir) {
        cx.report(process.span, "asynchronous reset").label =
            Some("reset synchronously: wait for the clock alone".to_string());
    }
}

/// Each `always_ff` in `hir` that waits for the edges of more than one
/// signal, its events and the statement it runs.
fn asynchronous(hir: &Hir) -> impl Iterator<Item = (&Process, &[Event], StmtId)> {
    (hir.scopes())
        .flat_map(|(_, scope)| &scope.members)
        .filter_map(move |member| {
            let Member::Process(process) = member else {
                return None;
            };
            let StmtKind::Timed {
                timing: Timing::Events(events),
                body: Some(body),
            } = &hir[process.body].kind
            else {
                return None;
            };
            let other = |event: &Event| !same(hir, event.expr, events[0].expr);
            let edges = events.iter().all(|it| it.edge.is_some());
            (process.kind == ALWAYS_FF_KW && edges && events.iter().any(other)).then_some((
                process,
                &events[..],
                *body,
            ))
        })
}

/// An `if` that tests one of the process's events alone, where a flop
/// takes its reset.
struct ResetIf {
    signal: ExprId,
    /// Whether the process waits for the signal's falling edge; `None` for
    /// `edge`.
    falling: Option<bool>,
    /// Whether it tests the signal low.
    low: bool,
    /// Whether it is all of the process, with nothing beside it.
    alone: bool,
}

/// Each `if` under `stmt` that tests one of `events` alone and stands where
/// a flop takes its reset: reached through `begin` blocks and loops over an
/// array, never inside another branch.
fn reset_ifs(hir: &Hir, stmt: StmtId, alone: bool, events: &[Event], ifs: &mut Vec<ResetIf>) {
    match &hir[stmt].kind {
        StmtKind::If { condition, .. } => {
            let Some((signal, low)) = tested(hir, *condition) else {
                return;
            };
            if let Some(event) = events.iter().find(|it| same(hir, it.expr, signal)) {
                ifs.push(ResetIf {
                    signal,
                    falling: match event.edge {
                        Some(NEGEDGE_KW) => Some(true),
                        Some(POSEDGE_KW) => Some(false),
                        _ => None,
                    },
                    low,
                    alone,
                });
            }
        }
        StmtKind::Block { stmts, .. } => {
            for &inner in stmts {
                reset_ifs(hir, inner, alone && stmts.len() == 1, events, ifs);
            }
        }
        StmtKind::For {
            body: Some(body), ..
        }
        | StmtKind::Foreach {
            body: Some(body), ..
        } => reset_ifs(hir, *body, alone, events, ifs),
        _ => {}
    }
}

/// The signal `condition` tests, and whether it tests it low: the signal
/// alone, under `!` or `~`, or compared with 0 or 1.
fn tested(hir: &Hir, condition: ExprId) -> Option<(ExprId, bool)> {
    match hir[condition].kind {
        ExprKind::Unary {
            op: BANG | TILDE,
            operand,
        } => tested(hir, operand).map(|(signal, low)| (signal, !low)),
        ExprKind::Binary {
            op: op @ (EQ_EQ | BANG_EQ),
            lhs,
            rhs,
        } => {
            let (signal, value) = match (bit(hir, lhs), bit(hir, rhs)) {
                (None, Some(value)) => (lhs, value),
                (Some(value), None) => (rhs, value),
                _ => return None,
            };
            let (signal, low) = tested(hir, signal)?;
            Some((signal, low == (value == (op == EQ_EQ))))
        }
        ExprKind::Name(_) | ExprKind::Member { .. } | ExprKind::Select { .. } => {
            Some((condition, false))
        }
        _ => None,
    }
}

/// The value of `expr`, if it is a literal of 0 or 1.
fn bit(hir: &Hir, expr: ExprId) -> Option<bool> {
    let ExprKind::Literal(text) = &hir[expr].kind else {
        return None;
    };
    match literal(text) {
        Literal::Bits(bits) if !bits.has_unknown() => match bits.to_u64()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `a` and `b` name the same signal: a name, a member of one, or a
/// select of one by the same index.
fn same(hir: &Hir, a: ExprId, b: ExprId) -> bool {
    match (&hir[a].kind, &hir[b].kind) {
        (ExprKind::Name(a), ExprKind::Name(b)) | (ExprKind::Literal(a), ExprKind::Literal(b)) => {
            a == b
        }
        (ExprKind::Member { base: a, name: x }, ExprKind::Member { base: b, name: y }) => {
            x == y && same(hir, *a, *b)
        }
        (
            ExprKind::Select {
                base: a,
                index: i,
                range: None,
            },
            ExprKind::Select {
                base: b,
                index: j,
                range: None,
            },
        ) => same(hir, *a, *b) && same(hir, *i, *j),
        _ => false,
    }
}
