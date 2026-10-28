//! Selects and members: what `a[i]`, `a[hi:lo]`, `a[i+:w]` and `a.m` take
//! of a value, and the places in a constant function's variables that an
//! assignment to one writes.

use std::sync::Arc;

use astli_syntax::SyntaxKind::{self, *};

use super::expr::{ELEMENTS, Operator, apply, compound, initial, member, merge, operand};
use super::{Cx, Queries};
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::{ArrayTy, Packed, Shape, Ty};
use crate::value::{Bit, Bits, Value};

/// What a select takes of a value of some type.
pub(crate) struct Selected {
    pub(crate) ty: Ty,
    pub(crate) part: Part,
}

#[derive(Clone, Copy)]
pub(crate) enum Part {
    /// Bits of a packed value, from `offset` up; no offset when the index
    /// has an unknown bit.
    Bits { offset: Option<i64>, width: u32 },
    /// Elements of an unpacked array, from the `first` in the order stored,
    /// left bound first; one, or a slice of `count`.
    Elements {
        first: Option<i64>,
        count: u64,
        slice: bool,
    },
}

/// One step from a variable to the part of it a place is.
#[derive(Clone, Copy)]
enum Step {
    Bits { offset: i64, width: u32 },
    Element(i64),
    Member(usize),
}

/// Where an assignment in a constant function writes: one of its variables,
/// or a part of one, and the type of what it writes.
pub(crate) struct Place {
    symbol: SymbolId,
    steps: Vec<Step>,
    pub(crate) ty: Ty,
}

impl Queries {
    /// Where in its variable `expr` is, as [`reach`](crate::Elaboration::reach)
    /// counts: its first bit, its bits, and its type.
    pub(crate) fn reached(&self, cx: Cx, expr: ExprId) -> Option<(u64, u64, Ty)> {
        let hir = &self.design[cx.file()];
        match &hir[expr].kind {
            ExprKind::Name(_) | ExprKind::Scoped { .. } => {
                let ty = self.self_type(cx, expr);
                Some((0, ty.bits()?, ty))
            }
            ExprKind::Select { base, index, range } => {
                let (offset, bits, ty) = self.reached(cx, *base)?;
                let selected = self.selected(cx, &ty, *index, *range)?;
                let (first, width) = match selected.part {
                    Part::Bits {
                        offset: Some(first),
                        width,
                    } => (u64::try_from(first).ok()?, u64::from(width)),
                    Part::Elements {
                        first: Some(first),
                        count,
                        ..
                    } => {
                        let Ty::Array(array) = &ty else {
                            return None;
                        };
                        let element = array.element.bits()?;
                        (u64::try_from(first).ok()? * element, count * element)
                    }
                    _ => return None,
                };
                // A select past the bounds names nothing of the variable.
                (first + width <= bits).then_some((offset + first, width, selected.ty))
            }
            ExprKind::Member { base, name } => {
                let (offset, _, ty) = self.reached(cx, *base)?;
                let found = member(&ty, &name.text)?;
                let first = match (found.offset, &ty) {
                    (Some(first), _) => u64::from(first),
                    (None, Ty::Struct(st)) if st.union => 0,
                    (None, Ty::Struct(st)) => (st.members[..found.index].iter())
                        .map(|member| member.ty.bits())
                        .sum::<Option<u64>>()?,
                    _ => return None,
                };
                Some((offset + first, found.ty.bits()?, found.ty))
            }
            _ => None,
        }
    }

    /// What `[index]`, or `[index op end]` with a range, takes of a value of
    /// type `base`.
    pub(crate) fn selected(
        &self,
        cx: Cx,
        base: &Ty,
        index: ExprId,
        range: Option<(SyntaxKind, ExprId)>,
    ) -> Option<Selected> {
        let (left, right) = match base {
            Ty::Packed(packed) => packed.range()?,
            Ty::Array(array) => (array.left, array.right),
            _ => return None,
        };
        let index = match self.eval_self(cx, index) {
            Value::Bits(bits) => bits.to_i64(),
            _ => return None,
        };
        // The indices taken, lowest first; none where the index is unknown.
        let (low, count) = match range {
            None => (index, 1),
            Some((COLON, end)) => {
                let (index, end) = (index?, self.integer(cx, end)?);
                (Some(index.min(end)), index.abs_diff(end) + 1)
            }
            Some((op, width)) => {
                let width = self.integer(cx, width).filter(|&width| width > 0)?;
                let low = match op {
                    PLUS_COLON => index,
                    _ => index.map(|index| index - (width - 1)),
                };
                (low, width as u64)
            }
        };
        let high = low.map(|low| low + (count as i64 - 1));
        let descending = left >= right;
        match base {
            Ty::Packed(packed) => {
                let element = packed.element();
                // The least significant index is the one nearest `right`.
                let offset = match descending {
                    true => low.map(|low| low - right),
                    false => high.map(|high| right - high),
                };
                let width = u64::from(element.width) * count;
                let width = u32::try_from(width)
                    .ok()
                    .filter(|&width| width <= Bits::MAX_WIDTH)?;
                let ty = match range {
                    None => element.clone(),
                    Some(_) => slice(&element, count),
                };
                Some(Selected {
                    ty: Ty::Packed(ty),
                    part: Part::Bits {
                        offset: offset.map(|offset| offset * i64::from(element.width)),
                        width,
                    },
                })
            }
            Ty::Array(array) => {
                let first = match descending {
                    true => high.map(|high| left - high),
                    false => low.map(|low| low - left),
                };
                let ty = match range {
                    None => array.element.clone(),
                    Some(_) => Ty::Array(Arc::new(ArrayTy {
                        left: 0,
                        right: count as i64 - 1,
                        element: array.element.clone(),
                    })),
                };
                Some(Selected {
                    ty,
                    part: Part::Elements {
                        first,
                        count,
                        slice: range.is_some(),
                    },
                })
            }
            _ => None,
        }
    }

