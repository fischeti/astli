//! What `always_ff` holds: registers, and nothing that computes their next
//! value.
//!
//! Read in the design, so a flop a macro writes, `` `FF(q, d, '0) ``, is
//! judged as a written one, and a constant from parameters is known to be
//! one. What a macro writes is reported at its call: the call put it in
//! the `always_ff`.

use astli_sema::{
    BodyId, Elaboration, Event, ExprId, ExprKind, Hir, Member, StmtId, StmtKind, SymbolKind,
    Timing, Value,
};
use astli_syntax::SyntaxKind::{ALWAYS_FF_KW, BANG, EQ, LT_EQ, TILDE};
use astli_text::Span;

use super::drivers::definition_body;
use super::resets::{self, same};
use crate::rule::DesignCx;

/// A register's next value computed in `always_comb` leaves `always_ff` a
/// list of flops, each loaded from a name, as a library's flop takes it: a
/// reset, a synchronous clear, and an
/// enable, in that order, at most one of each. It puts all of the logic
/// where `always_comb` holds it to the rules of combinational logic.
///
/// An asynchronous reset is a signal the process waits for. Any other reset
/// or enable is told by what it loads: a condition one of whose branches
/// loads constants, and holds no condition, is a reset, or a clear, and
/// one that loads data on one side and nothing on the other is an enable.
/// Past the enable, any condition is a second signal it is combined with;
/// data on both sides of a condition is a mux, as is a clear on the side of
/// an enable that holds; and a register two statements load is a mux in
/// which the last one wins. A condition that is a constant chooses no
/// value, as a `generate` `if` does not.
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
            let resets = match &hir[process.body].kind {
                StmtKind::Timed {
                    timing: Timing::Events(events),
                    ..
                } if events.len() > 1 => &events[..],
                _ => &[],
            };
            let mut walk = Walk {
                hir,
                elaboration: cx.elaboration,
                body: definition_body(cx, scope),
                resets,
                found: Vec::new(),
            };
            walk.stmt(process.body, Path::default());
            for (at, message, pointing) in walk.found {
                cx.report(cx.diagnostic(at, message).pointing(pointing));
            }
        }
    }
}

const ENABLE: &str = "compute the enable in `always_comb`";
const REGISTER: &str = "compute it in `always_comb`, and register the result";
const SELECT: &str = "select in `always_comb`, and register the result";

/// What a statement, or a value, loads into the registers it assigns.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Load {
    /// Nothing, or a register's own value back.
    Nothing,
    /// Constants alone, on no condition: what a reset loads.
    Constant,
    /// Anything else, constants on a condition among it.
    Data,
}

/// The conditions a statement stands under, as a flop takes them.
#[derive(Clone, Copy, Default)]
struct Path {
    /// How many resets and clears.
    resets: u8,
    /// Whether an enable.
    enabled: bool,
}

struct Walk<'a> {
    hir: &'a Hir,
    elaboration: &'a Elaboration,
    /// The body of the definition the process stands in, at its defaults,
    /// where a constant is evaluated.
    body: Option<BodyId>,
    /// What the process waits for, if it waits for more than its clock:
    /// the signals it may reset on asynchronously.
    resets: &'a [Event],
    found: Vec<(Span, &'static str, &'static str)>,
}

/// One step on the way to a register an assignment loads: its name, a
/// member, or a select by the index it evaluates to, if it is a constant.
#[derive(PartialEq)]
enum Step<'a> {
    Name(&'a str),
    Index(Option<u64>),
}

