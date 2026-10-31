//! Expressions: the type each has by itself, and its value once the
//! expression around it has sized it.
//!
//! An operator whose operands are context-determined (`+`, `&`, `?:`'s arms)
//! evaluates them at the width of the whole expression, and signed only if
//! every one of them is; one whose operands are self-determined (`==`, `&&`,
//! a shift's amount, a concatenation) evaluates each at its own type and
//! yields a type of its own.

use std::cmp::Ordering;
use std::sync::Arc;

use astli_syntax::SyntaxKind::{self, *};

use super::{Cx, Queries, builtin};
use crate::design::SymbolRef;
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::{Packed, Shape, Ty};
use crate::value::{Bit, Bits, Literal, Value, literal};

impl Queries {
    /// `expr`'s type by itself, before the expression around it sizes it.
    pub(crate) fn self_type(&self, cx: Cx, expr: ExprId) -> Ty {
        let hir = &self.design[cx.file];
        match &hir[expr].kind {
            ExprKind::Literal(text) => match literal(text) {
                Literal::Bits(bits) => {
                    Ty::Packed(Packed::vector(bits.width(), bits.is_signed(), true))
                }
                Literal::Fill(_) => Ty::BIT,
                Literal::Real(_) => Ty::Real,
                Literal::String(bytes) => {
                    let width = (bytes.len().max(1) * 8) as u32;
                    Ty::Packed(Packed::vector(width, false, false))
                }
                Literal::Unknown => Ty::Unknown,
            },
            // A `realtime`.
            ExprKind::Time { .. } => Ty::Real,
            ExprKind::Keyword(NULL_KW) => Ty::Null,
            // A system function called without parentheses, as `$time`.
            ExprKind::System(name) => self.system_type(cx, name, &[]),
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.resolution(cx, expr) {
                Some(Resolution::Declared(at)) => self.type_in(cx, at),
                _ => Ty::Unknown,
            },
            ExprKind::Unary { op, operand } => match op {
                PLUS | MINUS | TILDE | PLUS_PLUS | MINUS_MINUS => self.self_type(cx, *operand),
                BANG | AMP | PIPE | CARET | TILDE_AMP | TILDE_PIPE | TILDE_CARET | CARET_TILDE => {
                    Ty::BIT
                }
                _ => Ty::Unknown,
            },
            ExprKind::Postfix { operand, .. } => self.self_type(cx, *operand),
            ExprKind::Binary { op, lhs, rhs } => match Operator::of(*op) {
                Some(Operator::Context) => {
                    merge(&self.self_type(cx, *lhs), &self.self_type(cx, *rhs))
                }
                Some(Operator::Power) => match self.self_type(cx, *rhs) {
                    Ty::Real => merge(&self.self_type(cx, *lhs), &Ty::Real),
                    _ => self.self_type(cx, *lhs),
                },
                Some(Operator::Shift) => self.self_type(cx, *lhs),
                Some(Operator::Compare | Operator::Logical) => Ty::BIT,
                None => Ty::Unknown,
            },
            ExprKind::Conditional {
                then_value,
                else_value,
                ..
            } => {
                // An untyped pattern in one arm takes the other's type, if
                // that has one of its own: a fill, as `'0`, has none either.
                let untyped =
                    |arm: ExprId| matches!(hir[arm].kind, ExprKind::Pattern { ty: None, .. });
                let fill = |arm: ExprId| match &hir[arm].kind {
                    ExprKind::Literal(text) => matches!(literal(text), Literal::Fill(_)),
                    _ => false,
                };
                // A string literal against a `string` is one too.
                let text = |arm: ExprId| match &hir[arm].kind {
                    ExprKind::Literal(text) => matches!(literal(text), Literal::String(_)),
                    _ => false,
                };
                match (untyped(*then_value), untyped(*else_value)) {
                    (false, true) if !fill(*then_value) => self.self_type(cx, *then_value),
                    (true, false) if !fill(*else_value) => self.self_type(cx, *else_value),
                    _ => match (
                        self.self_type(cx, *then_value),
                        self.self_type(cx, *else_value),
                    ) {
                        (Ty::String, _) if text(*else_value) => Ty::String,
                        (_, Ty::String) if text(*then_value) => Ty::String,
                        (a, b) => merge(&a, &b),
                    },
                }
            }
            ExprKind::Select { base, index, range } => match self.self_type(cx, *base) {
                // An element, or a slice whose size its bounds' values give.
                Ty::Dynamic(dynamic) => match range {
                    None => dynamic.element.clone(),
                    Some(_) => Ty::Dynamic(dynamic),
                },
                // A character, a `byte`.
                Ty::String if range.is_none() => Ty::Packed(Packed::vector(8, true, false)),
                base => match self.selected(cx, &base, *index, *range) {
                    Some(selected) => selected.ty,
                    None => Ty::Unknown,
                },
            },
            ExprKind::Member { base, name } => {
                if let Some((cx, at)) = self.through(cx, expr, *base, &name.text) {
                    return self.type_in(cx, at);
                }
                let base = self.self_type(cx, *base);
                if let Some(member) = member(&base, &name.text) {
                    return member.ty;
                }
                // A member of an object, `h.m` or `this.m`: what its class
                // declares or inherits.
                match self.resolution(cx, expr) {
                    Some(Resolution::Declared(at)) if self.in_class(at) => self.type_in(cx, at),
                    _ => Ty::Unknown,
                }
            }
            ExprKind::Call { callee, args } => self.call_type(cx, *callee, args),
            ExprKind::Concat(parts) => self.concat_type(cx, parts),
            ExprKind::Replication { count, concat } => {
                match (self.count(cx, *count), self.self_type(cx, *concat)) {
                    (Some(count), Ty::Packed(each)) => {
                        let width = u64::from(each.width) * count;
                        match width {
                            1.. if width <= u64::from(Bits::MAX_WIDTH) => {
                                Ty::Packed(Packed::vector(width as u32, false, each.four_state))
                            }
                            _ => Ty::Unknown,
                        }
                    }
                    _ => Ty::Unknown,
                }
            }
            ExprKind::Cast { ty, operand } => match self.cast_target(cx, *ty, *operand) {
                Cast::Signed(signed) => match self.self_type(cx, *operand) {
                    Ty::Packed(own) => Ty::Packed(Packed { signed, ..own }),
                    _ => Ty::Unknown,
                },
                Cast::To(ty) => ty,
            },
            // An untyped pattern takes the type of what it is assigned to.
            ExprKind::Pattern { ty: Some(ty), .. } => self.type_or_expr(cx, *ty),
            ExprKind::Inside { .. } => Ty::BIT,
            ExprKind::Assign { lhs, .. } => self.self_type(cx, *lhs),
            _ => Ty::Unknown,
        }
    }

