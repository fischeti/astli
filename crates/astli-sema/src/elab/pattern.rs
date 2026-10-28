//! Assignment patterns, `'{…}`: a value for each member of a struct or
//! element of an array, by position, by name or index, by type, or by
//! default; and concatenations assigned to an unpacked array.

use std::sync::Arc;

use astli_syntax::SyntaxKind::*;

use super::expr::ELEMENTS;
use super::{Cx, Queries};
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::{ArrayTy, DynamicTy, Packed, Shape, StructTy, Ty};
use crate::value::{Bits, Value};

/// What a pattern fills: one slot per member or element, and what the
/// slots' values make together.
struct Slots {
    tys: Vec<Ty>,
    /// A struct's member names; empty for an array.
    names: Vec<Box<str>>,
    /// An array's bounds, which an index key counts from.
    bounds: Option<(i64, i64)>,
    whole: Whole,
}

enum Whole {
    /// Bits side by side, the first slot most significant.
    Packed {
        signed: bool,
    },
    Array,
    Struct,
}

impl Queries {
    /// The value of the pattern `items` as a value of type `target`, each
    /// slot it leaves out given `inherited`, a default from a pattern around
    /// it.
    pub(crate) fn pattern(
        &self,
        cx: Cx,
        items: &[PatternItem],
        inherited: Option<ExprId>,
        target: &Ty,
    ) -> Value {
        // A pattern for a dynamic array or a queue lists its elements, as
        // many as it has.
        if let Ty::Dynamic(dynamic) = target {
            if items.iter().any(|item| item.key.is_some()) {
                return Value::Unknown;
            }
            let positional = items.iter().map(|item| item.value).collect();
            return match self.replicated(cx, positional) {
                Some(values) if !values.is_empty() => {
                    let fixed = Ty::Array(Arc::new(dynamic.sized(values.len())));
                    self.pattern(cx, items, inherited, &fixed)
                }
                _ => Value::Unknown,
            };
        }
        let Some(slots) = slots(target) else {
            return Value::Unknown;
        };
        let Some(values) = self.fill(cx, items, inherited, &slots) else {
            return Value::Unknown;
        };
        match slots.whole {
            Whole::Packed { signed } => {
                let mut parts = Vec::new();
                for value in values {
                    match value {
                        Value::Bits(bits) => parts.push(bits),
                        _ => return Value::Unknown,
                    }
                }
                Value::Bits(Bits::concat(&parts).with_signed(signed))
            }
            Whole::Array => Value::Array(values.into()),
            Whole::Struct => Value::Struct(values.into()),
        }
    }

    /// The value of each slot.
    fn fill(
        &self,
        cx: Cx,
        items: &[PatternItem],
        inherited: Option<ExprId>,
        slots: &Slots,
    ) -> Option<Vec<Value>> {
        let hir = &self.design[cx.file];
        let positional: Vec<ExprId> = (items.iter())
            .filter(|item| item.key.is_none())
            .map(|item| item.value)
            .collect();
        if !positional.is_empty() {
            if positional.len() != items.len() {
                return None;
            }
            let positional = self.replicated(cx, positional)?;
            if positional.len() != slots.tys.len() {
                return None;
            }
            let values = positional.iter().zip(&slots.tys);
            return Some(
                values
                    .map(|(&value, ty)| self.eval_assign(cx, value, ty))
                    .collect(),
            );
        }

        let mut values: Vec<Option<Value>> = vec![None; slots.tys.len()];
        let mut by_type: Vec<(Ty, ExprId)> = Vec::new();
        let mut default = inherited;
        for item in items {
            let key = item.key.expect("positional items were handled");
            if let ExprKind::Keyword(DEFAULT_KW) = hir[key].kind {
                default = Some(item.value);
                continue;
            }
            // A member's name first, then an index, then a type.
            let named = match &hir[key].kind {
                ExprKind::Name(name) => slots.names.iter().position(|it| it == name),
                _ => None,
            };
            let indexed = match (named, slots.bounds) {
                (None, Some((left, right))) if self.is_value(cx, key) => {
                    let index = self.integer(cx, key)?;
                    let position = match left <= right {
                        true => index - left,
                        false => left - index,
                    };
                    let position = usize::try_from(position).ok()?;
                    Some(position).filter(|&position| position < values.len())
                }
                _ => named,
            };
            match indexed {
                Some(slot) => {
                    values[slot] = Some(self.eval_assign(cx, item.value, &slots.tys[slot]))
                }
                // A value that names no member and indexes nothing is an
                // error the pattern's compiler reports.
                None if self.is_value(cx, key) => return None,
                None => match self.type_or_expr(cx, key) {
                    Ty::Unknown => return None,
                    ty => by_type.push((ty, item.value)),
                },
            }
        }
        let filled = values.into_iter().zip(&slots.tys).map(|(value, ty)| {
            if let Some(value) = value {
                return Some(value);
            }
            // The last key of a type wins, as a later item overrides.
            if let Some((_, value)) = by_type.iter().rev().find(|(it, _)| it == ty) {
                return Some(self.eval_assign(cx, *value, ty));
            }
            Some(self.defaulted(cx, default?, ty))
        });
        filled.collect()
    }

