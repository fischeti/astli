//! What the root says of a top it is built from: the module, and what that
//! module needs defined and its parameters set to, so that no run has to
//! repeat it.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};

/// A `[top.NAME]` table, selected by `--top NAME`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Top {
    /// The module, interface or program elaborated from; the table's own
    /// name when not written.
    pub top: Option<String>,
    /// Each macro's body, by name; `None` for one written `true`, defined as
    /// `-D NAME` defines it.
    #[serde(deserialize_with = "defines")]
    pub defines: BTreeMap<String, Option<String>>,
    /// Each parameter of the top's, by name, with the text that spells its
    /// value as `-G` takes it: a TOML integer's digits, a TOML string in
    /// quotes.
    #[serde(deserialize_with = "params")]
    pub params: BTreeMap<String, String>,
}

impl Top {
    /// What the table named `name` elaborates from.
    pub fn module<'a>(&'a self, name: &'a str) -> &'a str {
        self.top.as_deref().unwrap_or(name)
    }
}

/// A macro's body: a string as written, an integer's digits, or none for
/// `true`.
struct Body(Option<String>);

impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Body, D::Error> {
        struct Of;
        impl Visitor<'_> for Of {
            type Value = Body;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string, an integer, or true")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Body, E> {
                match value {
                    true => Ok(Body(None)),
                    // A table defines; leaving a macro out leaves it undefined.
                    false => Err(E::invalid_value(de::Unexpected::Bool(false), &self)),
                }
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Body, E> {
                Ok(Body(Some(value.to_string())))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Body, E> {
                Ok(Body(Some(value.to_string())))
            }
        }
        deserializer.deserialize_any(Of)
    }
}

/// A parameter's value, as the text `-G` takes: an integer's digits, or a
/// string quoted as SystemVerilog quotes one.
struct Param(String);

impl<'de> Deserialize<'de> for Param {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Param, D::Error> {
        struct Of;
        impl Visitor<'_> for Of {
            type Value = Param;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an integer or a string")
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Param, E> {
                Ok(Param(value.to_string()))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Param, E> {
                Ok(Param(quoted(value)))
            }
        }
        deserializer.deserialize_any(Of)
    }
}

/// `text` as a SystemVerilog string literal.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn defines<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Option<String>>, D::Error> {
    let table = BTreeMap::<String, Body>::deserialize(deserializer)?;
    Ok(table
        .into_iter()
        .map(|(name, Body(body))| (name, body))
        .collect())
}

fn params<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    let table = BTreeMap::<String, Param>::deserialize(deserializer)?;
    Ok(table
        .into_iter()
        .map(|(name, Param(text))| (name, text))
        .collect())
}