    /// Where `expr`, `base.name` with `base` an interface instance or an
    /// interface port, finds `name`: in the interface's body. A generic
    /// port's member is found only there, as the port names no interface.
    fn through(
        &self,
        cx: Cx,
        expr: ExprId,
        base: ExprId,
        name: &str,
    ) -> Option<(Cx<'static>, SymbolRef)> {
        let body = self.interface(cx, base)?;
        let at = match self.resolution(cx, expr) {
            Some(Resolution::Declared(at)) => at,
            _ => {
                let (file, scope) = self.scope(body);
                let symbol = self.design.declared(file, scope, name)?;
                SymbolRef { file, symbol }
            }
        };
        Some((self.cx(body), at))
    }

    /// Whether `at` is declared in a class: one of its properties or
    /// methods.
    fn in_class(&self, at: SymbolRef) -> bool {
        let hir = &self.design[at.file];
        let home = self.design.home(at);
        matches!(hir[home].kind, ScopeKind::Class)
    }

    /// `expr` evaluated at its own type.
    pub(crate) fn eval_self(&self, cx: Cx, expr: ExprId) -> Value {
        let ty = self.self_type(cx, expr);
        self.eval_at(cx, expr, &ty)
    }

    /// `expr` assigned to a declaration of type `target`: evaluated at the
    /// wider of the two, then converted. A pattern takes its shape from
    /// `target`, as does a concatenation assigned to an unpacked array.
    pub(crate) fn eval_assign(&self, cx: Cx, expr: ExprId, target: &Ty) -> Value {
        let hir = &self.design[cx.file];
        match (&hir[expr].kind, target) {
            (ExprKind::Pattern { ty, items }, _) => {
                let ty = match ty {
                    Some(ty) => self.type_or_expr(cx, *ty),
                    None => target.clone(),
                };
                let value = self.pattern(cx, items, None, &ty);
                return self.convert(value, target);
            }
            // So does a pattern chosen between, which has no type of its own
            // to merge with the other arm's.
            (
                ExprKind::Conditional {
                    condition,
                    then_value,
                    else_value,
                },
                _,
            ) if [then_value, else_value]
                .iter()
                .any(|&&arm| matches!(hir[arm].kind, ExprKind::Pattern { ty: None, .. })) =>
            {
                return match truth(&self.eval_self(cx, *condition)) {
                    Some(Bit::One) => self.eval_assign(cx, *then_value, target),
                    Some(Bit::Zero) => self.eval_assign(cx, *else_value, target),
                    _ => Value::Unknown,
                };
            }
            (ExprKind::Concat(parts), Ty::Array(array)) => {
                return self.unpacked_concat(cx, parts, array);
            }
            (ExprKind::Concat(parts), Ty::Dynamic(dynamic)) => {
                return self.dynamic_concat(cx, parts, dynamic);
            }
            _ => {}
        }
        let own = self.self_type(cx, expr);
        let value = match (target, &own) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => return Value::Unknown,
            (Ty::Packed(target), Ty::Packed(own)) => {
                let width = target.width.max(own.width);
                let ty = Packed::vector(width, own.signed, own.four_state);
                self.eval_at(cx, expr, &Ty::Packed(ty))
            }
            (_, own) => self.eval_at(cx, expr, own),
        };
        self.convert(value, target)
    }

    /// `expr` evaluated where the expression around it has type `ty`: at
    /// least as wide as its own, and signed only if the expression is.
    pub(crate) fn eval_at(&self, cx: Cx, expr: ExprId, ty: &Ty) -> Value {
        if *ty == Ty::Unknown {
            return Value::Unknown;
        }
        let hir = &self.design[cx.file];
        let value = match &hir[expr].kind {
            ExprKind::Literal(text) => match literal(text) {
                Literal::Bits(bits) => Value::Bits(bits),
                Literal::Fill(bit) => match ty {
                    Ty::Packed(ty) => return Value::Bits(Bits::filled(ty.width, ty.signed, bit)),
                    _ => Value::Bits(Bits::bit_value(bit)),
                },
                Literal::Real(real) => Value::Real(real),
                Literal::String(bytes) if *ty == Ty::String => Value::String(bytes),
                Literal::String(bytes) => Value::Bits(string_bits(&bytes)),
                Literal::Unknown => Value::Unknown,
            },
            ExprKind::Time { value, .. } => value.map_or(Value::Unknown, Value::Real),
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.resolution(cx, expr) {
                Some(Resolution::Declared(at)) => self.symbol_value(cx, at),
                _ => Value::Unknown,
            },
            ExprKind::Unary { op, operand } => match op {
                PLUS => return self.eval_at(cx, *operand, ty),
                MINUS => {
                    return match self.eval_at(cx, *operand, ty) {
                        Value::Bits(bits) => Value::Bits(bits.neg()),
                        Value::Real(real) => Value::Real(-real),
                        _ => Value::Unknown,
                    };
                }
                TILDE => {
                    return match self.eval_at(cx, *operand, ty) {
                        Value::Bits(bits) => Value::Bits(bits.not()),
                        _ => Value::Unknown,
                    };
                }
                BANG => match truth(&self.eval_self(cx, *operand)) {
                    Some(bit) => Value::Bits(Bits::bit_value(not(bit))),
                    None => Value::Unknown,
                },
                PLUS_PLUS | MINUS_MINUS => self.step(cx, *operand, *op, true),
                op => match self.eval_self(cx, *operand) {
                    Value::Bits(bits) => Value::Bits(Bits::bit_value(reduce(*op, &bits))),
                    _ => Value::Unknown,
                },
            },
            ExprKind::Postfix { op, operand } => self.step(cx, *operand, *op, false),
            ExprKind::Binary { op, lhs, rhs } => return self.binary(cx, *op, *lhs, *rhs, ty),
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                return match truth(&self.eval_self(cx, *condition)) {
                    Some(Bit::One) => self.eval_at(cx, *then_value, ty),
                    Some(Bit::Zero) => self.eval_at(cx, *else_value, ty),
                    Some(_) => {
                        let then_value = self.eval_at(cx, *then_value, ty);
                        match (then_value, self.eval_at(cx, *else_value, ty)) {
                            (Value::Bits(a), Value::Bits(b)) => Value::Bits(a.merge(&b)),
                            _ => Value::Unknown,
                        }
                    }
                    None => Value::Unknown,
                };
            }
            ExprKind::Select { base, index, range } => self.select(cx, *base, *index, *range),
            ExprKind::Member { base, name } => match self.through(cx, expr, *base, &name.text) {
                Some((cx, at)) => self.symbol_value(cx, at),
                None => self.member_value(cx, *base, &name.text),
            },
            ExprKind::Call { callee, args } => self.call(cx, *callee, args),
            ExprKind::Concat(parts) => self.concat(cx, parts),
            ExprKind::Replication { count, concat } => {
                match (self.count(cx, *count), self.eval_self(cx, *concat)) {
                    (Some(count @ 1..), Value::Bits(each))
                        if u64::from(each.width()) * count <= u64::from(Bits::MAX_WIDTH) =>
                    {
                        Value::Bits(Bits::concat(std::iter::repeat_n(&each, count as usize)))
                    }
                    _ => Value::Unknown,
                }
            }
            ExprKind::Cast {
                ty: target,
                operand,
            } => match self.cast_target(cx, *target, *operand) {
                Cast::Signed(signed) => match self.eval_self(cx, *operand) {
                    Value::Bits(bits) => Value::Bits(bits.with_signed(signed)),
                    _ => Value::Unknown,
                },
                Cast::To(target) => self.eval_assign(cx, *operand, &target),
            },
            ExprKind::Pattern { ty: written, items } => {
                let own = match written {
                    Some(written) => self.type_or_expr(cx, *written),
                    None => ty.clone(),
                };
                self.pattern(cx, items, None, &own)
            }
            ExprKind::Assign { op, lhs, rhs, .. } => match self.assign(cx, *op, *lhs, *rhs) {
                Some(value) => value,
                None => Value::Unknown,
            },
            ExprKind::Inside { expr: tested, set } => {
                let common = self.set_type(cx, *tested, set);
                let tested = self.eval_at(cx, *tested, &common);
                let matches = set
                    .iter()
                    .map(|&item| self.in_item(cx, &tested, item, &common));
                match any(matches) {
                    Some(bit) => Value::Bits(Bits::bit_value(bit)),
                    None => Value::Unknown,
                }
            }
            _ => Value::Unknown,
        };
        operand(value, ty)
    }

    /// The type `tested` and the members of `set` are compared at: the
    /// widest of them, as a case's selector and labels are. An unpacked
    /// array counts by its elements, a range by its bounds but `$`, and a
    /// tolerance range by its value.
    pub(crate) fn set_type(&self, cx: Cx, tested: ExprId, set: &[ExprId]) -> Ty {
        let hir = &self.design[cx.file];
        let mut ty = self.self_type(cx, tested);
        for &item in set {
            let bounds = match &hir[item].kind {
                ExprKind::Range { op: COLON, lo, hi } => [Some(*lo), Some(*hi)],
                ExprKind::Range { lo, .. } => [Some(*lo), None],
                _ => [Some(item), None],
            };
            for bound in bounds.into_iter().flatten() {
                if !matches!(hir[bound].kind, ExprKind::Keyword(DOLLAR)) {
                    ty = merge(&ty, singular(&self.self_type(cx, bound)));
                }
            }
        }
        ty
    }

    /// Whether `tested` is in `item`, a member of an `inside` set or a
    /// `case … inside` label, both at `ty`: `None` if `item` cannot be
    /// evaluated, as a tolerance range is not.
    pub(crate) fn in_item(&self, cx: Cx, tested: &Value, item: ExprId, ty: &Ty) -> Option<Bit> {
        let hir = &self.design[cx.file];
        // Whether `tested` is `op` the bound, which `$` leaves open.
        let bound = |op, bound: ExprId| match hir[bound].kind {
            ExprKind::Keyword(DOLLAR) => Some(Bit::One),
            _ => compare(op, tested, &self.eval_at(cx, bound, ty)),
        };
        match &hir[item].kind {
            ExprKind::Range { op: COLON, lo, hi } => {
                Some(and(bound(GT_EQ, *lo)?, bound(LT_EQ, *hi)?))
            }
            ExprKind::Range { .. } => None,
            _ => match self.self_type(cx, item) {
                Ty::Array(_) | Ty::Dynamic(_) => in_value(tested, self.eval_self(cx, item), ty),
                _ => in_value(tested, self.eval_at(cx, item, ty), ty),
            },
        }
    }

    fn binary(&self, cx: Cx, op: SyntaxKind, lhs: ExprId, rhs: ExprId, ty: &Ty) -> Value {
        match Operator::of(op) {
            Some(Operator::Context) => {
                let lhs = self.eval_at(cx, lhs, ty);
                apply(op, lhs, self.eval_at(cx, rhs, ty))
            }
            Some(Operator::Power | Operator::Shift) => {
                let lhs = self.eval_at(cx, lhs, ty);
                apply(op, lhs, self.eval_self(cx, rhs))
            }
            Some(Operator::Compare) => {
                let common = merge(&self.self_type(cx, lhs), &self.self_type(cx, rhs));
                let lhs = self.eval_at(cx, lhs, &common);
                let rhs = self.eval_at(cx, rhs, &common);
                match compare(op, &lhs, &rhs) {
                    Some(bit) => operand(Value::Bits(Bits::bit_value(bit)), ty),
                    None => Value::Unknown,
                }
            }
            Some(Operator::Logical) => {
                let lhs = truth(&self.eval_self(cx, lhs));
                // The right side decides nothing once the left has.
                let decided = match op {
                    AMP_AMP => lhs == Some(Bit::Zero),
                    PIPE_PIPE => lhs == Some(Bit::One),
                    MINUS_GT => lhs == Some(Bit::Zero),
                    _ => false,
                };
                let rhs = match decided {
                    true => None,
                    false => truth(&self.eval_self(cx, rhs)),
                };
                let bit = match (op, lhs, rhs) {
                    (AMP_AMP, Some(Bit::Zero), _) | (AMP_AMP, _, Some(Bit::Zero)) => Bit::Zero,
                    (PIPE_PIPE, Some(Bit::One), _) | (PIPE_PIPE, _, Some(Bit::One)) => Bit::One,
                    (MINUS_GT, Some(Bit::Zero), _) | (MINUS_GT, _, Some(Bit::One)) => Bit::One,
                    (_, Some(a), Some(b)) => match op {
                        AMP_AMP => and(a, b),
                        PIPE_PIPE => not(and(not(a), not(b))),
                        MINUS_GT => not(and(a, not(b))),
                        _ => match (a, b) {
                            (Bit::Zero | Bit::One, Bit::Zero | Bit::One) => Bit::from(a == b),
                            _ => Bit::X,
                        },
                    },
                    _ => return Value::Unknown,
                };
                operand(Value::Bits(Bits::bit_value(bit)), ty)
            }
            None => Value::Unknown,
        }
    }

    /// A value written to a declaration of type `target`: truncated or
    /// extended to it, `x` and `z` dropped by a two-state one; an
    /// aggregate's elements each written to theirs.
    pub(crate) fn convert(&self, value: Value, target: &Ty) -> Value {
        match (value, target) {
            (Value::Bits(bits), Ty::Packed(ty)) => {
                let bits = bits.resize(ty.width).with_signed(ty.signed);
                Value::Bits(match ty.four_state {
                    true => bits,
                    false => bits.to_two_state(),
                })
            }
            (Value::Real(real), Ty::Packed(ty)) => {
                Value::Bits(Bits::from_f64(ty.width, ty.signed, real))
            }
            (Value::String(bytes), Ty::Packed(ty)) => {
                Value::Bits(string_bits(&bytes).resize(ty.width).with_signed(ty.signed))
            }
            (Value::Bits(bits), Ty::Real) => Value::Real(bits.to_f64()),
            (Value::Real(real), Ty::Real) => Value::Real(real),
            (Value::Bits(bits), Ty::String) => Value::String(bits_string(&bits)),
            (Value::String(bytes), Ty::String) => Value::String(bytes),
            (Value::Array(values), Ty::Array(array)) => {
                let size = array.left.abs_diff(array.right) + 1;
                if values.len() as u64 != size {
                    return Value::Unknown;
                }
                self.convert_elements(&values, &array.element)
            }
            (Value::Array(values), Ty::Dynamic(dynamic)) => {
                self.convert_elements(&values, &dynamic.element)
            }
            (Value::Struct(values), Ty::Struct(ty)) if values.len() == ty.members.len() => {
                Value::Struct(
                    (values.iter().zip(&ty.members))
                        .map(|(value, member)| self.convert(value.clone(), &member.ty))
                        .collect(),
                )
            }
            _ => Value::Unknown,
        }
    }

    /// An array's elements written to one whose elements are of type
    /// `element`, which they must be equivalent to: a packed one as wide
    /// and signed alike.
    fn convert_elements(&self, values: &[Value], element: &Ty) -> Value {
        let equivalent = |value: &Value| match (value, element) {
            (Value::Bits(bits), Ty::Packed(packed)) => {
                bits.width() == packed.width && bits.is_signed() == packed.signed
            }
            _ => true,
        };
        if !values.iter().all(equivalent) {
            return Value::Unknown;
        }
        Value::Array(
            (values.iter())
                .map(|value| self.convert(value.clone(), element))
                .collect(),
        )
    }

    // ------------------------------------------------------ concatenations

    /// A replication's count, if it is a constant that is not negative.
    pub(crate) fn count(&self, cx: Cx, count: ExprId) -> Option<u64> {
        self.integer(cx, count)
            .and_then(|count| u64::try_from(count).ok())
    }

    /// Whether `part` of a concatenation is a replication by zero, which
    /// stands for nothing there.
    fn empty(&self, cx: Cx, part: ExprId) -> bool {
        let hir = &self.design[cx.file];
        matches!(hir[part].kind, ExprKind::Replication { count, .. } if self.count(cx, count) == Some(0))
    }

    fn concat_type(&self, cx: Cx, parts: &[ExprId]) -> Ty {
        let mut width = 0u64;
        let mut four_state = false;
        for &part in parts {
            if self.empty(cx, part) {
                continue;
            }
            match self.self_type(cx, part) {
                Ty::Packed(part) => {
                    width += u64::from(part.width);
                    four_state |= part.four_state;
                }
                Ty::String => return Ty::String,
                _ => return Ty::Unknown,
            }
        }
        match width {
            1.. if width <= u64::from(Bits::MAX_WIDTH) => {
                Ty::Packed(Packed::vector(width as u32, false, four_state))
            }
            _ => Ty::Unknown,
        }
    }

    fn concat(&self, cx: Cx, parts: &[ExprId]) -> Value {
        if self.concat_type(cx, parts) == Ty::String {
            let mut bytes = Vec::new();
            for &part in parts {
                match self.eval_at(cx, part, &Ty::String) {
                    Value::String(part) => bytes.extend_from_slice(&part),
                    _ => return Value::Unknown,
                }
            }
            return Value::String(bytes.into());
        }
        let mut values = Vec::new();
        for &part in parts {
            if self.empty(cx, part) {
                continue;
            }
            match self.eval_self(cx, part) {
                Value::Bits(bits) => values.push(bits),
                _ => return Value::Unknown,
            }
        }
        match values.is_empty() {
            true => Value::Unknown,
            false => Value::Bits(Bits::concat(&values)),
        }
    }

    // --------------------------------------------------------------- casts

    /// What `target'(operand)` casts to.
    fn cast_target(&self, cx: Cx, target: ExprId, operand: ExprId) -> Cast {
        let hir = &self.design[cx.file];
        match &hir[target].kind {
            ExprKind::Keyword(SIGNED_KW) => Cast::Signed(true),
            ExprKind::Keyword(UNSIGNED_KW) => Cast::Signed(false),
            ExprKind::Keyword(CONST_KW) => Cast::To(self.self_type(cx, operand)),
            ExprKind::Keyword(keyword) => Cast::To(builtin(*keyword, None)),
            ExprKind::Type(ty) => Cast::To(self.resolve_type(cx, ty)),
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.resolution(cx, target) {
                Some(Resolution::Declared(at)) => match &self.design.symbol(at).kind {
                    SymbolKind::Typedef(_) => Cast::To(self.type_in(cx, at)),
                    SymbolKind::Parameter(parameter) if parameter.is_type => {
                        Cast::To(self.type_in(cx, at))
                    }
                    SymbolKind::Parameter(_) => self.width_cast(cx, target, operand),
                    _ => Cast::To(Ty::Unknown),
                },
                _ => Cast::To(Ty::Unknown),
            },
            _ => self.width_cast(cx, target, operand),
        }
    }

    /// `W'(operand)`: `W` bits, its signing and states the operand's.
    fn width_cast(&self, cx: Cx, width: ExprId, operand: ExprId) -> Cast {
        let width = self
            .integer(cx, width)
            .and_then(|width| u32::try_from(width).ok());
        match (width, self.self_type(cx, operand)) {
            (Some(width @ 1..), Ty::Packed(own)) if width <= Bits::MAX_WIDTH => Cast::To(
                Ty::Packed(Packed::vector(width, own.signed, own.four_state)),
            ),
            _ => Cast::To(Ty::Unknown),
        }
    }
}

