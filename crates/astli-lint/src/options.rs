//! What a project may set of a rule beyond its level: the styles the
//! naming rules hold names to, the names the unused rules leave alone, and
//! what a restriction lets through.

use std::fmt;

use crate::Rule;

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

/// What an option holds: a list of one of these, or a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    /// The cases a name may be in, any one of them.
    Cases,
    /// The patterns a name may match, any one of them.
    Patterns,
    /// The patterns of the names a rule leaves alone, any one of them,
    /// which may be none.
    Ignored,
    /// Whether a rule lets something through: one value, `true` or
    /// `false`.
    Flag,
}

/// An option's value, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Cases(Vec<Case>),
    Patterns(Vec<Pattern>),
    Flag(bool),
}

impl Value {
    /// `values` as `kind` holds them.
    pub(crate) fn parse<S: AsRef<str>>(kind: OptionKind, values: &[S]) -> Result<Value, String> {
        if kind == OptionKind::Flag {
            return match values.iter().map(|it| it.as_ref()).collect::<Vec<_>>()[..] {
                ["true"] => Ok(Value::Flag(true)),
                ["false"] => Ok(Value::Flag(false)),
                _ => Err("is `true` or `false`".to_string()),
            };
        }
        if values.is_empty() && kind != OptionKind::Ignored {
            return Err("names nothing, so that no name passes".to_string());
        }
        let values = values.iter().map(|it| it.as_ref());
        match kind {
            OptionKind::Cases => values
                .map(Case::named)
                .collect::<Result<_, _>>()
                .map(Value::Cases),
            OptionKind::Patterns | OptionKind::Ignored => (values.map(Pattern::parse))
                .collect::<Result<_, _>>()
                .map(Value::Patterns),
            OptionKind::Flag => unreachable!("a flag is parsed above"),
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

/// A name with fixed text around and between parts each `*` stands for,
/// written `gen_*`, `*_i` or `*unused*`. A `*` may stand for nothing, but
/// the stem, what lies between the text the pattern starts and ends with,
/// is at least one character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The text around each `*`, in order: one more than there are `*`s.
    pieces: Box<[Box<str>]>,
}

impl Pattern {
    pub fn parse(text: &str) -> Result<Pattern, String> {
        if !text.contains('*') {
            return Err(format!(
                "`{text}` is not a pattern, which has a `*` for the rest of the name"
            ));
        }
        Ok(Pattern {
            pieces: text.split('*').map(Into::into).collect(),
        })
    }

    /// The stem of `name`, if `name` matches.
    pub fn stem<'n>(&self, name: &'n str) -> Option<&'n str> {
        let (first, rest) = self.pieces.split_first()?;
        let (last, between) = rest.split_last()?;
        let stem = name.strip_prefix(&**first)?.strip_suffix(&**last)?;
        // Each piece found leftmost after the one before leaves the most
        // room for the rest, so no other placing matches where this fails.
        let mut left = stem;
        for piece in between {
            let at = left.find(&**piece)?;
            left = &left[at + piece.len()..];
        }
        (!stem.is_empty()).then_some(stem)
    }

    /// Whether `name` matches, in whatever case either is written.
    pub fn matches_ignoring_case(&self, name: &str) -> bool {
        let lower = Pattern {
            pieces: self
                .pieces
                .iter()
                .map(|it| it.to_lowercase().into())
                .collect(),
        };
        lower.stem(&name.to_lowercase()).is_some()
    }

    /// The text before and after the one `*`, if the pattern has one.
    fn affixes(&self) -> Option<(&str, &str)> {
        match &*self.pieces {
            [prefix, suffix] => Some((prefix, suffix)),
            _ => None,
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.pieces.join("*"))
    }
}

/// The values of a rule's options, in the order it declares them.
#[derive(Clone, Copy)]
pub(crate) struct Options<'a> {
    pub rule: &'static Rule,
    pub values: &'a [Value],
}