    /// `default: value` given to a slot of type `ty`: assigned to it if it
    /// is a vector or of the value's own type, and to each of its members or
    /// elements otherwise.
    fn defaulted(&self, cx: Cx, value: ExprId, ty: &Ty) -> Value {
        let aggregate = match ty {
            Ty::Packed(packed) => matches!(packed.shape, Shape::Array { .. } | Shape::Struct(_)),
            Ty::Array(_) | Ty::Struct(_) => true,
            _ => false,
        };
        match aggregate && self.self_type(cx, value) != *ty {
            true => self.pattern(cx, &[], Some(value), ty),
            false => self.eval_assign(cx, value, ty),
        }
    }

    /// Whether `expr` is a value rather than a type.
    fn is_value(&self, cx: Cx, expr: ExprId) -> bool {
        let hir = &self.design[cx.file];
        match &hir[expr].kind {
            ExprKind::Type(_) | ExprKind::Keyword(_) => false,
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.resolution(cx, expr) {
                Some(Resolution::Declared(at)) => !matches!(
                    &self.design.symbol(at).kind,
                    SymbolKind::Typedef(_) | SymbolKind::Parameter(Parameter { is_type: true, .. })
                ),
                _ => true,
            },
            _ => true,
        }
    }

    /// The items of `'{n{a, b}}` repeated, or `items` as they are.
    fn replicated(&self, cx: Cx, items: Vec<ExprId>) -> Option<Vec<ExprId>> {
        let hir = &self.design[cx.file];
        let [single] = items.as_slice() else {
            return Some(items);
        };
        let ExprKind::Replication { count, concat } = hir[*single].kind else {
            return Some(items);
        };
        let ExprKind::Concat(parts) = &hir[concat].kind else {
            return None;
        };
        let count = usize::try_from(self.count(cx, count)?).ok()?;
        if count.checked_mul(parts.len())? > ELEMENTS as usize {
            return None;
        }
        Some(parts.repeat(count))
    }

    /// `{a, b}` assigned to a dynamic array or a queue: an element each, but
    /// for a part that is itself an unpacked array of such elements, which
    /// gives each of its own.
    pub(crate) fn dynamic_concat(&self, cx: Cx, parts: &[ExprId], dynamic: &DynamicTy) -> Value {
        let nested = matches!(dynamic.element, Ty::Array(_) | Ty::Dynamic(_));
        let mut values = Vec::new();
        for &part in parts {
            match self.self_type(cx, part) {
                Ty::Array(_) | Ty::Dynamic(_) if !nested => match self.eval_self(cx, part) {
                    Value::Array(elements) => values.extend(
                        (elements.iter()).map(|it| self.convert(it.clone(), &dynamic.element)),
                    ),
                    _ => return Value::Unknown,
                },
                _ => values.push(self.eval_assign(cx, part, &dynamic.element)),
            }
            if values.len() as u64 > ELEMENTS {
                return Value::Unknown;
            }
        }
        Value::Array(values.into())
    }

    /// `{a, b}` assigned to an unpacked array: an element each.
    pub(crate) fn unpacked_concat(&self, cx: Cx, parts: &[ExprId], array: &ArrayTy) -> Value {
        let size = array.left.abs_diff(array.right) + 1;
        if parts.len() as u64 != size {
            return Value::Unknown;
        }
        let values = (parts.iter()).map(|&part| self.eval_assign(cx, part, &array.element));
        Value::Array(values.collect())
    }
}

/// The slots a pattern for a value of type `target` fills.
fn slots(target: &Ty) -> Option<Slots> {
    let elements = |left: i64, right: i64, element: Ty, whole: Whole| {
        let size = left.abs_diff(right) + 1;
        (size <= ELEMENTS).then(|| Slots {
            tys: vec![element; size as usize],
            names: Vec::new(),
            bounds: Some((left, right)),
            whole,
        })
    };
    let members = |ty: &StructTy, whole: Whole| {
        (!ty.union).then(|| Slots {
            tys: ty.members.iter().map(|member| member.ty.clone()).collect(),
            names: ty
                .members
                .iter()
                .map(|member| member.name.clone())
                .collect(),
            bounds: None,
            whole,
        })
    };
    match target {
        Ty::Packed(packed) => {
            let whole = Whole::Packed {
                signed: packed.signed,
            };
            match &packed.shape {
                Shape::Vector { left, right } => {
                    let bit = Ty::Packed(Packed::scalar(false, packed.four_state));
                    elements(*left, *right, bit, whole)
                }
                Shape::Array {
                    left,
                    right,
                    element,
                } => elements(*left, *right, Ty::Packed((**element).clone()), whole),
                Shape::Struct(ty) => members(ty, whole),
                Shape::Scalar | Shape::Enum(_) => None,
            }
        }
        Ty::Array(array) => elements(array.left, array.right, array.element.clone(), Whole::Array),
        Ty::Struct(ty) => members(ty, Whole::Struct),
        _ => None,
    }
}