enum Cast {
    /// `signed'` or `unsigned'`, which changes only the signing.
    Signed(bool),
    /// To a type, as an assignment converts.
    To(Ty),
}

/// How a binary operator sizes its operands.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operator {
    /// Both operands at the expression's type: arithmetic and bitwise.
    Context,
    /// The left at the expression's type, the exponent by itself.
    Power,
    /// The left at the expression's type, the amount by itself.
    Shift,
    /// The operands at the wider of the two, the result one bit.
    Compare,
    /// Each operand by itself, the result one bit.
    Logical,
}

impl Operator {
    pub(crate) fn of(op: SyntaxKind) -> Option<Operator> {
        Some(match op {
            PLUS | MINUS | STAR | SLASH | PERCENT | AMP | PIPE | CARET | CARET_TILDE
            | TILDE_CARET => Operator::Context,
            STAR_STAR => Operator::Power,
            LT_LT | LT_LT_LT | GT_GT | GT_GT_GT => Operator::Shift,
            EQ_EQ | BANG_EQ | EQ_EQ_EQ | BANG_EQ_EQ | EQ_EQ_QUESTION | BANG_EQ_QUESTION | LT
            | LT_EQ | GT | GT_EQ => Operator::Compare,
            AMP_AMP | PIPE_PIPE | MINUS_GT | LT_MINUS_GT => Operator::Logical,
            _ => return None,
        })
    }
}

