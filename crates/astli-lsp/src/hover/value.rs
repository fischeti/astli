//! A value and a type as hover writes them, on one line short enough for a
//! tooltip heading.
//!
//! A value is written by its type, as a reader of the source would: a
//! struct by member and an array by element, each in its own type's form,
//! as an assignment pattern would; an enum by its member's name; an integer
//! atom as a plain number, a vector of 32 bits or more in hex. A value's
//! own literal, what `elab` prints, is one decimal number whatever the
//! type, which says nothing of a struct or an address map.
//!
//! An aggregate is cut, so a hover stays a line: after a few members or
//! elements, once the line is long, and below a few levels of nesting.

use std::fmt::Write;

use astli_sema::{Bit, Bits, BodyId, Elaboration, EnumTy, Packed, Shape, StructTy, Ty, Value};

/// At most this many members of a struct's value.
const MEMBERS: usize = 8;
/// At most this many elements of an array's value.
const ELEMENTS: usize = 4;
/// Aggregates nested deeper than this are written `'{…}`.
const DEPTH: usize = 3;
/// Once a value is this many bytes long, an aggregate lists no more of it.
const LENGTH: usize = 160;
/// At most this many members of a struct type with no name.
const TYPE_MEMBERS: usize = 4;
/// A vector of this many bits or more is written in hex: it is more often
/// an address or a mask than a count.
const HEX: u32 = 32;

/// `value`, of type `ty`, evaluated in `body`.
pub fn said(elaboration: &Elaboration, body: BodyId, ty: &Ty, value: &Value) -> String {
    let mut said = Said {
        elaboration,
        body,
        out: String::new(),
    };
    said.value(ty, value, 0);
    said.out
}

/// `ty` as code, a struct or union with no name cut to its first members,
/// and one nested in it to `struct {…}`: its declaration as written is
/// shown beside it.
pub fn shown(ty: &Ty) -> String {
    let mut out = String::new();
    ty_into(&mut out, ty, 0);
    out
}

struct Said<'a> {
    elaboration: &'a Elaboration,
    /// Where the value was evaluated, which an enum's members are looked
    /// up in first, as their values may depend on its parameters.
    body: BodyId,
    out: String,
}

