//! Values: what a constant expression evaluates to.

mod bits;
mod literal;

use std::fmt;
use std::sync::Arc;

pub use bits::{Bit, Bits};
pub use literal::{Literal, literal};

/// The value of a constant expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An integral value: a vector of bits, each 0, 1, `x` or `z`.
    Bits(Bits),
    Real(f64),
    String(Box<[u8]>),
    /// An unpacked array's elements, from its left bound to its right.
    Array(Arc<[Value]>),
    /// An unpacked struct's members, in order.
    Struct(Arc<[Value]>),
    /// What sema cannot evaluate: it depends on what sema does not model,
    /// or on what only elaborating the design decides. It absorbs what is
    /// computed from it, and is never an error: [unknown is
    /// silent](crate#unknown-is-silent).
    Unknown,
}

impl Value {
    pub fn is_unknown(&self) -> bool {
        matches!(self, Value::Unknown)
    }

    /// Its integral value, if it has one.
    pub fn bits(&self) -> Option<&Bits> {
        match self {
            Value::Bits(bits) => Some(bits),
            _ => None,
        }
    }
}

/// As a literal that means the same, or `unknown`.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bits(bits) => write!(f, "{bits}"),
            Value::Real(real) => write!(f, "{real:?}"),
            Value::String(bytes) => {
                f.write_str("\"")?;
                for &byte in bytes.iter() {
                    match byte {
                        b'"' => f.write_str("\\\"")?,
                        b'\\' => f.write_str("\\\\")?,
                        b'\n' => f.write_str("\\n")?,
                        b'\t' => f.write_str("\\t")?,
                        0x20..=0x7e => write!(f, "{}", byte as char)?,
                        _ => write!(f, "\\x{byte:02x}")?,
                    }
                }
                f.write_str("\"")
            }
            Value::Array(values) | Value::Struct(values) => {
                f.write_str("'{")?;
                for (index, value) in values.iter().enumerate() {
                    let separator = if index == 0 { "" } else { ", " };
                    write!(f, "{separator}{value}")?;
                }
                f.write_str("}")
            }
            Value::Unknown => f.write_str("unknown"),
        }
    }
}