/// The type of an expression whose operands have types `a` and `b`, each
/// extended to it: the type both have, if they have one, else real if
/// either is, as wide as the wider, signed if both are. Aggregates only
/// meet their own type.
///
/// The standard leaves open whether two enums of one type give that enum;
/// they do, so `sel ? Idle : Busy` and `a | b` assign to one without a cast.
pub(crate) fn merge(a: &Ty, b: &Ty) -> Ty {
    match (a, b) {
        (Ty::Packed(_), Ty::Packed(_)) if a == b => a.clone(),
        (Ty::Packed(a), Ty::Packed(b)) => Ty::Packed(Packed::vector(
            a.width.max(b.width),
            a.signed && b.signed,
            a.four_state || b.four_state,
        )),
        (Ty::Real, Ty::Real | Ty::Packed(_)) | (Ty::Packed(_), Ty::Real) => Ty::Real,
        (Ty::String, Ty::String) => Ty::String,
        (Ty::Array(_) | Ty::Dynamic(_) | Ty::Struct(_) | Ty::Class(_) | Ty::Null, _) if a == b => {
            a.clone()
        }
        // `null` takes the handle's type.
        (Ty::Class(_), Ty::Null) => a.clone(),
        (Ty::Null, Ty::Class(_)) => b.clone(),
        _ => Ty::Unknown,
    }
}