impl<'a> Walk<'a> {
    /// Judges `id`, standing under `path`.
    fn stmt(&mut self, id: StmtId, path: Path) {
        let hir = self.hir;
        let stmt = &hir[id];
        match &stmt.kind {
            StmtKind::Expr(expr) => match hir[*expr].kind {
                ExprKind::Assign { op, lhs, rhs, .. } => {
                    if !matches!(op, EQ | LT_EQ) {
                        let at = hir[*expr].span;
                        self.found
                            .push((at, "compound assignment in `always_ff`", REGISTER));
                    }
                    self.assigned(lhs, rhs, path);
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
                let mut loaded: Vec<Vec<Step>> = Vec::new();
                for &stmt in stmts {
                    let mut written = Vec::new();
                    self.registers(stmt, &mut written);
                    let again = (written.iter())
                        .find(|(register, _)| loaded.iter().any(|it| overlap(it, register)));
                    if let Some(&(_, at)) = again {
                        self.found
                            .push((at, "register loaded again in `always_ff`", SELECT));
                    }
                    loaded.extend(written.into_iter().map(|(register, _)| register));
                    self.stmt(stmt, path);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let reset = resets::tested(hir, *condition).is_some_and(|(signal, _)| {
                    self.resets.iter().any(|it| same(hir, it.expr, signal))
                });
                let then = then_branch.map_or(Load::Nothing, |it| self.loads(it));
                let otherwise = else_branch.map_or(Load::Nothing, |it| self.loads(it));
                let path = match reset {
                    true => Path {
                        resets: path.resets + 1,
                        ..path
                    },
                    false => self.choice(*condition, then, otherwise, path),
                };
                for &branch in then_branch.iter().chain(else_branch) {
                    self.stmt(branch, path);
                }
            }
            StmtKind::Case { selector, .. } => {
                let at = selector.map_or(stmt.span, |it| hir[it].span);
                self.found.push((at, "`case` in `always_ff`", SELECT));
            }
            // A loop's header is not judged: it repeats registers.
            StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::Timed { body, .. } => {
                if let Some(body) = *body {
                    self.stmt(body, path);
                }
            }
            // An assertion, a wait, a `disable` computes no register.
            _ => {}
        }
    }

    /// Judges `lhs` loaded with `rhs`, a `?:` in which is judged as the
    /// `if` it could be written as.
    fn assigned(&mut self, lhs: ExprId, rhs: ExprId, path: Path) {
        match self.hir[rhs].kind {
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
                ..
            } if !self.constant(rhs) => {
                let then = self.loads_value(lhs, then_value);
                let otherwise = self.loads_value(lhs, else_value);
                let path = self.choice(condition, then, otherwise, path);
                self.assigned(lhs, then_value, path);
                self.assigned(lhs, else_value, path);
            }
            _ if same(self.hir, lhs, rhs) => {}
            _ => self.value(rhs, REGISTER),
        }
    }

    /// Judges a choice on `condition` between branches that load `then`
    /// and `otherwise`, under `path`, and returns the path they stand
    /// under. A finding leaves its branches to be judged as if past no
    /// enable, so that one structure is reported once.
    fn choice(&mut self, condition: ExprId, then: Load, otherwise: Load, path: Path) -> Path {
        if self.constant(condition) {
            return path;
        }
        let found = self.found.len();
        self.value(self.tested(condition), ENABLE);
        let at = self.hir[condition].span;
        let judged = Path {
            enabled: false,
            ..path
        };
        if self.found.len() > found {
            return judged;
        }
        if path.enabled {
            if then.max(otherwise) != Load::Nothing {
                let message = "condition under an enable in `always_ff`";
                self.found.push((at, message, ENABLE));
            }
            return judged;
        }
        match (then, otherwise) {
            (Load::Nothing, Load::Nothing) => path,
            (Load::Data, Load::Data) => {
                self.found.push((at, "mux in `always_ff`", SELECT));
                judged
            }
            (Load::Data, Load::Nothing) | (Load::Nothing, Load::Data) => Path {
                enabled: true,
                ..path
            },
            // A reset, or a clear, and what is loaded when it is not.
            _ => {
                if path.resets > 1 {
                    let message = "second clear in `always_ff`";
                    self.found.push((at, message, REGISTER));
                }
                Path {
                    resets: path.resets + 1,
                    ..path
                }
            }
        }
    }

    /// What `id` loads into the registers it assigns, all of its branches
    /// together.
    fn loads(&self, id: StmtId) -> Load {
        let hir = self.hir;
        let branch = |it: &Option<StmtId>| it.map_or(Load::Nothing, |it| self.loads(it));
        match &hir[id].kind {
            StmtKind::Expr(expr) => match hir[*expr].kind {
                ExprKind::Assign { lhs, rhs, .. } => self.loads_value(lhs, rhs),
                ExprKind::Call { .. } => Load::Nothing,
                _ => Load::Data,
            },
            StmtKind::Block { stmts, .. } => (stmts.iter().map(|&it| self.loads(it)))
                .max()
                .unwrap_or(Load::Nothing),
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => conditional(
                self.constant(*condition),
                branch(then_branch).max(branch(else_branch)),
            ),
            StmtKind::Case { .. } => Load::Data,
            StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::Timed { body, .. } => branch(body),
            _ => Load::Nothing,
        }
    }

    /// What `lhs` loaded with `rhs` loads, each value of a `?:` in it
    /// together.
    fn loads_value(&self, lhs: ExprId, rhs: ExprId) -> Load {
        match self.hir[rhs].kind {
            _ if same(self.hir, lhs, rhs) => Load::Nothing,
            _ if self.fixed(rhs) => Load::Constant,
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
                ..
            } => conditional(
                self.constant(condition),
                (self.loads_value(lhs, then_value)).max(self.loads_value(lhs, else_value)),
            ),
            _ => Load::Data,
        }
    }