impl<'a> Options<'a> {
    /// The patterns the option `key` holds.
    pub fn patterns(&self, key: &str) -> &'a [Pattern] {
        match self.value(key) {
            Value::Patterns(patterns) => patterns,
            _ => panic!("`{key}` of `{}` holds no patterns", self.rule.name),
        }
    }

    /// Whether the flag `key` is set.
    pub fn flag(&self, key: &str) -> bool {
        match self.value(key) {
            Value::Flag(flag) => *flag,
            _ => panic!("`{key}` of `{}` is no flag", self.rule.name),
        }
    }

    /// The style a naming rule's `case` and `patterns` say.
    pub fn style(&self) -> Style<'a> {
        let cases: &'a [Case] = match self.value("case") {
            Value::Cases(cases) => cases,
            _ => panic!("`case` of `{}` holds no cases", self.rule.name),
        };
        Style {
            cases,
            patterns: self.patterns("patterns"),
        }
    }

    fn value(&self, key: &str) -> &'a Value {
        let at = (self.rule.options.iter())
            .position(|it| it.key == key)
            .unwrap_or_else(|| panic!("`{}` declares no option `{key}`", self.rule.name));
        &self.values[at]
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
        // Of patterns with one `*` each, the text before and after it.
        let affixes: Option<Vec<(&str, &str)>> =
            self.patterns.iter().map(Pattern::affixes).collect();
        let all = |bare: fn(&(&str, &str)) -> bool| {
            affixes.as_ref().is_some_and(|it| it.iter().all(bare))
        };
        // What the patterns ask, as a verb after `does not` and as one after
        // a case.
        let affixes = if all(|(prefix, suffix)| prefix.is_empty() && suffix.is_empty()) {
            None
        } else if all(|(prefix, _)| prefix.is_empty()) {
            Some((
                "end in",
                "ending in",
                listed(&|it| quoted(&it.affixes().unwrap().1)),
            ))
        } else if all(|(_, suffix)| suffix.is_empty()) {
            Some((
                "start with",
                "starting with",
                listed(&|it| quoted(&it.affixes().unwrap().0)),
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
            (&[], &["*_if_*"], "does not match `*_if_*`"),
            (&[], &["*_i", "u_*_i"], "does not match `*_i` or `u_*_i`"),
            (&["ALL_CAPS"], &["**"], "is not ALL_CAPS matching `**`"),
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
    fn a_pattern_has_a_star() {
        assert!(Pattern::parse("gen_").is_err());
        assert_eq!(Pattern::parse("*_i").unwrap().to_string(), "*_i");
        assert_eq!(Pattern::parse("a**b").unwrap().to_string(), "a**b");
    }

    #[test]
    fn a_pattern_matches_a_stem_of_at_least_one_character() {
        let rows = [
            ("*", "a", Some("a")),
            ("*", "", None),
            ("**", "a", Some("a")),
            ("**", "", None),
            ("gen_*", "gen_a", Some("a")),
            ("gen_*", "gen_", None),
            ("gen_*", "gen", None),
            ("*_i", "data_i", Some("data")),
            ("*_i", "data_o", None),
            ("i_*", "i_data_i", Some("data_i")),
            // The fixed text at either end is not shared.
            ("xy*yx", "xyx", None),
            ("xy*yx", "xyayx", Some("a")),
            ("a*b*c", "abc", Some("b")),
            ("a*b*c", "axbyc", Some("xby")),
            ("a*b*c", "ac", None),
            ("a*b*c", "axc", None),
            ("a*b*c", "acb", None),
            // An empty piece between two `*`s asks for nothing.
            ("a**b", "ab", None),
            ("a**b", "axb", Some("x")),
            ("*unused*", "unused", Some("unused")),
            ("*unused*", "spare_unused_q", Some("spare_unused_q")),
            ("*unused*", "unuse", None),
            // Pieces must not overlap.
            ("*aa*aa*", "aaa", None),
            ("*aa*aa*", "aaaa", Some("aaaa")),
            ("*a*a*", "xaxax", Some("xaxax")),
            ("*_nc", "x_nc_y", None),
        ];
        for (pattern, name, stem) in rows {
            let parsed = Pattern::parse(pattern).unwrap();
            assert_eq!(parsed.stem(name), stem, "`{pattern}` against `{name}`");
        }
    }

    #[test]
    fn a_pattern_may_match_ignoring_case() {
        let pattern = Pattern::parse("*Unused*").unwrap();
        assert!(pattern.matches_ignoring_case("spare_UNUSED"));
        assert!(pattern.matches_ignoring_case("unused"));
        assert!(!pattern.matches_ignoring_case("spare"));
        assert_eq!(pattern.stem("unused"), None);
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