/// A value evaluated at its own type, taken to the type `ty` of the
/// expression around it: extended by its sign only if that is signed.
pub(crate) fn operand(value: Value, ty: &Ty) -> Value {
    match (value, ty) {
        (Value::Bits(bits), Ty::Packed(ty)) => {
            Value::Bits(bits.with_signed(ty.signed).resize(ty.width))
        }
        (Value::Bits(bits), Ty::Real) => Value::Real(bits.to_f64()),
        (Value::Real(real), Ty::Packed(ty)) => {
            Value::Bits(Bits::from_f64(ty.width, ty.signed, real))
        }
        (Value::Real(real), Ty::Real) => Value::Real(real),
        (Value::String(bytes), Ty::String) => Value::String(bytes),
        (Value::String(bytes), Ty::Packed(ty)) => {
            Value::Bits(string_bits(&bytes).with_signed(ty.signed).resize(ty.width))
        }
        (Value::Bits(bits), Ty::String) => Value::String(bits_string(&bits)),
        (value @ Value::Array(_), Ty::Array(_) | Ty::Dynamic(_))
        | (value @ Value::Struct(_), Ty::Struct(_)) => value,
        _ => Value::Unknown,
    }
}

/// The most elements an unpacked array may hold for sema to give it a
/// value: more than a constant table holds, and few enough to build.
pub(crate) const ELEMENTS: u64 = 1 << 20;