impl Said<'_> {
    fn value(&mut self, ty: &Ty, value: &Value, depth: usize) {
        match (ty, value) {
            (_, Value::Unknown) => self.out.push_str("unknown"),
            (Ty::Packed(packed), Value::Bits(bits)) => self.packed(packed, bits, depth),
            (Ty::Struct(ty), Value::Struct(values))
                if !ty.union && ty.members.len() == values.len() =>
            {
                let members = (ty.members.iter().zip(values.iter()))
                    .map(|(member, value)| (Some(&*member.name), member.ty.clone(), value.clone()));
                self.listed(depth, MEMBERS, values.len(), members);
            }
            (Ty::Array(array), Value::Array(values)) => {
                self.elements(&array.element, values, depth)
            }
            (Ty::Dynamic(dynamic), Value::Array(values)) => {
                self.elements(&dynamic.element, values, depth)
            }
            _ => write!(self.out, "{value}").unwrap(),
        }
    }

    fn elements(&mut self, element: &Ty, values: &[Value], depth: usize) {
        let each = values
            .iter()
            .map(|value| (None, element.clone(), value.clone()));
        self.listed(depth, ELEMENTS, values.len(), each);
    }

    /// A packed value: a struct's members and an array's elements are
    /// slices of its bits, the first the most significant.
    fn packed(&mut self, packed: &Packed, bits: &Bits, depth: usize) {
        match &packed.shape {
            Shape::Struct(ty) if !ty.union => {
                let widths: Option<Vec<u32>> = ty.members.iter().map(|it| it.ty.width()).collect();
                let Some(widths) = widths.filter(|it| it.iter().sum::<u32>() == bits.width())
                else {
                    return self.vector(packed, bits);
                };
                let mut top = bits.width();
                let members = ty.members.iter().zip(widths).map(|(member, width)| {
                    top -= width;
                    let signed = member.ty.packed().is_some_and(|it| it.signed);
                    let slice = bits.slice(i64::from(top), width).with_signed(signed);
                    (Some(&*member.name), member.ty.clone(), Value::Bits(slice))
                });
                self.listed(depth, MEMBERS, ty.members.len(), members);
            }
            Shape::Array {
                left,
                right,
                element,
            } => {
                let count = left.abs_diff(*right) + 1;
                let width = element.width;
                if u64::from(width) * count != u64::from(bits.width()) {
                    return self.vector(packed, bits);
                }
                let each = (0..count).rev().map(|index| {
                    let lo = index as i64 * i64::from(width);
                    let slice = bits.slice(lo, width).with_signed(element.signed);
                    (None, Ty::Packed((**element).clone()), Value::Bits(slice))
                });
                self.listed(depth, ELEMENTS, count as usize, each);
            }
            Shape::Enum(ty) => match self.member(ty, bits) {
                Some(name) => self.out.push_str(&name),
                None => self.vector(&ty.base, bits),
            },
            _ => self.vector(packed, bits),
        }
    }

    /// The name of `ty`'s member whose value is `bits`, if one is.
    fn member(&self, ty: &EnumTy, bits: &Bits) -> Option<String> {
        if bits.has_unknown() {
            return None;
        }
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let unsigned = bits.clone().with_signed(false);
        let found = ty.members.iter().find(|&&member| {
            let symbol = design.symbol(member);
            // An enum declared in this body takes its parameters; one of a
            // package or of another definition has its own body.
            let mut value = Value::Unknown;
            if elaboration.scope(self.body).0 == member.file {
                value = elaboration.value(self.body, member.symbol);
            }
            if value == Value::Unknown {
                let body = elaboration.body_at(member.file, symbol.scope);
                value = elaboration.value(body, member.symbol);
            }
            value
                .bits()
                .is_some_and(|it| it.resize(bits.width()).with_signed(false) == unsigned)
        });
        found.map(|&member| design.symbol(member).name.text.to_string())
    }

    /// A vector's value: a plain number for an integer atom type whose
    /// bits are known, hex for another of [`HEX`] bits or more, else its
    /// literal.
    ///
    /// A type keeps its width, its sign and its states rather than its
    /// keyword, so an atom type is a vector `[n-1:0]` of 8, 16, 32 or 64
    /// bits that is two-state or signed, as `integer` is the one atom type
    /// of four states. A vector written out alike, `bit signed [31:0]`, is
    /// plain too.
    fn vector(&mut self, packed: &Packed, bits: &Bits) {
        let atom = match packed {
            Packed {
                width: width @ (8 | 16 | 32 | 64),
                signed,
                four_state,
                shape: Shape::Vector { left, right: 0 },
            } => *left == i64::from(*width) - 1 && (*signed || !*four_state),
            _ => false,
        };
        let known = Some(bits).filter(|bits| atom && !bits.has_unknown());
        let plain = known.and_then(|bits| match bits.is_signed() {
            true => bits.to_i64().map(|it| it.to_string()),
            false => bits.to_u64().map(|it| it.to_string()),
        });
        match plain {
            Some(plain) => self.out.push_str(&plain),
            None if !atom && bits.width() >= HEX => self.out.push_str(&hex(bits)),
            None => write!(self.out, "{bits}").unwrap(),
        }
    }

    /// An assignment pattern of `each`, members by name and elements by
    /// position, `total` of them, cut at `limit` or once the line is
    /// [`LENGTH`] long, with a count of the rest.
    fn listed<'n>(
        &mut self,
        depth: usize,
        limit: usize,
        total: usize,
        each: impl Iterator<Item = (Option<&'n str>, Ty, Value)>,
    ) {
        if depth >= DEPTH {
            self.out.push_str("'{…}");
            return;
        }
        self.out.push_str("'{");
        let mut shown = 0;
        for (name, ty, value) in each {
            if shown == limit || (shown > 0 && self.out.len() >= LENGTH) {
                break;
            }
            if shown > 0 {
                self.out.push_str(", ");
            }
            if let Some(name) = name {
                write!(self.out, "{name}: ").unwrap();
            }
            self.value(&ty, &value, depth + 1);
            shown += 1;
        }
        if total > shown {
            write!(self.out, ", … {} more", total - shown).unwrap();
        }
        self.out.push('}');
    }
}

