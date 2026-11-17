//! What `always_ff` holds: registers, and nothing that computes their next
//! value.
//!
//! Read in the design, so a flop a macro writes, `` `FF(q, d, '0) ``, is
//! judged as a written one, and a constant from parameters is known to be
//! one. What a macro writes is reported at its call: the call put it in
//! the `always_ff`.

use astli_sema::{
    BodyId, Elaboration, ExprId, ExprKind, Hir, Member, StmtId, StmtKind, SymbolKind, Value,
};
use astli_syntax::SyntaxKind::{ALWAYS_FF_KW, BANG, EQ, LT_EQ, TILDE};
use astli_text::Span;

use super::drivers::definition_body;
use crate::rule::DesignCx;

/// A register's next value computed in `always_comb` leaves `always_ff` a
/// list of flops, each loaded from a name, under a reset and an enable at
/// most, and puts all of the logic where `always_comb` holds it to the rules
/// of combinational logic. Which signal is a reset or an enable is not
/// known, so a condition of one signal passes whatever it is.
pub(crate) fn always_ff_registers_only(cx: &mut DesignCx) {
    let hir = cx.hir();
    for (scope, it) in hir.scopes() {
        for member in &it.members {
            let Member::Process(process) = member else {
                continue;
            };
            if process.kind != ALWAYS_FF_KW {
                continue;
            }
            let mut walk = Walk {
                hir,
                elaboration: cx.elaboration,
                body: definition_body(cx, scope),
                found: Vec::new(),
            };
            walk.stmt(process.body);
            for (at, message, pointing) in walk.found {
                cx.report(at, message).label = Some(pointing.to_string());
            }
        }
    }
}

const ENABLE: &str = "compute the enable in `always_comb`";
const REGISTER: &str = "compute it in `always_comb`, and register the result";

struct Walk<'a> {
    hir: &'a Hir,
    elaboration: &'a Elaboration,
    /// The body of the definition the process stands in, at its defaults,
    /// where a constant is evaluated.
    body: Option<BodyId>,
    found: Vec<(Span, &'static str, &'static str)>,
}

impl Walk<'_> {
    fn stmt(&mut self, id: StmtId) {
        let hir = self.hir;
        let stmt = &hir[id];
        match &stmt.kind {
            StmtKind::Expr(expr) => match hir[*expr].kind {
                ExprKind::Assign { op, rhs, .. } => {
                    if !matches!(op, EQ | LT_EQ) {
                        let at = hir[*expr].span;
                        self.found
                            .push((at, "compound assignment in `always_ff`", REGISTER));
                    }
                    self.value(rhs, REGISTER);
                }
                // A step, `cnt_q++`, or a task's call.
                _ => self.value(*expr, REGISTER),
            },
            StmtKind::Block { scope, stmts, .. } => {
                for member in &hir[*scope].members {
                    if let Member::Declare(symbol) = member
                        && let SymbolKind::Variable(data) = &hir[*symbol].kind
                        && let Some(init) = data.init
                    {
                        self.value(init, REGISTER);
                    }
                }
                for &stmt in stmts {
                    self.stmt(stmt);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.value(self.tested(*condition), ENABLE);
                for &branch in then_branch.iter().chain(else_branch) {
                    self.stmt(branch);
                }
            }
            StmtKind::Case { selector, .. } => {
                let at = selector.map_or(stmt.span, |it| hir[it].span);
                self.found.push((
                    at,
                    "`case` in `always_ff`",
                    "select in `always_comb`, and register the result",
                ));
            }
            // A loop's header is not judged: it repeats registers.
            StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::Timed { body, .. } => {
                if let Some(body) = *body {
                    self.stmt(body);
                }
            }
            // An assertion, a wait, a `disable` computes no register.
            _ => {}
        }
    }

    /// Reports the outermost logic in `id`: an operator, a conditional, a
    /// call, that is not a constant. What only connects bits passes, and so
    /// does what the rule does not know. A select's index is not judged:
    /// picking a register by address, or writing one, is what a register
    /// file does.
    fn value(&mut self, id: ExprId, pointing: &'static str) {
        let hir = self.hir;
        let expr = &hir[id];
        match &expr.kind {
            // A mux on one signal, which `if` writes as well.
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
                ..
            } if [self.tested(*condition), *then_value, *else_value]
                .into_iter()
                .all(|it| self.wiring(it)) => {}
            ExprKind::Unary { .. }
            | ExprKind::Postfix { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Conditional { .. }
            | ExprKind::Call { .. }
            | ExprKind::Inside { .. }
            | ExprKind::Stream { .. }
            | ExprKind::With { .. } => {
                if !self.constant(id) {
                    self.found
                        .push((expr.span, "logic in `always_ff`", pointing));
                }
            }
            ExprKind::Select { base, .. }
            | ExprKind::Member { base, .. }
            | ExprKind::Cast { operand: base, .. } => self.value(*base, pointing),
            ExprKind::Replication { concat, .. } => self.value(*concat, pointing),
            ExprKind::Concat(parts) => {
                for &part in parts {
                    self.value(part, pointing);
                }
            }
            ExprKind::Pattern { items, .. } => {
                for item in items {
                    self.value(item.value, pointing);
                }
            }
            _ => {}
        }
    }

    /// What `condition` tests: one `!` or `~` on it is the low level of a
    /// reset or an enable.
    fn tested(&self, condition: ExprId) -> ExprId {
        match self.hir[condition].kind {
            ExprKind::Unary {
                op: BANG | TILDE,
                operand,
            } => operand,
            _ => condition,
        }
    }

    /// Whether `id` only connects bits, holding no logic to report.
    fn wiring(&mut self, id: ExprId) -> bool {
        let found = self.found.len();
        self.value(id, REGISTER);
        let wiring = self.found.len() == found;
        self.found.truncate(found);
        wiring
    }

    /// Whether `id` evaluates to a constant, as `Width - 1` does.
    fn constant(&self, id: ExprId) -> bool {
        (self.body).is_some_and(|body| !matches!(self.elaboration.eval(body, id), Value::Unknown))
    }
}