/// What a variable of type `ty` holds before it is written: `x` if it has
/// four states, 0 if two, and an aggregate of such.
pub(crate) fn initial(ty: &Ty) -> Value {
    match ty {
        Ty::Packed(ty) => {
            let bit = if ty.four_state { Bit::X } else { Bit::Zero };
            Value::Bits(Bits::filled(ty.width, ty.signed, bit))
        }
        Ty::Real => Value::Real(0.0),
        Ty::String => Value::String(Box::default()),
        Ty::Array(array) => {
            let size = array.left.abs_diff(array.right) + 1;
            match size <= ELEMENTS {
                true => Value::Array((0..size).map(|_| initial(&array.element)).collect()),
                false => Value::Unknown,
            }
        }
        Ty::Dynamic(_) => Value::Array(Arc::new([])),
        Ty::Struct(ty) => Value::Struct(
            ty.members
                .iter()
                .map(|member| initial(&member.ty))
                .collect(),
        ),
        // A handle holds `null`, which is no constant.
        Ty::Class(_) | Ty::Null | Ty::Unknown => Value::Unknown,
    }
}

/// A member of a struct of type `base`.
pub(crate) struct Member {
    /// Its place among the members.
    pub(crate) index: usize,
    pub(crate) ty: Ty,
    /// For a packed struct, the bit it starts at; a union's members all
    /// start at 0.
    pub(crate) offset: Option<u32>,
}

