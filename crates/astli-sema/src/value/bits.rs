//! [`Bits`]: an integral value of any width, each bit 0, 1, `x` or `z`.
//!
//! Each bit is a pair, its value and whether it is unknown, kept in two
//! vectors of 64-bit limbs, least significant first: 0 is (0, 0), 1 is
//! (1, 0), `z` is (0, 1) and `x` is (1, 1). Above the width, both are 0.
//!
//! An operation on two values takes them at one width and signing, which
//! the evaluator settles before calling it, as the standard sizes operands
//! before it applies an operator. `x` and `z` are alike to every operator
//! but the case equalities: arithmetic on either is all `x`, a bitwise
//! operator decides what it can bit by bit.

use std::cmp::Ordering;
use std::fmt;

/// One bit of a [`Bits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bit {
    Zero,
    One,
    X,
    Z,
}

impl Bit {
    fn pair(self) -> (bool, bool) {
        match self {
            Bit::Zero => (false, false),
            Bit::One => (true, false),
            Bit::Z => (false, true),
            Bit::X => (true, true),
        }
    }

    fn from_pair(value: bool, unknown: bool) -> Bit {
        match (value, unknown) {
            (false, false) => Bit::Zero,
            (true, false) => Bit::One,
            (false, true) => Bit::Z,
            (true, true) => Bit::X,
        }
    }
}

impl From<bool> for Bit {
    fn from(bit: bool) -> Bit {
        if bit { Bit::One } else { Bit::Zero }
    }
}

/// An integral value: `width` bits, each 0, 1, `x` or `z`, signed or not.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Bits {
    width: u32,
    signed: bool,
    value: Vec<u64>,
    unknown: Vec<u64>,
}

fn limbs(width: u32) -> usize {
    width.div_ceil(64) as usize
}

/// The mask of the low `count` bits of a limb, `count` at most 64.
fn low(count: u32) -> u64 {
    if count >= 64 {
        u64::MAX
    } else {
        (1 << count) - 1
    }
}

/// The 64 bits of `limbs` from bit `lo` up, 0 past the end.
fn chunk(limbs: &[u64], lo: u32) -> u64 {
    let (index, shift) = ((lo / 64) as usize, lo % 64);
    let first = limbs.get(index).copied().unwrap_or(0);
    if shift == 0 {
        return first;
    }
    let second = limbs.get(index + 1).copied().unwrap_or(0);
    (first >> shift) | (second << (64 - shift))
}

/// Writes the low `count` bits of `bits` into `limbs` from bit `lo` up.
fn put(limbs: &mut [u64], lo: u32, bits: u64, count: u32) {
    let mask = low(count);
    let bits = bits & mask;
    let (index, shift) = ((lo / 64) as usize, lo % 64);
    limbs[index] = (limbs[index] & !(mask << shift)) | (bits << shift);
    if shift != 0 && shift + count > 64 {
        let high = mask >> (64 - shift);
        limbs[index + 1] = (limbs[index + 1] & !high) | (bits >> (64 - shift));
    }
}

/// Copies `count` bits of `from`, starting at `from_lo`, into `to` at
/// `to_lo`.
fn copy(to: &mut [u64], to_lo: u32, from: &[u64], from_lo: u32, count: u32) {
    let mut done = 0;
    while done < count {
        let step = (count - done).min(64);
        put(to, to_lo + done, chunk(from, from_lo + done), step);
        done += step;
    }
}

fn add_limbs(a: &[u64], b: &[u64], carry_in: bool) -> Vec<u64> {
    let mut carry = carry_in as u64;
    (a.iter().zip(b))
        .map(|(&a, &b)| {
            let (sum, first) = a.overflowing_add(b);
            let (sum, second) = sum.overflowing_add(carry);
            carry = (first || second) as u64;
            sum
        })
        .collect()
}