/// `bits` as a sized hex literal, `_` every four digits. A digit of bits
/// some known and some not cannot say which, so such a value is binary.
fn hex(bits: &Bits) -> String {
    let mut digits = Vec::new();
    for digit in (0..bits.width().div_ceil(4)).rev() {
        let lo = digit * 4;
        let each: Vec<Bit> = (lo..(lo + 4).min(bits.width()))
            .map(|index| bits.bit(index))
            .collect();
        let all = |is: Bit| each.iter().all(|bit| *bit == is);
        let known = each.iter().all(|bit| matches!(bit, Bit::Zero | Bit::One));
        digits.push(if all(Bit::X) {
            'x'
        } else if all(Bit::Z) {
            'z'
        } else if known {
            let value =
                (each.iter().rev()).fold(0, |value, bit| value * 2 + u32::from(*bit == Bit::One));
            char::from_digit(value, 16).unwrap()
        } else {
            return grouped(bits, 'b', &binary(bits));
        });
    }
    grouped(bits, 'h', &digits)
}

/// `bits`' digits in binary, the most significant first.
fn binary(bits: &Bits) -> Vec<char> {
    (0..bits.width())
        .rev()
        .map(|index| match bits.bit(index) {
            Bit::Zero => '0',
            Bit::One => '1',
            Bit::X => 'x',
            Bit::Z => 'z',
        })
        .collect()
}

/// A sized literal of `digits` in `base`, `_` every four from the right,
/// or one digit where all are the same `x` or `z`, as a literal extends it.
fn grouped(bits: &Bits, base: char, digits: &[char]) -> String {
    if digits.is_empty() {
        return bits.to_string();
    }
    let signing = if bits.is_signed() { "s" } else { "" };
    let mut out = format!("{}'{signing}{base}", bits.width());
    if matches!(digits[0], 'x' | 'z') && digits.iter().all(|it| *it == digits[0]) {
        out.push(digits[0]);
        return out;
    }
    for (index, digit) in digits.iter().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(4) {
            out.push('_');
        }
        out.push(*digit);
    }
    out
}

fn ty_into(out: &mut String, ty: &Ty, depth: usize) {
    match ty {
        Ty::Struct(ty) => struct_into(out, ty, depth),
        Ty::Packed(packed) => packed_into(out, packed, depth),
        Ty::Array(array) => {
            write!(out, "array [{}:{}] of ", array.left, array.right).unwrap();
            ty_into(out, &array.element, depth);
        }
        Ty::Dynamic(dynamic) => {
            out.push_str(match dynamic.queue {
                true => "queue of ",
                false => "dynamic array of ",
            });
            ty_into(out, &dynamic.element, depth);
        }
        _ => write!(out, "{ty}").unwrap(),
    }
}

/// A packed type as it displays, its innermost element first and its
/// dimensions after, unless that element is a struct with no name.
fn packed_into(out: &mut String, packed: &Packed, depth: usize) {
    let mut dims = String::new();
    let mut at = packed;
    while let Shape::Array {
        left,
        right,
        element,
    } = &at.shape
    {
        write!(dims, " [{left}:{right}]").unwrap();
        at = element;
    }
    match &at.shape {
        Shape::Struct(ty) if ty.name.is_none() => {
            struct_into(out, ty, depth);
            if packed.signed {
                out.push_str(" signed");
            }
            out.push_str(&dims);
        }
        _ => write!(out, "{packed}").unwrap(),
    }
}

fn struct_into(out: &mut String, ty: &StructTy, depth: usize) {
    if let Some(name) = &ty.name {
        out.push_str(name);
        return;
    }
    out.push_str(if ty.union { "union {" } else { "struct {" });
    if depth > 0 {
        out.push_str("…}");
        return;
    }
    for (index, member) in ty.members.iter().take(TYPE_MEMBERS).enumerate() {
        if index > 0 {
            out.push_str("; ");
        }
        write!(out, "{}: ", member.name).unwrap();
        ty_into(out, &member.ty, depth + 1);
    }
    match ty.members.len().saturating_sub(TYPE_MEMBERS) {
        0 => {}
        more => write!(out, "; … {more} more").unwrap(),
    }
    out.push('}');
}
