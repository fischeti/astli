//! Types: what a declaration holds and what an expression yields, as far as
//! constants and widths need them.

use std::fmt;
use std::sync::Arc;

use astli_text::Span;

use crate::design::SymbolRef;

/// The type of a declaration or an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// A packed type: a vector of bits, however they are organised.
    Packed(Packed),
    /// `real`, `shortreal` or `realtime`, all held as a double.
    Real,
    String,
    /// An unpacked array of a fixed size.
    Array(Arc<ArrayTy>),
    /// A dynamic array or an unbounded queue, whose size is its value's.
    Dynamic(Arc<DynamicTy>),
    /// An unpacked struct or union.
    Struct(Arc<StructTy>),
    /// A type sema does not model, or cannot decide: an associative array,
    /// a bounded queue, a class, an interface.
    Unknown,
}

/// A packed type, by its bits and how they are organised.
#[derive(Debug, Clone, PartialEq)]
pub struct Packed {
    pub width: u32,
    pub signed: bool,
    /// Whether a bit may be `x` or `z`: `logic` and `integer`, not `bit` or
    /// `int`.
    pub four_state: bool,
    pub shape: Shape,
}

/// How a packed type's bits are organised.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A single bit: `logic`, `bit`.
    Scalar,
    /// A vector of single bits, indexed `[left:right]`: `logic [7:0]`, `int`.
    Vector {
        left: i64,
        right: i64,
    },
    /// A packed array of wider elements, `[left:right]` its outermost
    /// dimension: `logic [3:0][7:0]`, `byte_t [1:0]`.
    Array {
        left: i64,
        right: i64,
        element: Arc<Packed>,
    },
    /// A packed struct or union, its first member the most significant.
    Struct(Arc<StructTy>),
    Enum(Arc<EnumTy>),
}

/// An unpacked array of a fixed size, indexed `[left:right]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayTy {
    pub left: i64,
    pub right: i64,
    pub element: Ty,
}

/// A dynamic array, `[]`, or an unbounded queue, `[$]`: indexed from 0 to
/// its size less one.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicTy {
    pub queue: bool,
    pub element: Ty,
}

impl DynamicTy {
    /// The fixed array a value of `size` elements is.
    pub fn sized(&self, size: usize) -> ArrayTy {
        ArrayTy {
            left: 0,
            right: size as i64 - 1,
            element: self.element.clone(),
        }
    }
}

/// A struct or union, packed or not.
///
/// Two are the same type only if they are declared in the same place, as
/// their members' declarations tell.
#[derive(Debug, Clone, PartialEq)]
pub struct StructTy {
    /// The name of the typedef that declares it, if one does.
    pub name: Option<Box<str>>,
    pub typedef: Option<SymbolRef>,
    pub union: bool,
    pub members: Vec<MemberTy>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemberTy {
    pub name: Box<str>,
    /// Where the member is declared.
    pub span: Span,
    pub ty: Ty,
}

/// An enum: its base type and its members, each a symbol whose value a
/// body gives.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumTy {
    /// The name of the typedef that declares it, if one does.
    pub name: Option<Box<str>>,
    pub typedef: Option<SymbolRef>,
    pub base: Packed,
    pub members: Vec<SymbolRef>,
}

impl Packed {
    pub const fn scalar(signed: bool, four_state: bool) -> Packed {
        Packed {
            width: 1,
            signed,
            four_state,
            shape: Shape::Scalar,
        }
    }

    /// `width` bits indexed `[width-1:0]`.
    pub const fn vector(width: u32, signed: bool, four_state: bool) -> Packed {
        Packed {
            width,
            signed,
            four_state,
            shape: Shape::Vector {
                left: width as i64 - 1,
                right: 0,
            },
        }
    }

    /// The bounds a select indexes: a vector's or a packed array's own, a
    /// struct's or an enum's bits as `[width-1:0]`; `None` for a scalar.
    pub fn range(&self) -> Option<(i64, i64)> {
        match &self.shape {
            Shape::Scalar => None,
            Shape::Vector { left, right } | Shape::Array { left, right, .. } => {
                Some((*left, *right))
            }
            Shape::Struct(_) | Shape::Enum(_) => Some((i64::from(self.width) - 1, 0)),
        }
    }

    /// What one index of [`range`](Packed::range) selects: an element of a
    /// packed array, or a bit.
    pub fn element(&self) -> Packed {
        match &self.shape {
            Shape::Array { element, .. } => (**element).clone(),
            _ => Packed::scalar(false, self.four_state),
        }
    }
}

