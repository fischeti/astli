//! A top's parameters set from outside the design: by the text of a
//! literal, as a simulator's `-G` takes it, or to a value of the
//! parameter's own type, as a sweep lists one and a point's name spells it.

use super::{BodyId, Elaboration};
use crate::design::SymbolRef;
use crate::hir::*;
use crate::types::{Packed, Shape, Ty};
use crate::value::{Bits, Literal, literal};

/// The widest integral type whose values have a short spelling. A point's
/// name and a `-G` hold a value as an integer in decimal, which past 64
/// bits is a run of digits nobody reads or types; a value a sweep lists by
/// hand fits, and so does any of a `longint` or an address.
const WIDEST: u32 = 64;

/// Why a top cannot be built as asked, or one of its parameters set so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopError {
    /// No module, interface or program has the name.
    NoTop,
    /// The top has no parameter of this name.
    NoParameter(String),
    /// A `localparam`, which only its declaration sets.
    Local(String),
    /// A body `parameter` of a top with a parameter port list, which makes
    /// it local.
    Listed(String),
    /// A type parameter, which a value cannot name.
    Type(String),
    /// The parameter named, and text that spells no value it could take: no
    /// number or string as a literal, no number or name as a value.
    NotAValue(String, String),
    /// A type sema cannot tell, or an enum whose member's value it cannot.
    Unknown(String),
    /// A type whose values have no short spelling: a real, a string, a
    /// struct, an unpacked type, or one wider than 64 bits.
    NoSpelling(String),
    /// A type with more values than a list holds: not an enum, a `bit` or a
    /// 1-bit `logic`.
    NoValues(String),
    /// The parameter named, and a number its type's width and signedness
    /// cannot hold.
    OutOfRange(String, String),
    /// The parameter named, of an enum type, and a name that is none of its
    /// members.
    NoMember(String, String),
    /// The parameter named, of an enum type, and a number that is no
    /// member's value.
    NoMemberValued(String, String),
}

/// What a top's parameter is set to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopSetting {
    /// The text of a literal, as a simulator's `-G` takes it: a number,
    /// sized or not and maybe negated, a real or a string, converted to the
    /// parameter's type as an override is.
    Literal(String),
    /// A value of the parameter's type, as [`Elaboration::top_value`]
    /// reads one.
    ///
    /// [`Elaboration::top_value`]: super::Elaboration::top_value
    Value(TopValue),
}

/// A value of a top's parameter's type that has a short spelling: an
/// integral one no wider than 64 bits, or an enum's member.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopValue {
    /// At the type's width and signedness, an enum's those of its base.
    bits: Bits,
    /// The enum member it is, by name.
    member: Option<Box<str>>,
}

impl TopValue {
    pub fn bits(&self) -> &Bits {
        &self.bits
    }

    /// The name of the enum member it is.
    pub fn member(&self) -> Option<&str> {
        self.member.as_deref()
    }

    /// The integer a simulator's `-G` takes: in decimal, after a `-` when
    /// it is negative.
    pub fn integer(&self) -> String {
        match self.bits.is_signed() {
            true => self.bits.to_i64().map(|it| it.to_string()),
            false => self.bits.to_u64().map(|it| it.to_string()),
        }
        .expect("a top's value is known and no wider than 64 bits")
    }

    /// As a point's name spells it: an enum member by its name, any other
    /// value as its [`integer`](TopValue::integer).
    pub fn short(&self) -> String {
        match &self.member {
            Some(name) => name.to_string(),
            None => self.integer(),
        }
    }

    /// As a number, which 64 bits of either signedness fit.
    fn number(&self) -> i128 {
        match self.bits.is_signed() {
            true => self.bits.to_i64().map(i128::from),
            false => self.bits.to_u64().map(i128::from),
        }
        .expect("a top's value is known and no wider than 64 bits")
    }
}