/// The member `name` of a struct of type `base`.
pub(crate) fn member(base: &Ty, name: &str) -> Option<Member> {
    let (ty, packed) = match base {
        Ty::Packed(Packed {
            shape: Shape::Struct(ty),
            ..
        }) => (ty, true),
        Ty::Struct(ty) => (ty, false),
        _ => return None,
    };
    let index = ty.members.iter().position(|it| &*it.name == name)?;
    let offset = match (packed, ty.union) {
        (false, _) => None,
        (true, true) => Some(0),
        // The first member is the most significant.
        (true, false) => Some(
            ty.members[index + 1..]
                .iter()
                .filter_map(|member| member.ty.width())
                .sum(),
        ),
    };
    Some(Member {
        index,
        ty: ty.members[index].ty.clone(),
        offset,
    })
}

/// The operator a compound assignment applies; `None` for `=` and `<=`.
pub(crate) fn compound(op: SyntaxKind) -> Option<SyntaxKind> {
    Some(match op {
        PLUS_EQ => PLUS,
        MINUS_EQ => MINUS,
        STAR_EQ => STAR,
        SLASH_EQ => SLASH,
        PERCENT_EQ => PERCENT,
        AMP_EQ => AMP,
        PIPE_EQ => PIPE,
        CARET_EQ => CARET,
        LT_LT_EQ => LT_LT,
        GT_GT_EQ => GT_GT,
        LT_LT_LT_EQ => LT_LT_LT,
        GT_GT_GT_EQ => GT_GT_GT,
        _ => return None,
    })
}

/// A binary operator on two operands already sized.
pub(crate) fn apply(op: SyntaxKind, lhs: Value, rhs: Value) -> Value {
    match (lhs, rhs) {
        (Value::Bits(a), Value::Bits(b)) => Value::Bits(match op {
            PLUS => a.add(&b),
            MINUS => a.sub(&b),
            STAR => a.mul(&b),
            SLASH => a.div(&b),
            PERCENT => a.rem(&b),
            AMP => a.and(&b),
            PIPE => a.or(&b),
            CARET => a.xor(&b),
            CARET_TILDE | TILDE_CARET => a.xnor(&b),
            STAR_STAR => a.pow(&b),
            LT_LT | LT_LT_LT => a.shl(&b),
            GT_GT => a.shr(&b, false),
            GT_GT_GT => a.shr(&b, true),
            _ => return Value::Unknown,
        }),
        (Value::Real(a), b) => {
            let b = match b {
                Value::Real(b) => b,
                Value::Bits(b) => b.to_f64(),
                _ => return Value::Unknown,
            };
            Value::Real(match op {
                PLUS => a + b,
                MINUS => a - b,
                STAR => a * b,
                SLASH => a / b,
                STAR_STAR => a.powf(b),
                _ => return Value::Unknown,
            })
        }
        _ => Value::Unknown,
    }
}

