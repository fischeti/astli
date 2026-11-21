//! What each statement and expression is made of, listed once so that a walk
//! matches only the kinds it treats apart and recurses into the rest.

use super::*;

/// A statement or an expression, as another is made of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Expr(ExprId),
    Stmt(StmtId),
}

impl Hir {
    /// Gives `f` each statement and expression `stmt` is made of, in the
    /// order written, those of a timing or a property among them. What its
    /// scopes declare is no part of it: a block's, a loop's, a match's.
    pub fn stmt_children(&self, stmt: StmtId, mut f: impl FnMut(Node)) {
        let f = &mut f;
        match &self[stmt].kind {
            StmtKind::Empty | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr)
            | StmtKind::Trigger(expr)
            | StmtKind::ProceduralAssign { expr, .. } => f(Node::Expr(*expr)),
            StmtKind::Disable(expr) | StmtKind::Return(expr) => maybe(*expr, f),
            StmtKind::Block { stmts, .. } => {
                for &stmt in stmts {
                    f(Node::Stmt(stmt));
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                f(Node::Expr(*condition));
                branch(*then_branch, f);
                branch(*else_branch, f);
            }
            StmtKind::Case {
                selector, items, ..
            } => {
                maybe(*selector, f);
                for item in items {
                    each(&item.labels, f);
                    branch(item.body, f);
                }
            }
            StmtKind::For { header, body } => {
                each(&header.init, f);
                maybe(header.condition, f);
                each(&header.step, f);
                branch(*body, f);
            }
            StmtKind::Foreach { array, body, .. } => {
                f(Node::Expr(*array));
                branch(*body, f);
            }
            StmtKind::Loop {
                condition, body, ..
            }
            | StmtKind::Wait { condition, body } => {
                maybe(*condition, f);
                branch(*body, f);
            }
            StmtKind::Timed { timing: at, body } => {
                timing(at, f);
                branch(*body, f);
            }
            StmtKind::Assertion {
                condition,
                pass,
                fail,
                ..
            } => {
                f(Node::Expr(*condition));
                branch(*pass, f);
                branch(*fail, f);
            }
            StmtKind::ConcurrentAssertion {
                spec, pass, fail, ..
            } => {
                if let Some(clock) = &spec.clock {
                    timing(clock, f);
                }
                maybe(spec.disable, f);
                f(Node::Expr(spec.property));
                branch(*pass, f);
                branch(*fail, f);
            }
            StmtKind::Opaque(opaque) => each(&opaque.names, f),
        }
    }