    /// Pushes the registers `id` loads, each with where it is assigned: what
    /// a nonblocking assignment loads, as a variable a blocking one writes
    /// holds a value the process computes.
    fn registers(&self, id: StmtId, out: &mut Vec<(Vec<Step<'a>>, Span)>) {
        let hir = self.hir;
        match &hir[id].kind {
            StmtKind::Expr(expr) => match hir[*expr].kind {
                ExprKind::Assign {
                    op: LT_EQ,
                    lhs,
                    rhs,
                    ..
                } if !same(hir, lhs, rhs) => self.targets(lhs, out),
                _ => {}
            },
            StmtKind::Block { stmts, .. } => {
                for &stmt in stmts {
                    self.registers(stmt, out);
                }
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                for &branch in then_branch.iter().chain(else_branch) {
                    self.registers(branch, out);
                }
            }
            StmtKind::Case { items, .. } => {
                for body in items.iter().filter_map(|it| it.body) {
                    self.registers(body, out);
                }
            }
            StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::Timed { body, .. } => {
                if let Some(body) = *body {
                    self.registers(body, out);
                }
            }
            _ => {}
        }
    }

    /// Pushes the registers `lhs` names, each by the steps to it.
    fn targets(&self, lhs: ExprId, out: &mut Vec<(Vec<Step<'a>>, Span)>) {
        let hir = self.hir;
        let mut steps = Vec::new();
        let mut at = lhs;
        loop {
            match &hir[at].kind {
                ExprKind::Name(name) => {
                    steps.push(Step::Name(name));
                    break;
                }
                ExprKind::Member { base, name } => {
                    steps.push(Step::Name(&name.text));
                    at = *base;
                }
                ExprKind::Select { base, index, range } => {
                    let index = match range {
                        None => (self.body)
                            .and_then(|body| self.elaboration.eval(body, *index).bits()?.to_u64()),
                        Some(_) => None,
                    };
                    steps.push(Step::Index(index));
                    at = *base;
                }
                ExprKind::Concat(parts) => {
                    for &part in parts {
                        self.targets(part, out);
                    }
                    return;
                }
                _ => return,
            }
        }
        steps.reverse();
        out.push((steps, hir[lhs].span));
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

    /// Whether `id` evaluates to a constant, as `Width - 1` does.
    fn constant(&self, id: ExprId) -> bool {
        (self.body).is_some_and(|body| !matches!(self.elaboration.eval(body, id), Value::Unknown))
    }

    /// Whether `id` is a value a reset loads: a constant, or a pattern or
    /// a concatenation of them, which evaluate only once their type is
    /// known, as `'{default: '0}` does.
    fn fixed(&self, id: ExprId) -> bool {
        match &self.hir[id].kind {
            _ if self.constant(id) => true,
            ExprKind::Pattern { items, .. } => items.iter().all(|it| self.fixed(it.value)),
            ExprKind::Concat(parts) => parts.iter().all(|&it| self.fixed(it)),
            ExprKind::Replication { concat, .. } => self.fixed(*concat),
            ExprKind::Cast { operand, .. } => self.fixed(*operand),
            // An element of a constant, picked by a `generate` loop's index.
            ExprKind::Select { base, .. } | ExprKind::Member { base, .. } => self.fixed(*base),
            ExprKind::Literal { .. } => true,
            _ => false,
        }
    }
}

/// Whether two registers may be one: unless a member, or a constant index,
/// tells them apart, as `q[0]` and `q[1]` are.
fn overlap(a: &[Step], b: &[Step]) -> bool {
    a.iter().zip(b).all(|(a, b)| match (a, b) {
        (Step::Index(Some(i)), Step::Index(Some(j))) => i == j,
        (Step::Index(_), Step::Index(_)) => true,
        _ => a == b,
    })
}

/// What a choice loads, on a condition that is a `constant` or not, between
/// branches that together load `load`: constants a condition picks are no
/// longer what a reset loads.
fn conditional(constant: bool, load: Load) -> Load {
    match load {
        Load::Constant if !constant => Load::Data,
        _ => load,
    }
}