impl Elaboration {
    /// The body of `name` as a top, as [`Elaboration::top`] gives it, with
    /// each of `parameters` set, by name, as its [`TopSetting`] says. A
    /// `defparam` whose path starts at the top still wins, as it does over
    /// an instance's override.
    pub fn top_with(
        &self,
        name: &str,
        parameters: &[(impl AsRef<str>, TopSetting)],
    ) -> Result<BodyId, TopError> {
        let definition = self.design.definition(name).ok_or(TopError::NoTop)?;
        let mut set = Vec::new();
        for (name, setting) in parameters {
            let name = name.as_ref();
            let parameter = self.top_parameter(definition, name)?;
            let (text, literal) = match setting {
                TopSetting::Literal(text) => match given_literal(text) {
                    Some(literal) => (text.clone(), literal),
                    None => return Err(TopError::NotAValue(name.into(), text.clone())),
                },
                // Keyed by the literal that spells the bits, so a `-G` of
                // that text shares the body.
                TopSetting::Value(value) => {
                    (value.bits.to_string(), Literal::Bits(value.bits.clone()))
                }
            };
            set.push((parameter, text, literal));
        }
        self.top_set(definition, set).ok_or(TopError::NoTop)
    }

    /// Every value of the type of the parameter `name` of `top`, a top's
    /// body, where the type has a few: an enum's members in the order
    /// declared, or 0 and 1 for a `bit` or any other single bit. The type
    /// is the one `top` gives it, so one an earlier parameter decides
    /// follows what `top` sets that one to.
    pub fn top_values(&self, top: BodyId, name: &str) -> Result<Vec<TopValue>, TopError> {
        let (_, packed) = self.spelled(top, name)?;
        match &packed.shape {
            Shape::Enum(ty) => (ty.members.iter())
                .map(|&member| self.enum_value(top, name, member))
                .collect(),
            _ if packed.width == 1 => Ok((0..2)
                .map(|bit| TopValue {
                    bits: Bits::from_u64(1, packed.signed, bit),
                    member: None,
                })
                .collect()),
            _ => Err(TopError::NoValues(name.into())),
        }
    }

    /// The value of the type of the parameter `name` of `top`, a top's
    /// body, that `text` spells: a number in decimal, maybe after a `-`, or
    /// an enum member's name. A number its type cannot hold, or an enum
    /// member no name or number names, is refused, as is a type whose
    /// values have no short spelling. What it gives is for setting that
    /// parameter, of a body `top` or its parameters before this one.
    pub fn top_value(&self, top: BodyId, name: &str, text: &str) -> Result<TopValue, TopError> {
        let (_, packed) = self.spelled(top, name)?;
        let not_a_value = || TopError::NotAValue(name.into(), text.into());
        if let Shape::Enum(ty) = &packed.shape {
            let members = ty.members.iter();
            let values = members.map(|&member| self.enum_value(top, name, member));
            let values = values.collect::<Result<Vec<_>, _>>()?;
            if is_identifier(text) {
                return (values.into_iter())
                    .find(|value| value.member.as_deref() == Some(text))
                    .ok_or_else(|| TopError::NoMember(name.into(), text.into()));
            }
            let number = decimal(text).ok_or_else(not_a_value)?;
            return (values.into_iter())
                .find(|value| Some(value.number()) == number)
                .ok_or_else(|| TopError::NoMemberValued(name.into(), text.into()));
        }
        let number = decimal(text).ok_or_else(not_a_value)?;
        let (width, signed) = (packed.width, packed.signed);
        let (low, high) = match signed {
            true => (-(1i128 << (width - 1)), (1i128 << (width - 1)) - 1),
            false => (0, (1i128 << width) - 1),
        };
        match number {
            Some(number) if (low..=high).contains(&number) => Ok(TopValue {
                bits: match number < 0 {
                    true => Bits::from_i64(width, signed, number as i64),
                    false => Bits::from_u64(width, signed, number as u64),
                },
                member: None,
            }),
            _ => Err(TopError::OutOfRange(name.into(), text.into())),
        }
    }