/// A comparison of two operands sized alike; `None` if it cannot be made.
fn compare(op: SyntaxKind, lhs: &Value, rhs: &Value) -> Option<Bit> {
    let ordering = |ordering: Option<Ordering>| match ordering {
        None => Bit::X,
        Some(ordering) => Bit::from(match op {
            LT => ordering == Ordering::Less,
            LT_EQ => ordering != Ordering::Greater,
            GT => ordering == Ordering::Greater,
            _ => ordering != Ordering::Less,
        }),
    };
    Some(match (lhs, rhs) {
        (Value::Bits(a), Value::Bits(b)) => match op {
            EQ_EQ => a.logic_eq(b),
            BANG_EQ => not(a.logic_eq(b)),
            EQ_EQ_EQ => Bit::from(a.case_eq(b)),
            BANG_EQ_EQ => Bit::from(!a.case_eq(b)),
            EQ_EQ_QUESTION => a.wildcard_eq(b),
            BANG_EQ_QUESTION => not(a.wildcard_eq(b)),
            _ => ordering(a.compare(b)),
        },
        (Value::Real(a), Value::Real(b)) => match op {
            EQ_EQ | EQ_EQ_EQ => Bit::from(a == b),
            BANG_EQ | BANG_EQ_EQ => Bit::from(a != b),
            _ => ordering(a.partial_cmp(b)),
        },
        (Value::String(a), Value::String(b)) => match op {
            EQ_EQ | EQ_EQ_EQ => Bit::from(a == b),
            BANG_EQ | BANG_EQ_EQ => Bit::from(a != b),
            _ => ordering(Some(a.cmp(b))),
        },
        _ => return None,
    })
}

/// The element type of an unpacked array, however deep; any other type
/// itself.
fn singular(ty: &Ty) -> &Ty {
    match ty {
        Ty::Array(array) => singular(&array.element),
        Ty::Dynamic(dynamic) => singular(&dynamic.element),
        ty => ty,
    }
}

/// Whether `tested` equals `member`, at `ty`, or an element of it if it is
/// an unpacked array. An `x` or `z` in an integral member matches any bit.
fn in_value(tested: &Value, member: Value, ty: &Ty) -> Option<Bit> {
    match member {
        Value::Array(elements) => {
            any((elements.iter()).map(|element| in_value(tested, element.clone(), ty)))
        }
        member => match (tested, operand(member, ty)) {
            (Value::Bits(tested), Value::Bits(member)) => Some(tested.wildcard_eq(&member)),
            (tested, member) => compare(EQ_EQ, tested, &member),
        },
    }
}

/// Whether any of `matches` holds: 1 if one does, `x` if none does but one
/// may, and `None` if none does and one is not known.
fn any(matches: impl IntoIterator<Item = Option<Bit>>) -> Option<Bit> {
    let mut result = Some(Bit::Zero);
    for matched in matches {
        match matched {
            Some(Bit::One) => return Some(Bit::One),
            Some(Bit::Zero) => {}
            None => result = None,
            Some(_) => result = result.map(|_| Bit::X),
        }
    }
    result
}

/// What a value is as a condition; `None` if it is not known.
pub(crate) fn truth(value: &Value) -> Option<Bit> {
    match value {
        Value::Bits(bits) => Some(bits.truth()),
        Value::Real(real) => Some(Bit::from(*real != 0.0)),
        Value::String(bytes) => Some(Bit::from(!bytes.is_empty())),
        Value::Array(_) | Value::Struct(_) | Value::Unknown => None,
    }
}

fn not(bit: Bit) -> Bit {
    match bit {
        Bit::Zero => Bit::One,
        Bit::One => Bit::Zero,
        _ => Bit::X,
    }
}

fn and(a: Bit, b: Bit) -> Bit {
    match (a, b) {
        (Bit::Zero, _) | (_, Bit::Zero) => Bit::Zero,
        (Bit::One, Bit::One) => Bit::One,
        _ => Bit::X,
    }
}

/// A reduction operator over `bits`.
fn reduce(op: SyntaxKind, bits: &Bits) -> Bit {
    match op {
        AMP => bits.reduce_and(),
        PIPE => bits.truth(),
        CARET => bits.reduce_xor(),
        TILDE_AMP => not(bits.reduce_and()),
        TILDE_PIPE => not(bits.truth()),
        _ => not(bits.reduce_xor()),
    }
}

/// A string as bits, eight a character, the first most significant; an
/// empty string is one zero byte.
pub(crate) fn string_bits(bytes: &[u8]) -> Bits {
    if bytes.is_empty() {
        return Bits::from_u64(8, false, 0);
    }
    let parts: Vec<Bits> = (bytes.iter())
        .map(|&byte| Bits::from_u64(8, false, u64::from(byte)))
        .collect();
    Bits::concat(&parts)
}

/// Bits as a string, eight a character, its zero bytes left out.
pub(crate) fn bits_string(bits: &Bits) -> Box<[u8]> {
    let bytes = bits.width().div_ceil(8);
    let wide = bits.resize(bytes * 8);
    (0..bytes)
        .rev()
        .filter_map(|index| wide.slice(i64::from(index) * 8, 8).to_u64())
        .map(|byte| byte as u8)
        .filter(|&byte| byte != 0)
        .collect()
}
