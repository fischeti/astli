//! What a project may set of a rule beyond its level: the styles the
//! naming rules hold names to.

use std::fmt;

/// Something a project may set of a rule, `[lint.rules.<rule>] <key>`.
#[derive(Debug)]
pub struct RuleOption {
    pub key: &'static str,
    pub kind: OptionKind,
    /// What it is unless set, as configuration spells it.
    pub default: &'static [&'static str],
    /// What it says, in a phrase.
    pub summary: &'static str,
}

/// What an option holds: a list of one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    /// The cases a name may be in, any one of them.
    Cases,
    /// The patterns a name may match, any one of them.
    Patterns,
}

/// An option's value, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Cases(Vec<Case>),
    Patterns(Vec<Pattern>),
}

impl Value {
    /// `values` as `kind` holds them.
    pub(crate) fn parse<S: AsRef<str>>(kind: OptionKind, values: &[S]) -> Result<Value, String> {
        if values.is_empty() {
            return Err("names nothing, so that no name passes".to_string());
        }
        let values = values.iter().map(|it| it.as_ref());
        match kind {
            OptionKind::Cases => values
                .map(Case::named)
                .collect::<Result<_, _>>()
                .map(Value::Cases),
            OptionKind::Patterns => (values.map(Pattern::parse))
                .collect::<Result<_, _>>()
                .map(Value::Patterns),
        }
    }
}

/// A case a name is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    /// Lower-case letters, digits and underscores.
    LowerSnake,
    /// Upper-case letters, digits and underscores, as C constants are.
    AllCaps,
    /// Letters and digits, starting with an upper-case letter or a digit.
    Camel,
    /// Letters and digits, starting with a lower-case letter.
    LowerCamel,
}

impl Case {
    pub const ALL: [Case; 4] = [
        Case::LowerSnake,
        Case::AllCaps,
        Case::Camel,
        Case::LowerCamel,
    ];

    /// The case called `name`, as configuration spells it.
    pub fn named(name: &str) -> Result<Case, String> {
        Case::ALL
            .into_iter()
            .find(|case| case.name() == name)
            .ok_or_else(|| {
                let known: Vec<String> = Case::ALL.iter().map(|it| format!("`{it}`")).collect();
                format!("`{name}` is not a case; one is {}", either(&known, "or"))
            })
    }

    pub fn name(self) -> &'static str {
        match self {
            Case::LowerSnake => "lower_snake_case",
            Case::AllCaps => "ALL_CAPS",
            Case::Camel => "CamelCase",
            Case::LowerCamel => "camelCase",
        }
    }

    /// Whether `name` is written in it.
    pub fn holds(self, name: &str) -> bool {
        match self {
            Case::LowerSnake => {
                !name.is_empty()
                    && (name.bytes())
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            }
            Case::AllCaps => {
                !name.is_empty()
                    && (name.bytes())
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            }
            Case::Camel => camel(name, |b| b.is_ascii_uppercase() || b.is_ascii_digit()),
            Case::LowerCamel => camel(name, |b| b.is_ascii_lowercase()),
        }
    }
}

impl fmt::Display for Case {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Letters and digits, the first as `first` says, so that `AXI4Lite` is
/// `CamelCase` and `Num_Ports` is not. It may end in an underscore and
/// digits, as in `InitHash_256`.
fn camel(name: &str, first: fn(u8) -> bool) -> bool {
    let stem = match name.rsplit_once('_') {
        Some((stem, digits))
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            stem
        }
        _ => name,
    };
    let mut bytes = stem.bytes();
    bytes.next().is_some_and(first) && bytes.all(|b| b.is_ascii_alphanumeric())
}

/// A name with a prefix, a suffix or both around a stem of at least one
/// character, written `gen_*` or `*_i`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    prefix: Box<str>,
    suffix: Box<str>,
}

impl Pattern {
    pub fn parse(text: &str) -> Result<Pattern, String> {
        match text.split_once('*') {
            Some((prefix, suffix)) if !suffix.contains('*') => Ok(Pattern {
                prefix: prefix.into(),
                suffix: suffix.into(),
            }),
            _ => Err(format!(
                "`{text}` is not a pattern, which has one `*` for the rest of the name"
            )),
        }
    }

    /// What `*` stands for in `name`, if `name` matches.
    pub fn stem<'n>(&self, name: &'n str) -> Option<&'n str> {
        let stem = name
            .strip_prefix(&*self.prefix)?
            .strip_suffix(&*self.suffix)?;
        (!stem.is_empty()).then_some(stem)
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}*{}", self.prefix, self.suffix)
    }
}

/// How a rule wants a name: matching one of `patterns`, with what the `*`
/// stands for in one of `cases`. No cases means any.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Style<'a> {
    pub cases: &'a [Case],
    pub patterns: &'a [Pattern],
}