impl Ty {
    /// `int`: 32 bits, signed, two states.
    pub const INT: Ty = Ty::Packed(Packed::vector(32, true, false));

    /// `integer`: 32 bits, signed, four states.
    pub const INTEGER: Ty = Ty::Packed(Packed::vector(32, true, true));

    /// A single bit, the type of a comparison.
    pub const BIT: Ty = Ty::Packed(Packed::scalar(false, true));

    pub fn packed(&self) -> Option<&Packed> {
        match self {
            Ty::Packed(packed) => Some(packed),
            _ => None,
        }
    }

    /// Its width in bits, if it is packed.
    pub fn width(&self) -> Option<u32> {
        self.packed().map(|packed| packed.width)
    }

    /// How many bits it holds, if that is fixed: what `$bits` says.
    pub fn bits(&self) -> Option<u64> {
        match self {
            Ty::Packed(packed) => Some(u64::from(packed.width)),
            Ty::Real => Some(64),
            Ty::Array(array) => {
                let size = array.left.abs_diff(array.right) + 1;
                array.element.bits()?.checked_mul(size)
            }
            Ty::Struct(ty) => {
                let mut members = ty.members.iter().map(|member| member.ty.bits());
                match ty.union {
                    true => members.try_fold(0, |max, bits| Some(max.max(bits?))),
                    false => members.try_fold(0u64, |sum, bits| sum.checked_add(bits?)),
                }
            }
            Ty::String | Ty::Dynamic(_) | Ty::Unknown => None,
        }
    }

    /// The bounds of its `n`th dimension, unpacked ones first, counting from
    /// 1; what `$left` and its kind read. A dynamic array's own is its
    /// value's, which a type does not know.
    pub fn dimension(&self, n: u32) -> Option<(i64, i64)> {
        if let Ty::Dynamic(dynamic) = self {
            return dynamic
                .element
                .dimension(n.checked_sub(1).filter(|&n| n > 0)?);
        }
        let range = match self {
            Ty::Array(array) => (array.left, array.right),
            Ty::Packed(packed) => packed.range()?,
            _ => return None,
        };
        match n {
            0 => None,
            1 => Some(range),
            n => match self {
                Ty::Array(array) => array.element.dimension(n - 1),
                Ty::Packed(packed) => Ty::Packed(packed.element()).dimension(n - 1),
                _ => None,
            },
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Packed(packed) => write!(f, "{packed}"),
            Ty::Real => f.write_str("real"),
            Ty::String => f.write_str("string"),
            Ty::Array(array) => write!(
                f,
                "array [{}:{}] of {}",
                array.left, array.right, array.element
            ),
            Ty::Dynamic(dynamic) => match dynamic.queue {
                true => write!(f, "queue of {}", dynamic.element),
                false => write!(f, "dynamic array of {}", dynamic.element),
            },
            Ty::Struct(ty) => write!(f, "{ty}"),
            Ty::Unknown => f.write_str("unknown"),
        }
    }
}

impl fmt::Display for Packed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let signing = if self.signed { " signed" } else { "" };
        let state = if self.four_state { "logic" } else { "bit" };
        // The innermost element names the type; the dimensions follow it,
        // outermost first.
        let mut dims = String::new();
        let mut at = self;
        loop {
            match &at.shape {
                Shape::Scalar => return write!(f, "{state}{signing}{dims}"),
                Shape::Vector { left, right } => {
                    return write!(f, "{state}{signing}{dims} [{left}:{right}]");
                }
                Shape::Array {
                    left,
                    right,
                    element,
                } => {
                    dims.push_str(&format!(" [{left}:{right}]"));
                    at = element;
                }
                Shape::Struct(ty) => return write!(f, "{ty}{signing}{dims}"),
                Shape::Enum(ty) => {
                    return match &ty.name {
                        Some(name) => write!(f, "{name}{dims}"),
                        None => write!(f, "enum {}{dims}", ty.base),
                    };
                }
            }
        }
    }
}

impl fmt::Display for StructTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = &self.name {
            return f.write_str(name);
        }
        f.write_str(if self.union { "union {" } else { "struct {" })?;
        for (index, member) in self.members.iter().enumerate() {
            let separator = if index == 0 { "" } else { "; " };
            write!(f, "{separator}{}: {}", member.name, member.ty)?;
        }
        f.write_str("}")
    }
}