    /// Gives `f` each expression `expr` is made of, in the order written but
    /// for an assignment's timing, after its sides: its operands, arguments
    /// and items, and those of a timing, a type or a property among them;
    /// and the constraints of `with {…}`, a statement. What a `with (…)`
    /// declares is no part of it.
    pub fn expr_children(&self, expr: ExprId, mut f: impl FnMut(Node)) {
        let f = &mut f;
        match &self[expr].kind {
            ExprKind::Name(_)
            | ExprKind::System(_)
            | ExprKind::Keyword(_)
            | ExprKind::Literal { .. }
            | ExprKind::Time { .. }
            | ExprKind::Bind(_)
            | ExprKind::Loose(_) => {}
            ExprKind::Unary { operand, .. } | ExprKind::Postfix { operand, .. } => {
                f(Node::Expr(*operand));
            }
            ExprKind::Binary { lhs, rhs, .. }
            | ExprKind::Range {
                lo: lhs, hi: rhs, ..
            } => {
                each(&[*lhs, *rhs], f);
            }
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
                ..
            } => each(&[*condition, *then_value, *else_value], f),
            ExprKind::Select { base, index, range } => {
                each(&[*base, *index], f);
                maybe(range.map(|(_, end)| end), f);
            }
            ExprKind::Member { base, .. } | ExprKind::Scoped { base, .. } => f(Node::Expr(*base)),
            ExprKind::Call { callee, args } => {
                f(Node::Expr(*callee));
                for arg in args {
                    match arg {
                        Arg::Positional(value) | Arg::Named { value, .. } => maybe(*value, f),
                        Arg::Implicit { value, .. } => f(Node::Expr(*value)),
                        Arg::Wildcard(_) => {}
                    }
                }
            }
            ExprKind::Concat(parts) => each(parts, f),
            ExprKind::Replication { count, concat } => each(&[*count, *concat], f),
            ExprKind::Stream { slice, concat, .. } => {
                maybe(*slice, f);
                f(Node::Expr(*concat));
            }
            ExprKind::Pattern { ty, items } => {
                maybe(*ty, f);
                for item in items {
                    maybe(item.key, f);
                    f(Node::Expr(item.value));
                }
            }
            ExprKind::Cast { ty, operand } => each(&[*ty, *operand], f),
            ExprKind::Inside { expr, set } | ExprKind::Dist { expr, set } => {
                f(Node::Expr(*expr));
                each(set, f);
            }
            ExprKind::Binsof { bins, intersect } => {
                f(Node::Expr(*bins));
                each(intersect, f);
            }
            ExprKind::Assign {
                lhs,
                rhs,
                timing: at,
                ..
            } => {
                each(&[*lhs, *rhs], f);
                if let Some(at) = at {
                    timing(at, f);
                }
            }
            ExprKind::With { base, with } => each(&[*base, with.expr], f),
            ExprKind::Constrained {
                call, constraints, ..
            } => {
                f(Node::Expr(*call));
                f(Node::Stmt(*constraints));
            }
            ExprKind::Tagged { value, .. } => maybe(*value, f),
            ExprKind::Type(ty) => self.type_children(ty, f),
            ExprKind::Property(property) => property_children(property, f),
            ExprKind::Opaque(opaque) => each(&opaque.names, f),
        }
    }

    /// The expressions a type is written with: what names it, its struct's
    /// members' and its enum's base, and its dimensions. An enum's members
    /// are declarations, and what a pattern variable's type is the type of
    /// is where it is matched.
    fn type_children(&self, ty: &Type, f: &mut impl FnMut(Node)) {
        match &ty.kind {
            TypeKind::Implicit { .. } | TypeKind::Builtin { .. } | TypeKind::Matched { .. } => {}
            TypeKind::Named(expr) | TypeKind::TypeOf(expr) => f(Node::Expr(*expr)),
            TypeKind::Enum(id) => self.type_children(&self[*id].base, f),
            TypeKind::Struct { fields, .. } => {
                for field in fields {
                    self.type_children(&field.ty, f);
                    maybe(field.default, f);
                }
            }
            TypeKind::Opaque(opaque) => each(&opaque.names, f),
        }
        for dim in &ty.dims {
            match dim {
                Dim::Range(hi, lo) => each(&[*hi, *lo], f),
                Dim::Single(size) => f(Node::Expr(*size)),
                Dim::Empty | Dim::Wildcard => {}
            }
        }
    }
}

fn property_children(property: &Property, f: &mut impl FnMut(Node)) {
    match property {
        Property::Binary { lhs, rhs, .. } => each(&[*lhs, *rhs], f),
        Property::Unary {
            bounds: range,
            condition,
            operand,
            ..
        } => {
            bounds(range, f);
            maybe(*condition, f);
            f(Node::Expr(*operand));
        }
        Property::Delay {
            lhs,
            bounds: range,
            rhs,
            ..
        } => {
            maybe(*lhs, f);
            bounds(range, f);
            f(Node::Expr(*rhs));
        }
        Property::Repetition {
            operand,
            bounds: range,
            ..
        } => {
            f(Node::Expr(*operand));
            bounds(range, f);
        }
        Property::Match { sequence, items } => {
            f(Node::Expr(*sequence));
            each(items, f);
        }
        Property::If {
            condition,
            then_branch,
            else_branch,
        } => {
            each(&[*condition, *then_branch], f);
            maybe(*else_branch, f);
        }
        Property::Case { selector, arms } => {
            f(Node::Expr(*selector));
            for arm in arms {
                each(&arm.labels, f);
                f(Node::Expr(arm.property));
            }
        }
        Property::Clocked { clock, property } => {
            timing(clock, f);
            f(Node::Expr(*property));
        }
    }
}

fn bounds(range: &Option<Bounds>, f: &mut impl FnMut(Node)) {
    if let Some(range) = range {
        f(Node::Expr(range.low));
        maybe(range.high, f);
    }
}

fn timing(timing: &Timing, f: &mut impl FnMut(Node)) {
    let events = match timing {
        Timing::Delay(_, values) => return each(values, f),
        Timing::Star => return,
        Timing::Events(events) => events,
        Timing::Repeat(count, events) => {
            f(Node::Expr(*count));
            events
        }
    };
    for event in events {
        f(Node::Expr(event.expr));
        maybe(event.iff, f);
    }
}

fn each(exprs: &[ExprId], f: &mut impl FnMut(Node)) {
    for &expr in exprs {
        f(Node::Expr(expr));
    }
}

fn maybe(expr: Option<ExprId>, f: &mut impl FnMut(Node)) {
    if let Some(expr) = expr {
        f(Node::Expr(expr));
    }
}

fn branch(stmt: Option<StmtId>, f: &mut impl FnMut(Node)) {
    if let Some(stmt) = stmt {
        f(Node::Stmt(stmt));
    }
}
