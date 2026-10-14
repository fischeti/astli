//! Types, as far as constants need them: what a value is made of and how
//! wide it is.

use std::fmt;

/// The type of a declaration or an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    /// A packed type: a vector of bits, an enum, a packed struct.
    Integral(Integral),
    /// `real`, `shortreal` or `realtime`, all held as a double.
    Real,
    String,
    /// A type sema does not model yet, or cannot decide: an unpacked array,
    /// an unpacked struct, a class, a type parameter.
    Unknown,
}

/// A packed type, by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Integral {
    pub width: u32,
    pub signed: bool,
    /// Whether a bit may be `x` or `z`: `logic` and `integer`, not `bit` or
    /// `int`.
    pub four_state: bool,
    /// A one-dimensional vector's bounds, `[left:right]`, which a select
    /// indexes; `None` for a type of more dimensions or of members.
    pub range: Option<(i64, i64)>,
}

impl Integral {
    /// `width` bits indexed `[width-1:0]`.
    pub fn vector(width: u32, signed: bool, four_state: bool) -> Integral {
        Integral {
            width,
            signed,
            four_state,
            range: Some((i64::from(width) - 1, 0)),
        }
    }
}

impl Ty {
    /// `int`: 32 bits, signed, two states.
    pub(crate) const INT: Ty = Ty::Integral(Integral {
        width: 32,
        signed: true,
        four_state: false,
        range: Some((31, 0)),
    });

    /// `integer`: 32 bits, signed, four states.
    pub(crate) const INTEGER: Ty = Ty::Integral(Integral {
        width: 32,
        signed: true,
        four_state: true,
        range: Some((31, 0)),
    });

    /// A single bit, the type of a comparison.
    pub(crate) const BIT: Ty = Ty::Integral(Integral {
        width: 1,
        signed: false,
        four_state: true,
        range: Some((0, 0)),
    });

    pub fn integral(self) -> Option<Integral> {
        match self {
            Ty::Integral(integral) => Some(integral),
            _ => None,
        }
    }

    /// Its width in bits, if it is integral.
    pub fn width(self) -> Option<u32> {
        self.integral().map(|integral| integral.width)
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Integral(integral) => {
                let state = if integral.four_state { "logic" } else { "bit" };
                let signing = if integral.signed { " signed" } else { "" };
                match integral.range {
                    Some((0, 0)) => write!(f, "{state}{signing}"),
                    Some((left, right)) => write!(f, "{state}{signing} [{left}:{right}]"),
                    None => write!(f, "{state}{signing} ({} bits)", integral.width),
                }
            }
            Ty::Real => f.write_str("real"),
            Ty::String => f.write_str("string"),
            Ty::Unknown => f.write_str("unknown"),
        }
    }
}
