//! Expressions: the type each has by itself, and its value once the
//! expression around it has sized it.
//!
//! An operator whose operands are context-determined (`+`, `&`, `?:`'s arms)
//! evaluates them at the width of the whole expression, and signed only if
//! every one of them is; one whose operands are self-determined (`==`, `&&`,
//! a shift's amount, a concatenation) evaluates each at its own type and
//! yields a type of its own.

use std::cmp::Ordering;

use astli_syntax::SyntaxKind::{self, *};

use super::{Cx, Elaboration, builtin};
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::{Integral, Ty};
use crate::value::{Bit, Bits, Literal, Value, literal};

impl Elaboration<'_> {
    /// `expr`'s type by itself, before the expression around it sizes it.
    pub(crate) fn self_type(&self, cx: Cx, expr: ExprId) -> Ty {
        let hir = &self.design[cx.file];
        match &hir[expr].kind {
            ExprKind::Literal(text) => match literal(text) {
                Literal::Bits(bits) => {
                    Ty::Integral(Integral::vector(bits.width(), bits.is_signed(), true))
                }
                Literal::Fill(_) => Ty::BIT,
                Literal::Real(_) => Ty::Real,
                Literal::String(bytes) => {
                    let width = (bytes.len().max(1) * 8) as u32;
                    Ty::Integral(Integral::vector(width, false, false))
                }
                Literal::Unknown => Ty::Unknown,
            },
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.resolution(cx, expr) {
                Some(Resolution::Declared(at)) => self.symbol_type(cx, at),
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
                    merge(self.self_type(cx, *lhs), self.self_type(cx, *rhs))
                }
                Some(Operator::Power) => match self.self_type(cx, *rhs) {
                    Ty::Real => merge(self.self_type(cx, *lhs), Ty::Real),
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
            } => merge(
                self.self_type(cx, *then_value),
                self.self_type(cx, *else_value),
            ),
            ExprKind::Select { base, index, range } => {
                let Some(base) = self.self_type(cx, *base).integral() else {
                    return Ty::Unknown;
                };
                match self.part(cx, base, *index, *range) {
                    Some(Part { width, .. }) => {
                        let mut ty = Integral::vector(width, false, base.four_state);
                        if range.is_none() {
                            ty.range = Some((0, 0));
                        }
                        Ty::Integral(ty)
                    }
                    None => Ty::Unknown,
                }
            }
            ExprKind::Call { callee, args } => self.call_type(cx, *callee, args),
            ExprKind::Concat(parts) => self.concat_type(cx, parts),
            ExprKind::Replication { count, concat } => {
                match (self.count(cx, *count), self.self_type(cx, *concat)) {
                    (Some(count), Ty::Integral(each)) => {
                        let width = u64::from(each.width) * count;
                        match width {
                            1.. if width <= u64::from(Bits::MAX_WIDTH) => {
                                Ty::Integral(Integral::vector(width as u32, false, each.four_state))
                            }
                            _ => Ty::Unknown,
                        }
                    }
                    _ => Ty::Unknown,
                }
            }
            ExprKind::Cast { ty, operand } => match self.cast_target(cx, *ty, *operand) {
                Cast::Signed(signed) => match self.self_type(cx, *operand) {
                    Ty::Integral(own) => Ty::Integral(Integral { signed, ..own }),
                    _ => Ty::Unknown,
                },
                Cast::To(ty) => ty,
            },
            ExprKind::Inside { .. } => Ty::BIT,
            ExprKind::Assign { lhs, .. } => self.self_type(cx, *lhs),
            _ => Ty::Unknown,
        }
    }

    /// `expr` evaluated at its own type.
    pub(crate) fn eval_self(&self, cx: Cx, expr: ExprId) -> Value {
        let ty = self.self_type(cx, expr);
        self.eval_at(cx, expr, ty)
    }

    /// `expr` assigned to a declaration of type `target`: evaluated at the
    /// wider of the two, then converted.
    pub(crate) fn eval_assign(&self, cx: Cx, expr: ExprId, target: Ty) -> Value {
        let own = self.self_type(cx, expr);
        let value = match (target, own) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => return Value::Unknown,
            (Ty::Integral(target), Ty::Integral(own)) => {
                let width = target.width.max(own.width);
                let ty = Integral::vector(width, own.signed, own.four_state);
                self.eval_at(cx, expr, Ty::Integral(ty))
            }
            (_, own) => self.eval_at(cx, expr, own),
        };
        self.convert(value, target)
    }

    /// `expr` evaluated where the expression around it has type `ty`: at
    /// least as wide as its own, and signed only if the expression is.
    pub(crate) fn eval_at(&self, cx: Cx, expr: ExprId, ty: Ty) -> Value {
        if ty == Ty::Unknown {
            return Value::Unknown;
        }
        let hir = &self.design[cx.file];
        let value = match &hir[expr].kind {
            ExprKind::Literal(text) => match literal(text) {
                Literal::Bits(bits) => Value::Bits(bits),
                Literal::Fill(bit) => match ty {
                    Ty::Integral(ty) => return Value::Bits(Bits::filled(ty.width, ty.signed, bit)),
                    _ => Value::Bits(Bits::bit_value(bit)),
                },
                Literal::Real(real) => Value::Real(real),
                Literal::String(bytes) if ty == Ty::String => Value::String(bytes),
                Literal::String(bytes) => Value::Bits(string_bits(&bytes)),
                Literal::Unknown => Value::Unknown,
            },
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
                Cast::To(target) => self.eval_assign(cx, *operand, target),
            },
            ExprKind::Assign { op, lhs, rhs, .. } => match self.assign(cx, *op, *lhs, *rhs) {
                Some(value) => value,
                None => Value::Unknown,
            },
            _ => Value::Unknown,
        };
        operand(value, ty)
    }

    fn binary(&self, cx: Cx, op: SyntaxKind, lhs: ExprId, rhs: ExprId, ty: Ty) -> Value {
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
                let common = merge(self.self_type(cx, lhs), self.self_type(cx, rhs));
                let lhs = self.eval_at(cx, lhs, common);
                let rhs = self.eval_at(cx, rhs, common);
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
                            (Bit::Zero | Bit::One, Bit::Zero | Bit::One) => {
                                if a == b {
                                    Bit::One
                                } else {
                                    Bit::Zero
                                }
                            }
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
    /// extended to it, `x` and `z` dropped by a two-state one.
    pub(crate) fn convert(&self, value: Value, target: Ty) -> Value {
        match (value, target) {
            (Value::Bits(bits), Ty::Integral(ty)) => {
                let bits = bits.resize(ty.width).with_signed(ty.signed);
                Value::Bits(if ty.four_state {
                    bits
                } else {
                    bits.to_two_state()
                })
            }
            (Value::Real(real), Ty::Integral(ty)) => {
                Value::Bits(Bits::from_f64(ty.width, ty.signed, real))
            }
            (Value::String(bytes), Ty::Integral(ty)) => {
                Value::Bits(string_bits(&bytes).resize(ty.width).with_signed(ty.signed))
            }
            (Value::Bits(bits), Ty::Real) => Value::Real(bits.to_f64()),
            (Value::Real(real), Ty::Real) => Value::Real(real),
            (Value::Bits(bits), Ty::String) => Value::String(bits_string(&bits)),
            (Value::String(bytes), Ty::String) => Value::String(bytes),
            _ => Value::Unknown,
        }
    }

    // ------------------------------------------------------------- selects

    /// Which bits of a vector of type `base` the select `[index]`,
    /// `[hi:lo]`, `[i+:w]` or `[i-:w]` takes.
    fn part(
        &self,
        cx: Cx,
        base: Integral,
        index: ExprId,
        range: Option<(SyntaxKind, ExprId)>,
    ) -> Option<Part> {
        let (left, right) = base.range?;
        let descending = left >= right;
        // The bit an index names, counted from the least significant.
        let offset = |index: i64| {
            if descending {
                index - right
            } else {
                right - index
            }
        };
        let index = match self.eval_self(cx, index) {
            Value::Bits(bits) => bits.to_i64(),
            _ => return None,
        };
        let (lsb, width) = match range {
            None => (index, 1),
            Some((COLON, end)) => {
                let end = self.integer(cx, end)?;
                (Some(end), index?.abs_diff(end) + 1)
            }
            Some((op, width)) => {
                let width = self.integer(cx, width).filter(|&width| width > 0)?;
                // `+:` counts up from the index and `-:` down, whichever
                // way the vector is numbered.
                let lsb = index.map(|index| match (op == PLUS_COLON, descending) {
                    (true, true) | (false, false) => index,
                    (false, true) => index - (width - 1),
                    (true, false) => index + (width - 1),
                });
                (lsb, width as u64)
            }
        };
        let width = u32::try_from(width)
            .ok()
            .filter(|&w| w <= Bits::MAX_WIDTH)?;
        Some(Part {
            offset: lsb.map(offset),
            width,
        })
    }

    fn select(
        &self,
        cx: Cx,
        base: ExprId,
        index: ExprId,
        range: Option<(SyntaxKind, ExprId)>,
    ) -> Value {
        let Some(ty) = self.self_type(cx, base).integral() else {
            return Value::Unknown;
        };
        let Some(part) = self.part(cx, ty, index, range) else {
            return Value::Unknown;
        };
        match (self.eval_self(cx, base), part.offset) {
            (Value::Bits(bits), Some(offset)) => Value::Bits(bits.slice(offset, part.width)),
            // An index with an unknown bit selects an unknown value.
            (Value::Bits(_), None) => Value::Bits(Bits::filled(part.width, false, Bit::X)),
            _ => Value::Unknown,
        }
    }

    /// Where `expr`, a variable or a select of one, writes, and the type of
    /// what it writes.
    pub(crate) fn place(&self, cx: Cx, expr: ExprId) -> Option<Place> {
        let hir = &self.design[cx.file];
        let frame = cx.frame?;
        let local = |expr: ExprId| match self.resolution(cx, expr) {
            Some(Resolution::Declared(at))
                if at.file == cx.file && frame.locals.borrow().contains_key(&at.symbol) =>
            {
                Some(at)
            }
            _ => None,
        };
        match &hir[expr].kind {
            ExprKind::Name(_) => {
                let at = local(expr)?;
                Some(Place {
                    symbol: at.symbol,
                    part: None,
                    ty: self.symbol_type(cx, at),
                })
            }
            ExprKind::Select { base, index, range } => {
                let at = local(*base)?;
                let whole = self.symbol_type(cx, at).integral()?;
                let part = self.part(cx, whole, *index, *range)?;
                let ty = Integral::vector(part.width, false, whole.four_state);
                Some(Place {
                    symbol: at.symbol,
                    part: Some(part.offset?),
                    ty: Ty::Integral(ty),
                })
            }
            _ => None,
        }
    }

    /// What `place` holds.
    pub(crate) fn read(&self, cx: Cx, place: &Place) -> Value {
        let Some(frame) = cx.frame else {
            return Value::Unknown;
        };
        let value = frame.locals.borrow().get(&place.symbol).cloned();
        match (value, place.part, place.ty) {
            (Some(value), None, _) => value,
            (Some(Value::Bits(bits)), Some(offset), Ty::Integral(ty)) => {
                Value::Bits(bits.slice(offset, ty.width))
            }
            _ => Value::Unknown,
        }
    }

    /// Writes `value`, of `place`'s type, to it.
    pub(crate) fn write(&self, cx: Cx, place: &Place, value: Value) {
        let Some(frame) = cx.frame else {
            return;
        };
        let mut locals = frame.locals.borrow_mut();
        let Some(slot) = locals.get_mut(&place.symbol) else {
            return;
        };
        match (place.part, slot, value) {
            (None, slot, value) => *slot = value,
            (Some(offset), Value::Bits(bits), Value::Bits(part)) => bits.write(offset, &part),
            (Some(_), slot, _) => *slot = Value::Unknown,
        }
    }

    /// `lhs = rhs`, or `lhs op= rhs`, run for its effect; `None` if `lhs`
    /// is nothing a constant function may write.
    pub(crate) fn assign(&self, cx: Cx, op: SyntaxKind, lhs: ExprId, rhs: ExprId) -> Option<Value> {
        let place = self.place(cx, lhs)?;
        let value = match compound(op) {
            None => self.eval_assign(cx, rhs, place.ty),
            Some(op) => {
                let current = self.read(cx, &place);
                let ty = match Operator::of(op) {
                    Some(Operator::Context) => merge(place.ty, self.self_type(cx, rhs)),
                    _ => place.ty,
                };
                let rhs = match Operator::of(op) {
                    Some(Operator::Context) => self.eval_at(cx, rhs, ty),
                    _ => self.eval_self(cx, rhs),
                };
                self.convert(apply(op, operand(current, ty), rhs), place.ty)
            }
        };
        self.write(cx, &place, value.clone());
        Some(value)
    }

    /// `++x`, `x++`, `--x` or `x--`: the value after it when `prefix`,
    /// before it otherwise.
    fn step(&self, cx: Cx, operand: ExprId, op: SyntaxKind, prefix: bool) -> Value {
        let Some(place) = self.place(cx, operand) else {
            return Value::Unknown;
        };
        let before = self.read(cx, &place);
        let after = match &before {
            Value::Bits(bits) => {
                let one = Bits::from_u64(bits.width(), bits.is_signed(), 1);
                Value::Bits(if op == PLUS_PLUS {
                    bits.add(&one)
                } else {
                    bits.sub(&one)
                })
            }
            _ => Value::Unknown,
        };
        self.write(cx, &place, after.clone());
        if prefix { after } else { before }
    }

    // ------------------------------------------------------ concatenations

    /// A replication's count, if it is a constant that is not negative.
    fn count(&self, cx: Cx, count: ExprId) -> Option<u64> {
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
                Ty::Integral(part) => {
                    width += u64::from(part.width);
                    four_state |= part.four_state;
                }
                Ty::String => return Ty::String,
                _ => return Ty::Unknown,
            }
        }
        match width {
            1.. if width <= u64::from(Bits::MAX_WIDTH) => {
                Ty::Integral(Integral::vector(width as u32, false, four_state))
            }
            _ => Ty::Unknown,
        }
    }

    fn concat(&self, cx: Cx, parts: &[ExprId]) -> Value {
        if self.concat_type(cx, parts) == Ty::String {
            let mut bytes = Vec::new();
            for &part in parts {
                match self.eval_at(cx, part, Ty::String) {
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
                    SymbolKind::Typedef(_) => Cast::To(self.symbol_type(cx, at)),
                    SymbolKind::Parameter(parameter) if !parameter.is_type => {
                        self.width_cast(cx, target, operand)
                    }
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
            (Some(width @ 1..), Ty::Integral(own)) if width <= Bits::MAX_WIDTH => Cast::To(
                Ty::Integral(Integral::vector(width, own.signed, own.four_state)),
            ),
            _ => Cast::To(Ty::Unknown),
        }
    }
}

/// The bits a select takes: from `offset` up, counted from the least
/// significant; no offset when the index has an unknown bit.
#[derive(Clone, Copy)]
struct Part {
    offset: Option<i64>,
    width: u32,
}

/// Where an assignment in a constant function writes: one of its variables,
/// whole or a part from a bit up.
pub(crate) struct Place {
    symbol: SymbolId,
    part: Option<i64>,
    ty: Ty,
}

enum Cast {
    /// `signed'` or `unsigned'`, which changes only the signing.
    Signed(bool),
    /// To a type, as an assignment converts.
    To(Ty),
}

/// How a binary operator sizes its operands.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
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
    fn of(op: SyntaxKind) -> Option<Operator> {
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

/// The operator a compound assignment applies; `None` for `=` and `<=`.
fn compound(op: SyntaxKind) -> Option<SyntaxKind> {
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

/// The type of an expression whose operands have types `a` and `b`, each
/// extended to it: real if either is, as wide as the wider, signed if both
/// are.
pub(crate) fn merge(a: Ty, b: Ty) -> Ty {
    match (a, b) {
        (Ty::Integral(a), Ty::Integral(b)) => Ty::Integral(Integral::vector(
            a.width.max(b.width),
            a.signed && b.signed,
            a.four_state || b.four_state,
        )),
        (Ty::Real, Ty::Real | Ty::Integral(_)) | (Ty::Integral(_), Ty::Real) => Ty::Real,
        (Ty::String, Ty::String) => Ty::String,
        _ => Ty::Unknown,
    }
}

/// A value evaluated at its own type, taken to the type `ty` of the
/// expression around it: extended by its sign only if that is signed.
pub(crate) fn operand(value: Value, ty: Ty) -> Value {
    match (value, ty) {
        (Value::Bits(bits), Ty::Integral(ty)) => {
            Value::Bits(bits.with_signed(ty.signed).resize(ty.width))
        }
        (Value::Bits(bits), Ty::Real) => Value::Real(bits.to_f64()),
        (Value::Real(real), Ty::Integral(ty)) => {
            Value::Bits(Bits::from_f64(ty.width, ty.signed, real))
        }
        (Value::Real(real), Ty::Real) => Value::Real(real),
        (Value::String(bytes), Ty::String) => Value::String(bytes),
        (Value::String(bytes), Ty::Integral(ty)) => {
            Value::Bits(string_bits(&bytes).with_signed(ty.signed).resize(ty.width))
        }
        (Value::Bits(bits), Ty::String) => Value::String(bits_string(&bits)),
        _ => Value::Unknown,
    }
}

/// A binary operator on two operands already sized.
fn apply(op: SyntaxKind, lhs: Value, rhs: Value) -> Value {
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

/// What a value is as a condition; `None` if it is not known.
pub(crate) fn truth(value: &Value) -> Option<Bit> {
    match value {
        Value::Bits(bits) => Some(bits.truth()),
        Value::Real(real) => Some(Bit::from(*real != 0.0)),
        Value::String(bytes) => Some(Bit::from(!bytes.is_empty())),
        Value::Unknown => None,
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
fn bits_string(bits: &Bits) -> Box<[u8]> {
    let bytes = bits.width().div_ceil(8);
    let wide = bits.resize(bytes * 8);
    (0..bytes)
        .rev()
        .filter_map(|index| wide.slice(i64::from(index) * 8, 8).to_u64())
        .map(|byte| byte as u8)
        .filter(|&byte| byte != 0)
        .collect()
}