    /// `body`'s parameter `name` and its type, if the type's values have a
    /// short spelling.
    fn spelled(&self, body: BodyId, name: &str) -> Result<(SymbolId, Packed), TopError> {
        let (file, scope) = self.scope(body);
        let definition = match self.design[file][scope].owner {
            Some(symbol) if self.bodies.get(body.index()).parent.is_none() => {
                SymbolRef { file, symbol }
            }
            _ => return Err(TopError::NoTop),
        };
        let symbol = self.top_parameter(definition, name)?;
        match self.type_of(body, symbol) {
            Ty::Unknown => Err(TopError::Unknown(name.into())),
            Ty::Packed(packed)
                if packed.width <= WIDEST && !matches!(packed.shape, Shape::Struct(_)) =>
            {
                Ok((symbol, packed))
            }
            _ => Err(TopError::NoSpelling(name.into())),
        }
    }

    /// The value of the enum member `member`, seen from `body`.
    fn enum_value(
        &self,
        body: BodyId,
        name: &str,
        member: SymbolRef,
    ) -> Result<TopValue, TopError> {
        let value = self.symbol_value(self.cx(body), member);
        match value.bits() {
            Some(bits) if bits.has_unknown() => Err(TopError::NoSpelling(name.into())),
            Some(bits) => Ok(TopValue {
                bits: bits.clone(),
                member: Some(self.design.symbol(member).name.text.clone()),
            }),
            None => Err(TopError::Unknown(name.into())),
        }
    }

    /// The parameter `name` of `definition` that a top may be given a value
    /// for.
    fn top_parameter(&self, definition: SymbolRef, name: &str) -> Result<SymbolId, TopError> {
        let SymbolKind::Definition {
            scope, overridable, ..
        } = &self.design.symbol(definition).kind
        else {
            return Err(TopError::NoTop);
        };
        let hir = &self.design[definition.file];
        let named = |&&symbol: &&SymbolId| *hir[symbol].name.text == *name;
        if let Some(&parameter) = overridable.iter().find(named) {
            return match &hir[parameter].kind {
                SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                    Err(TopError::Type(name.into()))
                }
                _ => Ok(parameter),
            };
        }
        // Not one an override sets, but declared in the body: say why.
        let declared = self.design.declared(definition.file, *scope, name);
        match declared.map(|it| &hir[it].kind) {
            Some(SymbolKind::Parameter(Parameter {
                kind: ParameterKind::Demoted,
                ..
            })) => Err(TopError::Listed(name.into())),
            Some(SymbolKind::Parameter(_)) => Err(TopError::Local(name.into())),
            _ => Err(TopError::NoParameter(name.into())),
        }
    }
}

/// The literal `text` spells as a parameter's value set from outside the
/// design: a number, which a `-` before may negate, a real or a string.
fn given_literal(text: &str) -> Option<Literal> {
    let (negated, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    match (literal(text), negated) {
        (Literal::Unknown, _) => None,
        (Literal::Bits(bits), true) => Some(Literal::Bits(bits.neg())),
        (Literal::Real(real), true) => Some(Literal::Real(-real)),
        (_, true) => None,
        (it, false) => Some(it),
    }
}

/// The number `text` spells in decimal, maybe after a `-`: `None` if it
/// spells none, `Some(None)` if it is too large for any type with a short
/// spelling.
fn decimal(text: &str) -> Option<Option<i128>> {
    let (negated, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if !digits.starts_with(|c: char| c.is_ascii_digit())
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let digits: String = digits.chars().filter(|&c| c != '_').collect();
    // Twenty digits hold any 64-bit value; more cannot be one.
    let number: Option<i128> = (digits.trim_start_matches('0').len() <= 20)
        .then(|| digits.parse().expect("at most twenty digits"));
    Some(number.map(|it| if negated { -it } else { it }))
}

/// Whether `text` is a simple identifier, as an enum member's name is.
fn is_identifier(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && (text.chars()).all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