impl Style<'_> {
    /// Whether `name` is in the style.
    pub fn holds(&self, name: &str) -> bool {
        self.stems(name).any(|stem| self.cased(stem))
    }

    /// What `*` stands for in each pattern `name` matches.
    pub fn stems<'n>(&self, name: &'n str) -> impl Iterator<Item = &'n str> {
        self.patterns.iter().filter_map(move |it| it.stem(name))
    }

    /// Whether `stem` is in one of the cases.
    pub fn cased(&self, stem: &str) -> bool {
        self.cases.is_empty() || self.cases.iter().any(|case| case.holds(stem))
    }

    /// What a name not in the style is not, after the name: `is not
    /// lower_snake_case ending in `_t``, `does not start with `gen_``.
    pub fn describe(&self) -> String {
        let quoted = |it: &dyn fmt::Display| format!("`{it}`");
        let listed = |each: &dyn Fn(&Pattern) -> String| {
            let all: Vec<String> = self.patterns.iter().map(each).collect();
            either(&all, "or")
        };
        let bare = |it: &str| it.is_empty();
        // What the patterns ask, as a verb after `does not` and as one after
        // a case.
        let affixes = if self
            .patterns
            .iter()
            .all(|it| bare(&it.prefix) && bare(&it.suffix))
        {
            None
        } else if self.patterns.iter().all(|it| bare(&it.prefix)) {
            Some(("end in", "ending in", listed(&|it| quoted(&it.suffix))))
        } else if self.patterns.iter().all(|it| bare(&it.suffix)) {
            Some((
                "start with",
                "starting with",
                listed(&|it| quoted(&it.prefix)),
            ))
        } else {
            Some(("match", "matching", listed(&|it| quoted(it))))
        };
        let cases: Vec<String> = self.cases.iter().map(ToString::to_string).collect();
        let is = match cases.as_slice() {
            [] => None,
            [case] => Some(format!("is not {case}")),
            [first, second] => Some(format!("is neither {first} nor {second}")),
            more => Some(format!("is none of {}", either(more, "or"))),
        };
        match (is, affixes) {
            (Some(is), Some((_, after_case, list))) => format!("{is} {after_case} {list}"),
            (Some(is), None) => is,
            (None, Some((verb, _, list))) => format!("does not {verb} {list}"),
            // Every name of at least one character passes.
            (None, None) => "is empty".to_string(),
        }
    }
}

/// `items` joined as a sentence lists alternatives: `a`, `a or b`, `a, b or
/// c`.
pub(crate) fn either(items: &[String], or: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} {or} {last}", init.join(", ")),
    }
}

/// An option a project set that the rule does not have, or a value it does
/// not take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionError(pub String);

impl fmt::Display for OptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OptionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(cases: &[&str], patterns: &[&str]) -> (Vec<Case>, Vec<Pattern>) {
        let cases = cases.iter().map(|it| Case::named(it).unwrap()).collect();
        let patterns = patterns
            .iter()
            .map(|it| Pattern::parse(it).unwrap())
            .collect();
        (cases, patterns)
    }

    #[test]
    fn a_style_holds_names_by_their_stem() {
        let (cases, patterns) = style(&["lower_snake_case"], &["i_*", "*_i"]);
        let style = Style {
            cases: &cases,
            patterns: &patterns,
        };
        assert!(style.holds("i_data"));
        assert!(style.holds("data_i"));
        assert!(!style.holds("i_Data"));
        assert!(!style.holds("data"));
        // The stem is never empty.
        assert!(!style.holds("i_"));
    }

    #[test]
    fn a_style_says_what_a_name_is_not() {
        let cases = [
            (
                &["lower_snake_case"][..],
                &["*"][..],
                "is not lower_snake_case",
            ),
            (
                &["CamelCase", "ALL_CAPS"],
                &["*"],
                "is neither CamelCase nor ALL_CAPS",
            ),
            (
                &["lower_snake_case"],
                &["*_e", "*_t"],
                "is not lower_snake_case ending in `_e` or `_t`",
            ),
            (&[], &["gen_*", "g_*"], "does not start with `gen_` or `g_`"),
            (&[], &["*_i", "*_ni"], "does not end in `_i` or `_ni`"),
            (&[], &["i_*", "*_i"], "does not match `i_*` or `*_i`"),
            (
                &["lower_snake_case", "camelCase", "ALL_CAPS"],
                &["u_*"],
                "is none of lower_snake_case, camelCase or ALL_CAPS starting with `u_`",
            ),
        ];
        for (named, written, expected) in cases {
            let (cases, patterns) = style(named, written);
            let style = Style {
                cases: &cases,
                patterns: &patterns,
            };
            assert_eq!(style.describe(), expected);
        }
    }

    #[test]
    fn a_pattern_has_one_star() {
        assert!(Pattern::parse("gen_").is_err());
        assert!(Pattern::parse("*_*").is_err());
        assert_eq!(Pattern::parse("*_i").unwrap().to_string(), "*_i");
    }

    #[test]
    fn cases() {
        assert!(Case::Camel.holds("AXI4Lite"));
        assert!(Case::Camel.holds("InitHash_256"));
        assert!(!Case::Camel.holds("numPorts"));
        assert!(Case::LowerCamel.holds("numPorts"));
        assert!(!Case::LowerCamel.holds("NumPorts"));
        assert!(Case::named("snake").is_err());
    }
}