fn compare_limbs(a: &[u64], b: &[u64]) -> Ordering {
    for (a, b) in a.iter().rev().zip(b.iter().rev()) {
        match a.cmp(b) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

impl Bits {
    /// The widest value there is. The standard lets a tool limit a vector to
    /// 2^16 bits; past this, a replication or shift that went wrong would
    /// cost memory rather than mean anything.
    pub const MAX_WIDTH: u32 = 1 << 24;

    fn blank(width: u32, signed: bool) -> Bits {
        debug_assert!(width > 0, "a value is at least one bit wide");
        let count = limbs(width);
        Bits {
            width,
            signed,
            value: vec![0; count],
            unknown: vec![0; count],
        }
    }

    /// Clears the bits above the width.
    fn masked(mut self) -> Bits {
        let rest = self.width % 64;
        if rest != 0 {
            let last = self.value.len() - 1;
            self.value[last] &= low(rest);
            self.unknown[last] &= low(rest);
        }
        self
    }

    /// `width` bits, each `bit`.
    pub fn filled(width: u32, signed: bool, bit: Bit) -> Bits {
        let mut bits = Bits::blank(width, signed);
        bits.fill(0, width, bit);
        bits
    }

    /// `value`, its low `width` bits.
    pub fn from_u64(width: u32, signed: bool, value: u64) -> Bits {
        let mut bits = Bits::blank(width, signed);
        bits.value[0] = value;
        bits.masked()
    }

    /// `value`, its low `width` bits, sign-extended past 64.
    pub fn from_i64(width: u32, signed: bool, value: i64) -> Bits {
        let mut bits = Bits::blank(width, signed);
        bits.value[0] = value as u64;
        if value < 0 {
            for limb in &mut bits.value[1..] {
                *limb = u64::MAX;
            }
        }
        bits.masked()
    }

    /// A one-bit unsigned value.
    pub fn bit_value(bit: Bit) -> Bits {
        Bits::filled(1, false, bit)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn is_signed(&self) -> bool {
        self.signed
    }

    /// Whether any bit is `x` or `z`.
    pub fn has_unknown(&self) -> bool {
        self.unknown.iter().any(|&limb| limb != 0)
    }

    pub fn bit(&self, index: u32) -> Bit {
        let (limb, shift) = ((index / 64) as usize, index % 64);
        Bit::from_pair(
            self.value[limb] >> shift & 1 == 1,
            self.unknown[limb] >> shift & 1 == 1,
        )
    }

    /// How many bits it needs: up to its highest bit that is not 0, and at
    /// least one.
    pub fn needed(&self) -> u32 {
        let limbs = (self.value.iter().zip(&self.unknown)).map(|(v, u)| v | u);
        let top = limbs.enumerate().rev().find(|(_, limb)| *limb != 0);
        top.map_or(1, |(index, limb)| {
            index as u32 * 64 + 64 - limb.leading_zeros()
        })
    }

    fn top(&self) -> Bit {
        self.bit(self.width - 1)
    }

    /// Whether it is signed and its sign bit is 1.
    pub fn is_negative(&self) -> bool {
        self.signed && self.top() == Bit::One
    }

    /// Its value, if no bit is unknown and it fits, read unsigned.
    pub fn to_u64(&self) -> Option<u64> {
        if self.has_unknown() || self.value[1..].iter().any(|&limb| limb != 0) {
            return None;
        }
        Some(self.value[0])
    }

    /// Its value, if no bit is unknown and it fits, read as its signing
    /// says.
    pub fn to_i64(&self) -> Option<i64> {
        if self.has_unknown() {
            return None;
        }
        if !self.is_negative() {
            return self.to_u64().and_then(|value| i64::try_from(value).ok());
        }
        let magnitude = self.neg().with_signed(false).to_u64()?;
        (magnitude <= 1 << 63).then(|| (magnitude as i64).wrapping_neg())
    }

    fn fill(&mut self, lo: u32, hi: u32, bit: Bit) {
        let (value, unknown) = bit.pair();
        let mut at = lo;
        while at < hi {
            let step = (hi - at).min(64);
            put(&mut self.value, at, if value { u64::MAX } else { 0 }, step);
            put(
                &mut self.unknown,
                at,
                if unknown { u64::MAX } else { 0 },
                step,
            );
            at += step;
        }
    }

    fn all_x(width: u32, signed: bool) -> Bits {
        Bits::filled(width, signed, Bit::X)
    }

    /// The same bits, signed or not.
    pub fn with_signed(mut self, signed: bool) -> Bits {
        self.signed = signed;
        self
    }

    /// Truncated or extended to `width`: extended by its sign bit if it is
    /// signed, by zeros if not.
    pub fn resize(&self, width: u32) -> Bits {
        let mut bits = Bits::blank(width, self.signed);
        let kept = width.min(self.width);
        copy(&mut bits.value, 0, &self.value, 0, kept);
        copy(&mut bits.unknown, 0, &self.unknown, 0, kept);
        if width > self.width && self.signed {
            bits.fill(self.width, width, self.top());
        }
        bits
    }

    /// `width` bits from bit `lo` up, unsigned; a bit outside the value is
    /// `x`.
    pub fn slice(&self, lo: i64, width: u32) -> Bits {
        let mut bits = Bits::all_x(width, false);
        let start = lo.max(0);
        let end = (lo + i64::from(width)).min(i64::from(self.width));
        if start < end {
            let (to, from, count) = ((start - lo) as u32, start as u32, (end - start) as u32);
            copy(&mut bits.value, to, &self.value, from, count);
            copy(&mut bits.unknown, to, &self.unknown, from, count);
        }
        bits
    }

    /// The parts side by side, the first most significant; unsigned.
    pub fn concat<'a>(parts: impl IntoIterator<Item = &'a Bits> + Clone) -> Bits {
        let width = parts.clone().into_iter().map(|part| part.width).sum();
        let mut bits = Bits::blank(width, false);
        let mut at = width;
        for part in parts {
            at -= part.width;
            copy(&mut bits.value, at, &part.value, 0, part.width);
            copy(&mut bits.unknown, at, &part.unknown, 0, part.width);
        }
        bits
    }

    // ---------------------------------------------------------- bitwise

    fn bitwise(&self, other: &Bits, f: impl Fn(u64, u64, u64, u64) -> (u64, u64)) -> Bits {
        let mut bits = Bits::blank(self.width, self.signed);
        for i in 0..self.value.len() {
            let (value, unknown) = f(
                self.value[i],
                self.unknown[i],
                other.value[i],
                other.unknown[i],
            );
            bits.value[i] = value;
            bits.unknown[i] = unknown;
        }
        bits.masked()
    }

    pub fn not(&self) -> Bits {
        let other = Bits::blank(self.width, self.signed);
        self.bitwise(&other, |av, au, _, _| (!av | au, au))
    }

    pub fn and(&self, other: &Bits) -> Bits {
        self.bitwise(other, |av, au, bv, bu| {
            let zero = (!av & !au) | (!bv & !bu);
            let one = (av & !au) & (bv & !bu);
            let x = !(zero | one);
            (one | x, x)
        })
    }

    pub fn or(&self, other: &Bits) -> Bits {
        self.bitwise(other, |av, au, bv, bu| {
            let one = (av & !au) | (bv & !bu);
            let zero = (!av & !au) & (!bv & !bu);
            let x = !(zero | one);
            (one | x, x)
        })
    }

    pub fn xor(&self, other: &Bits) -> Bits {
        self.bitwise(other, |av, au, bv, bu| {
            let x = au | bu;
            ((av ^ bv) | x, x)
        })
    }

    pub fn xnor(&self, other: &Bits) -> Bits {
        self.xor(other).not()
    }

    // ------------------------------------------------------- arithmetic

    /// `f` over the values when both are known, all `x` when either is not.
    fn arithmetic(&self, other: &Bits, f: impl FnOnce(&Bits, &Bits) -> Bits) -> Bits {
        if self.has_unknown() || other.has_unknown() {
            return Bits::all_x(self.width, self.signed);
        }
        f(self, other)
    }

    fn with_limbs(&self, value: Vec<u64>) -> Bits {
        let unknown = vec![0; value.len()];
        Bits {
            width: self.width,
            signed: self.signed,
            value,
            unknown,
        }
        .masked()
    }

    pub fn add(&self, other: &Bits) -> Bits {
        self.arithmetic(other, |a, b| {
            a.with_limbs(add_limbs(&a.value, &b.value, false))
        })
    }

    pub fn sub(&self, other: &Bits) -> Bits {
        self.arithmetic(other, |a, b| {
            let inverted: Vec<u64> = b.value.iter().map(|limb| !limb).collect();
            a.with_limbs(add_limbs(&a.value, &inverted, true))
        })
    }

    pub fn neg(&self) -> Bits {
        Bits::blank(self.width, self.signed).sub(self)
    }

    pub fn mul(&self, other: &Bits) -> Bits {
        self.arithmetic(other, |a, b| {
            let count = a.value.len();
            let mut product = vec![0u64; count];
            for i in 0..count {
                let mut carry = 0u128;
                for j in 0..count - i {
                    let at = &mut product[i + j];
                    let sum =
                        u128::from(a.value[i]) * u128::from(b.value[j]) + u128::from(*at) + carry;
                    *at = sum as u64;
                    carry = sum >> 64;
                }
            }
            a.with_limbs(product)
        })
    }

    /// The quotient and remainder of the magnitudes, `None` for a divisor
    /// of zero.
    fn divide_unsigned(&self, other: &Bits) -> Option<(Bits, Bits)> {
        if other.value.iter().all(|&limb| limb == 0) {
            return None;
        }
        if self.value.len() == 1 {
            let (a, b) = (self.value[0], other.value[0]);
            return Some((self.with_limbs(vec![a / b]), self.with_limbs(vec![a % b])));
        }
        let mut quotient = vec![0u64; self.value.len()];
        let mut remainder = vec![0u64; self.value.len()];
        for index in (0..self.width).rev() {
            // remainder = remainder << 1 | bit
            let mut carry = (self.value[(index / 64) as usize] >> (index % 64)) & 1;
            for limb in remainder.iter_mut() {
                let next = *limb >> 63;
                *limb = (*limb << 1) | carry;
                carry = next;
            }
            // A remainder past the width cannot be, as it stays below the
            // divisor, which fits.
            if carry == 1 || compare_limbs(&remainder, &other.value) != Ordering::Less {
                let inverted: Vec<u64> = other.value.iter().map(|limb| !limb).collect();
                remainder = add_limbs(&remainder, &inverted, true);
                quotient[(index / 64) as usize] |= 1 << (index % 64);
            }
        }
        Some((self.with_limbs(quotient), self.with_limbs(remainder)))
    }

    /// The quotient and remainder, truncating towards zero as the standard
    /// does, the remainder taking the dividend's sign; all `x` for a
    /// divisor of zero.
    fn divide(&self, other: &Bits) -> (Bits, Bits) {
        let x = || {
            (
                Bits::all_x(self.width, self.signed),
                Bits::all_x(self.width, self.signed),
            )
        };
        if self.has_unknown() || other.has_unknown() {
            return x();
        }
        let (a_negative, b_negative) = (self.is_negative(), other.is_negative());
        let a = if a_negative { self.neg() } else { self.clone() };
        let b = if b_negative {
            other.neg()
        } else {
            other.clone()
        };
        let Some((quotient, remainder)) = a.with_signed(false).divide_unsigned(&b) else {
            return x();
        };
        let quotient = quotient.with_signed(self.signed);
        let remainder = remainder.with_signed(self.signed);
        let quotient = if a_negative != b_negative {
            quotient.neg()
        } else {
            quotient
        };
        let remainder = if a_negative {
            remainder.neg()
        } else {
            remainder
        };
        (quotient, remainder)
    }

    pub fn div(&self, other: &Bits) -> Bits {
        self.divide(other).0
    }

    pub fn rem(&self, other: &Bits) -> Bits {
        self.divide(other).1
    }

    fn is_zero(&self) -> bool {
        !self.has_unknown() && self.value.iter().all(|&limb| limb == 0)
    }

    fn is_one(&self) -> bool {
        !self.has_unknown() && self.value[0] == 1 && self.value[1..].iter().all(|&limb| limb == 0)
    }

    /// `self ** exponent`, at this one's width and signing; the exponent is
    /// self-determined.
    pub fn pow(&self, exponent: &Bits) -> Bits {
        if self.has_unknown() || exponent.has_unknown() {
            return Bits::all_x(self.width, self.signed);
        }
        let one = Bits::from_u64(self.width, self.signed, 1);
        if exponent.is_negative() {
            // Only 1 and -1 have an integer reciprocal; 0 has none.
            let minus_one = self.is_negative() && self.neg().is_one();
            return if self.is_zero() {
                Bits::all_x(self.width, self.signed)
            } else if self.is_one() {
                one
            } else if minus_one {
                match exponent.bit(0) {
                    Bit::One => self.clone(),
                    _ => one,
                }
            } else {
                Bits::blank(self.width, self.signed)
            };
        }
        let mut result = one;
        for index in (0..exponent.width).rev() {
            result = result.mul(&result);
            if exponent.bit(index) == Bit::One {
                result = result.mul(self);
            }
        }
        result
    }

    // ----------------------------------------------------------- shifts

    /// The shift amount, always read unsigned; `None` if it is unknown. One
    /// past the width is as good as any larger.
    fn amount(amount: &Bits, width: u32) -> Option<u32> {
        if amount.has_unknown() {
            return None;
        }
        let large = amount.value[1..].iter().any(|&limb| limb != 0);
        Some(match large {
            true => width,
            false => amount.value[0].min(u64::from(width)) as u32,
        })
    }

    pub fn shl(&self, amount: &Bits) -> Bits {
        let Some(amount) = Bits::amount(amount, self.width) else {
            return Bits::all_x(self.width, self.signed);
        };
        let mut bits = Bits::blank(self.width, self.signed);
        let kept = self.width - amount;
        copy(&mut bits.value, amount, &self.value, 0, kept);
        copy(&mut bits.unknown, amount, &self.unknown, 0, kept);
        bits
    }

    /// `>>`, or `>>>` when `arithmetic`, which fills with the sign bit of a
    /// signed value.
    pub fn shr(&self, amount: &Bits, arithmetic: bool) -> Bits {
        let Some(amount) = Bits::amount(amount, self.width) else {
            return Bits::all_x(self.width, self.signed);
        };
        let mut bits = Bits::blank(self.width, self.signed);
        let kept = self.width - amount;
        copy(&mut bits.value, 0, &self.value, amount, kept);
        copy(&mut bits.unknown, 0, &self.unknown, amount, kept);
        if arithmetic && self.signed {
            bits.fill(kept, self.width, self.top());
        }
        bits
    }

    // ------------------------------------------------------ comparisons

    /// `==`: 0 where a known bit differs, `x` where only an unknown one may.
    pub fn logic_eq(&self, other: &Bits) -> Bit {
        let mut unknown = false;
        for i in 0..self.value.len() {
            let known = !self.unknown[i] & !other.unknown[i];
            if (self.value[i] ^ other.value[i]) & known != 0 {
                return Bit::Zero;
            }
            unknown |= (self.unknown[i] | other.unknown[i]) != 0;
        }
        if unknown { Bit::X } else { Bit::One }
    }

    /// `===`: `x` and `z` match only themselves.
    pub fn case_eq(&self, other: &Bits) -> bool {
        self.value == other.value && self.unknown == other.unknown
    }

    /// `==?`: an `x` or `z` in `other` matches anything.
    pub fn wildcard_eq(&self, other: &Bits) -> Bit {
        let mut unknown = false;
        for i in 0..self.value.len() {
            let care = !other.unknown[i];
            let known = care & !self.unknown[i];
            if (self.value[i] ^ other.value[i]) & known != 0 {
                return Bit::Zero;
            }
            unknown |= care & self.unknown[i] != 0;
        }
        if unknown { Bit::X } else { Bit::One }
    }

    /// The order of two values of one width and signing, `None` if either
    /// has an unknown bit.
    pub fn compare(&self, other: &Bits) -> Option<Ordering> {
        if self.has_unknown() || other.has_unknown() {
            return None;
        }
        Some(match (self.is_negative(), other.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => compare_limbs(&self.value, &other.value),
        })
    }

    // ------------------------------------------------------- reductions

    /// What it is as a condition: 1 if any bit is 1, 0 if every bit is 0,
    /// `x` otherwise. Also `|`, reduced.
    pub fn truth(&self) -> Bit {
        let one = (self.value.iter().zip(&self.unknown)).any(|(v, u)| v & !u != 0);
        match (one, self.has_unknown()) {
            (true, _) => Bit::One,
            (false, true) => Bit::X,
            (false, false) => Bit::Zero,
        }
    }

    /// `&`, reduced.
    pub fn reduce_and(&self) -> Bit {
        let not = self.not();
        match not.truth() {
            Bit::One => Bit::Zero,
            Bit::Zero => Bit::One,
            _ => Bit::X,
        }
    }

    /// `^`, reduced.
    pub fn reduce_xor(&self) -> Bit {
        if self.has_unknown() {
            return Bit::X;
        }
        let ones: u32 = self.value.iter().map(|limb| limb.count_ones()).sum();
        Bit::from(ones % 2 == 1)
    }

    /// How many bits are 1.
    pub fn count_ones(&self) -> u64 {
        let ones = (self.value.iter().zip(&self.unknown)).map(|(v, u)| (v & !u).count_ones());
        ones.map(u64::from).sum()
    }

    /// `casez`'s match: a `z` in either is a bit that matches anything.
    pub fn casez_eq(&self, other: &Bits) -> bool {
        (0..self.value.len()).all(|i| {
            let z = (self.unknown[i] & !self.value[i]) | (other.unknown[i] & !other.value[i]);
            let differ = (self.value[i] ^ other.value[i]) | (self.unknown[i] ^ other.unknown[i]);
            differ & !z == 0
        })
    }

    /// `casex`'s match: an `x` or `z` in either matches anything.
    pub fn casex_eq(&self, other: &Bits) -> bool {
        (0..self.value.len()).all(|i| {
            let care = !(self.unknown[i] | other.unknown[i]);
            (self.value[i] ^ other.value[i]) & care == 0
        })
    }

    /// What `c ? self : other` is when `c` is `x`: each bit the two agree
    /// on and know, `x` where they do not.
    pub fn merge(&self, other: &Bits) -> Bits {
        self.bitwise(other, |av, au, bv, bu| {
            let x = (av ^ bv) | au | bu;
            (av | x, x)
        })
    }

    /// The value with each `x` and `z` bit 0, as a two-state variable holds
    /// it.
    pub fn to_two_state(&self) -> Bits {
        let value = (self.value.iter().zip(&self.unknown)).map(|(v, u)| v & !u);
        Bits {
            value: value.collect(),
            unknown: vec![0; self.value.len()],
            ..self.clone()
        }
    }

    /// Writes `part` over the bits from `lo` up, leaving out what falls
    /// outside the value.
    pub fn write(&mut self, lo: i64, part: &Bits) {
        let start = lo.max(0);
        let end = (lo + i64::from(part.width)).min(i64::from(self.width));
        if start < end {
            let (to, from, count) = (start as u32, (start - lo) as u32, (end - start) as u32);
            copy(&mut self.value, to, &part.value, from, count);
            copy(&mut self.unknown, to, &part.unknown, from, count);
        }
    }

    // ------------------------------------------------------ conversions

    /// Its value as a real, an `x` or `z` bit read as 0.
    pub fn to_f64(&self) -> f64 {
        let known = Bits {
            value: (self.value.iter().zip(&self.unknown))
                .map(|(v, u)| v & !u)
                .collect(),
            unknown: vec![0; self.value.len()],
            ..self.clone()
        };
        let negative = known.is_negative();
        let magnitude = if negative { known.neg() } else { known };
        let value = (magnitude.value.iter().rev()).fold(0.0, |sum, &limb| {
            sum * 18_446_744_073_709_551_616.0 + limb as f64
        });
        if negative { -value } else { value }
    }

    /// `value` rounded to the nearest integer, half away from zero, and
    /// truncated to `width`; all `x` if it is not a number.
    pub fn from_f64(width: u32, signed: bool, value: f64) -> Bits {
        if !value.is_finite() {
            return Bits::all_x(width, signed);
        }
        let rounded = value.round();
        let magnitude = rounded.abs();
        let mut bits = Bits::blank(width.max(64), false);
        if magnitude < 18_446_744_073_709_551_616.0 {
            bits.value[0] = magnitude as u64;
        } else {
            // A double is its 53-bit mantissa, shifted left.
            let raw = magnitude.to_bits();
            let exponent = ((raw >> 52) & 0x7ff) as u32 - 1075;
            let mantissa = (raw & ((1 << 52) - 1)) | (1 << 52);
            let mantissa = Bits::from_u64(bits.width, false, mantissa);
            let amount = Bits::from_u64(32, false, u64::from(exponent));
            bits = mantissa.shl(&amount);
        }
        let bits = if rounded < 0.0 { bits.neg() } else { bits };
        bits.resize(width).with_signed(signed)
    }

    /// Its magnitude in decimal, all bits known.
    fn decimal(&self) -> String {
        const CHUNK: u64 = 10_000_000_000_000_000_000;
        let mut limbs = self.value.clone();
        let mut chunks = Vec::new();
        while limbs.iter().any(|&limb| limb != 0) {
            let mut remainder = 0u128;
            for limb in limbs.iter_mut().rev() {
                let current = (remainder << 64) | u128::from(*limb);
                *limb = (current / u128::from(CHUNK)) as u64;
                remainder = current % u128::from(CHUNK);
            }
            chunks.push(remainder as u64);
        }
        let Some((last, rest)) = chunks.split_last() else {
            return "0".to_string();
        };
        let mut text = last.to_string();
        for chunk in rest.iter().rev() {
            text.push_str(&format!("{chunk:019}"));
        }
        text
    }
}

/// As a literal that means the same: decimal when every bit is known, a
/// negative value signed and after a `-`, binary otherwise.
impl fmt::Display for Bits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let signing = if self.signed { "s" } else { "" };
        if !self.has_unknown() {
            let (sign, magnitude) = match self.is_negative() {
                true => ("-", self.neg().with_signed(false)),
                false => ("", self.clone()),
            };
            return write!(f, "{sign}{}'{signing}d{}", self.width, magnitude.decimal());
        }
        write!(f, "{}'{signing}b", self.width)?;
        let first = self.top();
        if (0..self.width).all(|index| self.bit(index) == first) {
            return write!(f, "{}", if first == Bit::X { 'x' } else { 'z' });
        }
        for index in (0..self.width).rev() {
            let digit = match self.bit(index) {
                Bit::Zero => '0',
                Bit::One => '1',
                Bit::X => 'x',
                Bit::Z => 'z',
            };
            write!(f, "{digit}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Bits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(width: u32, value: u64) -> Bits {
        Bits::from_u64(width, false, value)
    }

    fn s(width: u32, value: i64) -> Bits {
        Bits::from_i64(width, true, value)
    }

    #[test]
    fn display() {
        assert_eq!(u(8, 255).to_string(), "8'd255");
        assert_eq!(s(32, -1).to_string(), "-32'sd1");
        assert_eq!(s(32, i64::from(i32::MIN)).to_string(), "-32'sd2147483648");
        assert_eq!(Bits::filled(4, false, Bit::X).to_string(), "4'bx");
        let mut mixed = u(4, 0b1001);
        mixed.fill(1, 2, Bit::Z);
        assert_eq!(mixed.to_string(), "4'b10z1");
        let wide = u(128, u64::MAX).shl(&u(8, 64)).or(&u(128, u64::MAX));
        assert_eq!(
            wide.to_string(),
            "128'd340282366920938463463374607431768211455"
        );
    }

    #[test]
    fn resize() {
        assert_eq!(s(4, -2).resize(8), s(8, -2));
        assert_eq!(u(4, 0b1110).resize(8), u(8, 0b1110));
        assert_eq!(u(8, 0x1ff).resize(4), u(4, 0xf));
        let x = Bits::filled(2, true, Bit::X).resize(70);
        assert_eq!(x, Bits::filled(70, true, Bit::X));
        assert_eq!(s(64, -1).resize(130).to_i64(), Some(-1));
    }

    #[test]
    fn arithmetic() {
        assert_eq!(u(8, 200).add(&u(8, 100)), u(8, 44));
        assert_eq!(u(8, 1).sub(&u(8, 2)), u(8, 255));
        assert_eq!(s(8, -7).div(&s(8, 2)), s(8, -3));
        assert_eq!(s(8, -7).rem(&s(8, 2)), s(8, -1));
        assert_eq!(s(8, 7).rem(&s(8, -2)), s(8, 1));
        assert_eq!(u(8, 7).div(&u(8, 0)), Bits::filled(8, false, Bit::X));
        assert_eq!(
            u(8, 3).add(&Bits::filled(8, false, Bit::Z)).to_string(),
            "8'bx"
        );
        let big = u(130, 1).shl(&u(8, 129));
        assert_eq!(
            big.div(&u(130, 3))
                .mul(&u(130, 3))
                .add(&big.rem(&u(130, 3))),
            big
        );
        assert_eq!(
            u(130, 1 << 40).mul(&u(130, 1 << 40)),
            u(130, 1).shl(&u(8, 80))
        );
    }

    #[test]
    fn pow() {
        assert_eq!(s(32, 2).pow(&s(32, 10)), s(32, 1024));
        assert_eq!(s(32, 3).pow(&s(32, 0)), s(32, 1));
        assert_eq!(s(32, 0).pow(&s(32, 0)), s(32, 1));
        assert_eq!(s(32, -1).pow(&s(32, -3)), s(32, -1));
        assert_eq!(s(32, -1).pow(&s(32, -4)), s(32, 1));
        assert_eq!(s(32, 2).pow(&s(32, -1)), s(32, 0));
        assert!(s(32, 0).pow(&s(32, -1)).has_unknown());
        assert_eq!(u(8, 2).pow(&u(8, 9)), u(8, 0));
    }

    #[test]
    fn shifts() {
        assert_eq!(u(8, 0b1001).shl(&u(3, 2)), u(8, 0b100100));
        assert_eq!(s(8, -16).shr(&u(3, 2), true), s(8, -4));
        assert_eq!(
            s(8, -16).shr(&u(3, 2), false),
            u(8, 0b0011_1100).with_signed(true)
        );
        assert_eq!(u(8, 0xff).shl(&u(64, 1000)), u(8, 0));
        assert_eq!(u(100, 1).shl(&u(8, 99)).shr(&u(8, 99), false), u(100, 1));
    }

    #[test]
    fn four_state() {
        let x = Bits::filled(1, false, Bit::X);
        let zero = u(1, 0);
        let one = u(1, 1);
        assert_eq!(x.and(&zero), zero);
        assert_eq!(x.and(&one).bit(0), Bit::X);
        assert_eq!(x.or(&one), one);
        assert_eq!(Bits::filled(1, false, Bit::Z).not().bit(0), Bit::X);
        let mut ax = u(4, 0b1010);
        ax.fill(0, 1, Bit::X);
        assert_eq!(ax.logic_eq(&u(4, 0b0000)), Bit::Zero);
        assert_eq!(ax.logic_eq(&u(4, 0b1010)), Bit::X);
        assert!(ax.case_eq(&ax.clone()));
        assert_eq!(u(4, 0b1011).wildcard_eq(&ax), Bit::One);
        assert_eq!(ax.wildcard_eq(&u(4, 0b1010)), Bit::X);
        assert_eq!(ax.truth(), Bit::One);
        assert_eq!(ax.reduce_and(), Bit::Zero);
        assert_eq!(ax.reduce_xor(), Bit::X);
    }

    #[test]
    fn cases() {
        let mut z = u(4, 0b1000);
        z.fill(0, 2, Bit::Z);
        let mut x = u(4, 0b1000);
        x.fill(0, 2, Bit::X);
        assert!(u(4, 0b1011).casez_eq(&z));
        assert!(!u(4, 0b0011).casez_eq(&z));
        assert!(!x.casez_eq(&u(4, 0b1000)));
        assert!(x.casex_eq(&u(4, 0b1011)));
        assert_eq!(u(4, 0b1100).merge(&u(4, 0b1010)).to_string(), "4'b1xx0");
        assert_eq!(x.to_two_state(), u(4, 0b1000));
        let mut written = u(8, 0);
        written.write(6, &u(4, 0b1111));
        assert_eq!(written, u(8, 0b1100_0000));
    }

    #[test]
    fn compare() {
        assert_eq!(s(8, -1).compare(&s(8, 1)), Some(Ordering::Less));
        assert_eq!(u(8, 255).compare(&u(8, 1)), Some(Ordering::Greater));
    }

    #[test]
    fn slices() {
        let value = u(8, 0b1011_0110);
        assert_eq!(value.slice(2, 4), u(4, 0b1101));
        assert_eq!(value.slice(6, 4).to_string(), "4'bxx10");
        assert_eq!(value.slice(-1, 2).to_string(), "2'b0x");
        let joined = Bits::concat([&u(4, 0xa), &u(64, 0), &u(4, 0x5)]);
        assert_eq!(joined.width(), 72);
        assert_eq!(joined.slice(68, 4), u(4, 0xa));
        assert_eq!(joined.slice(0, 4), u(4, 0x5));
    }

    #[test]
    fn reals() {
        assert_eq!(Bits::from_f64(32, true, 2.5), s(32, 3));
        assert_eq!(Bits::from_f64(32, true, -2.5), s(32, -3));
        assert_eq!(Bits::from_f64(8, false, 257.0), u(8, 1));
        assert_eq!(
            Bits::from_f64(80, false, 2f64.powi(70)),
            u(80, 1).shl(&u(8, 70))
        );
        assert_eq!(s(16, -5).to_f64(), -5.0);
        assert_eq!(u(70, 1).shl(&u(8, 66)).to_f64(), 2f64.powi(66));
    }

    #[test]
    fn integers() {
        assert_eq!(s(8, -128).to_i64(), Some(-128));
        assert_eq!(s(100, -3).to_i64(), Some(-3));
        assert_eq!(u(100, 3).shl(&u(8, 80)).to_i64(), None);
        assert_eq!(u(64, u64::MAX).to_i64(), None);
        assert_eq!(u(64, u64::MAX).to_u64(), Some(u64::MAX));
    }
}