    pub(crate) fn select(
        &self,
        cx: Cx,
        base: ExprId,
        index: ExprId,
        range: Option<(SyntaxKind, ExprId)>,
    ) -> Value {
        let value = self.eval_self(cx, base);
        let ty = match self.self_type(cx, base) {
            // Indexed by its value's size, which its type does not know.
            Ty::Dynamic(dynamic) => match &value {
                Value::Array(values) if values.is_empty() => {
                    return match range {
                        None => initial(&dynamic.element),
                        Some(_) => Value::Unknown,
                    };
                }
                Value::Array(values) => Ty::Array(Arc::new(dynamic.sized(values.len()))),
                _ => return Value::Unknown,
            },
            ty => ty,
        };
        let Some(selected) = self.selected(cx, &ty, index, range) else {
            return Value::Unknown;
        };
        take(value, &selected)
    }

    pub(crate) fn member_value(&self, cx: Cx, base: ExprId, name: &str) -> Value {
        let ty = self.self_type(cx, base);
        let Some(member) = member(&ty, name) else {
            return Value::Unknown;
        };
        match (self.eval_self(cx, base), member.offset, &member.ty) {
            (Value::Bits(bits), Some(offset), Ty::Packed(packed)) => Value::Bits(
                bits.slice(i64::from(offset), packed.width)
                    .with_signed(packed.signed),
            ),
            (Value::Struct(values), None, _) => values[member.index].clone(),
            _ => Value::Unknown,
        }
    }

    // -------------------------------------------------------------- places

    /// Where `expr`, a variable of the running constant function or a select
    /// or member of one, writes.
    pub(crate) fn place(&self, cx: Cx, expr: ExprId) -> Option<Place> {
        let hir = &self.design[cx.file];
        let frame = cx.frame?;
        match &hir[expr].kind {
            ExprKind::Name(_) => match self.resolution(cx, expr) {
                Some(Resolution::Declared(at))
                    if at.file == cx.file && frame.locals.borrow().contains_key(&at.symbol) =>
                {
                    Some(Place {
                        symbol: at.symbol,
                        steps: Vec::new(),
                        ty: self.type_in(cx, at),
                    })
                }
                _ => None,
            },
            ExprKind::Select { base, index, range } => {
                let mut place = self.place(cx, *base)?;
                let selected = self.selected(cx, &place.ty, *index, *range)?;
                let step = match selected.part {
                    Part::Bits {
                        offset: Some(offset),
                        width,
                    } => Step::Bits { offset, width },
                    Part::Elements {
                        first: Some(first),
                        slice: false,
                        ..
                    } => Step::Element(first),
                    _ => return None,
                };
                place.steps.push(step);
                place.ty = selected.ty;
                Some(place)
            }
            ExprKind::Member { base, name } => {
                let mut place = self.place(cx, *base)?;
                let member = member(&place.ty, &name.text)?;
                let step = match (member.offset, &member.ty) {
                    (Some(offset), Ty::Packed(packed)) => Step::Bits {
                        offset: i64::from(offset),
                        width: packed.width,
                    },
                    (None, _) => Step::Member(member.index),
                    _ => return None,
                };
                place.steps.push(step);
                place.ty = member.ty;
                Some(place)
            }
            _ => None,
        }
    }

    /// What `place` holds.
    pub(crate) fn read(&self, cx: Cx, place: &Place) -> Value {
        let Some(frame) = cx.frame else {
            return Value::Unknown;
        };
        let Some(mut value) = frame.locals.borrow().get(&place.symbol).cloned() else {
            return Value::Unknown;
        };
        for step in &place.steps {
            value = match (*step, value) {
                (Step::Bits { offset, width }, Value::Bits(bits)) => {
                    Value::Bits(bits.slice(offset, width))
                }
                (Step::Element(index), Value::Array(values)) => usize::try_from(index)
                    .ok()
                    .and_then(|index| values.get(index).cloned())
                    .unwrap_or(Value::Unknown),
                (Step::Member(index), Value::Struct(values)) => values[index].clone(),
                _ => Value::Unknown,
            };
        }
        match (value, &place.ty) {
            (Value::Bits(bits), Ty::Packed(ty)) => Value::Bits(bits.with_signed(ty.signed)),
            (value, _) => value,
        }
    }

