//! A [`Json`] document, such as the design `elab --json` writes, and the
//! escaping every JSON string the driver writes goes through.
//!
//! Written by hand rather than through a serializer: what the driver writes
//! is a handful of strings and numbers, and escaping a string is all JSON
//! asks of it.

use std::fmt::Write as _;

/// A JSON value, built up and then written whole, indented for a person
/// to read as well as a program.
pub enum Json {
    Null,
    Bool(bool),
    Int(i128),
    String(String),
    Array(Vec<Json>),
    /// Its keys in the order written.
    Object(Vec<(&'static str, Json)>),
}

impl Json {
    /// `text` as a string.
    pub fn str(text: impl Into<String>) -> Json {
        Json::String(text.into())
    }

    /// `value` as a number.
    pub fn int(value: impl Into<i128>) -> Json {
        Json::Int(value.into())
    }

    /// The value, or null for none.
    pub fn or_null<T>(value: Option<T>, json: impl FnOnce(T) -> Json) -> Json {
        value.map_or(Json::Null, json)
    }

    /// Writes it at `depth` levels of indentation, two spaces each, and a
    /// list or an object with nothing in it, or a list of numbers, on one
    /// line.
    fn write(&self, out: &mut String, depth: usize) {
        let indent = |out: &mut String, depth: usize| {
            out.push('\n');
            out.extend(std::iter::repeat_n("  ", depth));
        };
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Int(value) => {
                let _ = write!(out, "{value}");
            }
            Json::String(text) => out.push_str(&string(text)),
            Json::Array(items) if items.iter().all(|item| matches!(item, Json::Int(_))) => {
                out.push('[');
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        out.push_str(", ");
                    }
                    item.write(out, depth);
                }
                out.push(']');
            }
            Json::Array(items) => {
                out.push('[');
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                }
                indent(out, depth);
                out.push(']');
            }
            Json::Object(fields) if fields.is_empty() => out.push_str("{}"),
            Json::Object(fields) => {
                out.push('{');
                for (at, (key, value)) in fields.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    indent(out, depth + 1);
                    out.push_str(&string(key));
                    out.push_str(": ");
                    value.write(out, depth + 1);
                }
                indent(out, depth);
                out.push('}');
            }
        }
    }
}

/// The document, ending in a newline.
impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        self.write(&mut out, 0);
        writeln!(f, "{out}")
    }
}

/// `text` as a JSON string.
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A 64-bit FNV-1a hash of `parts`, in hex. It must not change between
/// runs, or between builds of astli, as `std`'s hasher may.
pub(crate) fn hash<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        // A zero between parts keeps `ab`, `c` apart from `a`, `bc`.
        for &byte in part.iter().chain(&[0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}