    /// Writes `value`, of `place`'s type, to it.
    pub(crate) fn write(&self, cx: Cx, place: &Place, value: Value) {
        let Some(frame) = cx.frame else {
            return;
        };
        let mut locals = frame.locals.borrow_mut();
        if let Some(slot) = locals.get_mut(&place.symbol) {
            put(slot, &place.steps, value);
        }
    }

    /// `lhs = rhs`, or `lhs op= rhs`, run for its effect; `None` if `lhs`
    /// is nothing a constant function may write.
    pub(crate) fn assign(&self, cx: Cx, op: SyntaxKind, lhs: ExprId, rhs: ExprId) -> Option<Value> {
        let place = self.place(cx, lhs)?;
        let value = match compound(op) {
            None => self.eval_assign(cx, rhs, &place.ty),
            Some(op) => {
                let current = self.read(cx, &place);
                let context = Operator::of(op) == Some(Operator::Context);
                let ty = match context {
                    true => merge(&place.ty, &self.self_type(cx, rhs)),
                    false => place.ty.clone(),
                };
                let rhs = match context {
                    true => self.eval_at(cx, rhs, &ty),
                    false => self.eval_self(cx, rhs),
                };
                self.convert(apply(op, operand(current, &ty), rhs), &place.ty)
            }
        };
        self.write(cx, &place, value.clone());
        Some(value)
    }

    /// `++x`, `x++`, `--x` or `x--`: the value after it when `prefix`,
    /// before it otherwise.
    pub(crate) fn step(&self, cx: Cx, operand: ExprId, op: SyntaxKind, prefix: bool) -> Value {
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
}

/// A slice of `count` elements of type `element`, indexed from 0.
fn slice(element: &Packed, count: u64) -> Packed {
    let width = (u64::from(element.width) * count) as u32;
    let right = 0;
    let left = count as i64 - 1;
    let shape = match element.shape {
        Shape::Scalar => Shape::Vector { left, right },
        _ => Shape::Array {
            left,
            right,
            element: Arc::new(element.clone()),
        },
    };
    Packed {
        width,
        signed: false,
        four_state: element.four_state,
        shape,
    }
}

/// What `selected` takes of `value`. An index outside an unpacked array
/// reads the value its element type starts with, and one with an unknown
/// bit reads `x`.
fn take(value: Value, selected: &Selected) -> Value {
    match (value, selected.part) {
        (Value::Bits(bits), Part::Bits { offset, width }) => {
            let signed = matches!(&selected.ty, Ty::Packed(packed) if packed.signed);
            match offset {
                Some(offset) => Value::Bits(bits.slice(offset, width).with_signed(signed)),
                None => Value::Bits(Bits::filled(width, false, Bit::X)),
            }
        }
        (
            Value::Array(values),
            Part::Elements {
                first,
                count,
                slice,
            },
        ) => {
            let element = match &selected.ty {
                Ty::Array(array) if slice => &array.element,
                ty => ty,
            };
            let at = |position: i64| {
                usize::try_from(position)
                    .ok()
                    .and_then(|position| values.get(position).cloned())
                    .unwrap_or_else(|| initial(element))
            };
            match (first, slice) {
                (None, false) => initial(element),
                (Some(first), false) => at(first),
                (None, true) => Value::Unknown,
                (Some(first), true) if count <= ELEMENTS => {
                    Value::Array((0..count as i64).map(|k| at(first + k)).collect())
                }
                (Some(_), true) => Value::Unknown,
            }
        }
        _ => Value::Unknown,
    }
}

/// Writes `value` to the part of `slot` that `steps` lead to.
fn put(slot: &mut Value, steps: &[Step], value: Value) {
    let Some((step, rest)) = steps.split_first() else {
        *slot = value;
        return;
    };
    match (*step, &mut *slot) {
        (Step::Bits { offset, width }, Value::Bits(bits)) => {
            let mut part = Value::Bits(bits.slice(offset, width));
            put(&mut part, rest, value);
            match part {
                Value::Bits(part) => bits.write(offset, &part),
                _ => *slot = Value::Unknown,
            }
        }
        (Step::Element(index), Value::Array(values)) => {
            let mut values = values.to_vec();
            if let Some(element) = usize::try_from(index)
                .ok()
                .and_then(|index| values.get_mut(index))
            {
                put(element, rest, value);
            }
            *slot = Value::Array(values.into());
        }
        (Step::Member(index), Value::Struct(values)) => {
            let mut values = values.to_vec();
            put(&mut values[index], rest, value);
            *slot = Value::Struct(values.into());
        }
        _ => *slot = Value::Unknown,
    }
}
