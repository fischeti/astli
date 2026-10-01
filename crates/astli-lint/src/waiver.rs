//! Waivers: `(* astli_allow = "rule, group" *)` on the construct they cover.
//!
//! An attribute, rather than a comment, because it is part of the construct:
//! it covers exactly that node, however many lines it spans, and moves with
//! it when the formatter moves lines. Tools that do not know an attribute
//! ignore it.

use astli_parse::SyntaxTree;
use astli_syntax::SyntaxKind::{ATTRIBUTE_SPEC, IDENT, STRING_LITERAL};
use astli_syntax::SyntaxNode;
use std::path::PathBuf;

use astli_text::{Code, Diagnostic, Origins, Span};
use rowan::{TextRange, TextSize};

use crate::{Group, RULES, Rule};

/// The attribute that waives.
const ALLOW: &str = "astli_allow";

/// Code for a waiver that is not a string of names, or names something that
/// is neither a rule nor a group.
pub const INVALID_WAIVER: Code = Code("invalid-waiver");

/// Every waiver in a file as written: the range of the node it stands on,
/// and the rule or group names it lists.
///
/// It holds no tree, so it outlives the one it was read from: the rules that
/// read a design run after every file's tree is gone, and are waived by what
/// the file as written says.
#[derive(Debug, Clone)]
pub struct Waivers {
    path: PathBuf,
    covered: Vec<(TextRange, Vec<String>)>,
}

impl Waivers {
    /// The waivers in `tree`. What is malformed about them is what
    /// [`lint`](crate::lint) reports.
    pub fn of(tree: &SyntaxTree) -> Waivers {
        Waivers::read(tree, &mut Vec::new())
    }

    /// Reads the waivers in `tree`, reporting into `found` any that is
    /// malformed or names something that does not exist.
    pub(crate) fn read(tree: &SyntaxTree, found: &mut Vec<Diagnostic>) -> Waivers {
        // Each covered node, the spec that waives it, and the names it lists.
        let mut waivers: Vec<(SyntaxNode, TextRange, Vec<String>)> = Vec::new();
        let specs = tree
            .root()
            .descendants()
            .filter(|node| node.kind() == ATTRIBUTE_SPEC)
            .filter(|spec| is_named(spec, ALLOW));
        for spec in specs {
            let Some(covered) = spec.ancestors().nth(2) else {
                continue;
            };
            // Of two on one construct the later counts, as for any attribute.
            // The earlier is reported, since the string can list every name.
            if let Some(at) = waivers.iter().position(|(node, ..)| *node == covered) {
                let (_, earlier, _) = waivers.remove(at);
                let message = format!(
                    "a later `{ALLOW}` on the same construct replaces this one; \
                     list every name in one string"
                );
                found.push(Diagnostic::warning(
                    INVALID_WAIVER,
                    tree.span(earlier),
                    message,
                ));
            }
            let names = match value(&spec) {
                Some(value) => value,
                None => {
                    let at = tree.span(spec.text_range());
                    let message = format!("`{ALLOW}` takes a string of rule or group names");
                    found.push(Diagnostic::warning(INVALID_WAIVER, at, message));
                    // Still one, so that an earlier one is reported as replaced.
                    waivers.push((covered, spec.text_range(), Vec::new()));
                    continue;
                }
            };
            for (name, range) in &names {
                if Group::named(name).is_none() && !RULES.iter().any(|it| it.name == name) {
                    let message = format!("no lint rule or group is called `{name}`");
                    found.push(Diagnostic::warning(
                        INVALID_WAIVER,
                        tree.span(*range),
                        message,
                    ));
                }
            }
            let names = names.into_iter().map(|(name, _)| name).collect();
            waivers.push((covered, spec.text_range(), names));
        }
        Waivers {
            path: tree.path().to_path_buf(),
            covered: waivers
                .into_iter()
                .map(|(node, _, names)| (node.text_range(), names))
                .collect(),
        }
    }

    /// Whether a finding of `rule` at `range` stands inside a node that waives
    /// the rule or its group.
    pub(crate) fn cover(&self, rule: &Rule, range: TextRange) -> bool {
        self.covered.iter().any(|(covered, names)| {
            covered.contains_range(range)
                && (names.iter()).any(|name| name == rule.name || name == rule.group.name())
        })
    }
}

impl Waivers {
    /// Whether a finding of `rule` at `at`, a span `origins` resolves, is
    /// waived: where it is reported, in this file, stands inside a node that
    /// waives the rule or its group.
    pub(crate) fn cover_span(&self, rule: &Rule, origins: &Origins, at: Span) -> bool {
        let at = origins.spelled(origins.reported_at(at));
        if origins.path(at.src_id) != Some(self.path.as_path()) {
            return false;
        }
        self.cover(rule, TextRange::new(at.start.into(), at.end.into()))
    }
}

/// Whether an attribute spec is the one called `name`.
fn is_named(spec: &SyntaxNode, name: &str) -> bool {
    let token = spec
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia());
    token.is_some_and(|token| token.kind() == IDENT && token.text() == name)
}

/// The names a spec's string lists, split at commas and spaces, each with
/// its range in the file; `None` if the value is not one string, or names
/// nothing.
///
/// `astli_allow = "a, b"` gives `a` and `b`, and so do `"a b"` and `"a,,b"`.
/// `astli_allow = 1`, a bare `astli_allow` and `astli_allow = ""` give `None`.
fn value(spec: &SyntaxNode) -> Option<Vec<(String, TextRange)>> {
    let literal = spec.children().next()?;
    let mut tokens = (literal.descendants_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    let string = tokens.next().filter(|it| it.kind() == STRING_LITERAL)?;
    if tokens.next().is_some() {
        return None;
    }

    let start = string.text_range().start() + TextSize::from(1);
    let text = string.text();
    let inner = text.get(1..text.len() - 1)?;
    let names: Vec<_> = (inner.split(|c: char| c == ',' || c.is_whitespace()))
        .filter(|word| !word.is_empty())
        .map(|word| {
            // `word` borrows from `inner`, so the distance between them is
            // its offset, whatever the separators before it were.
            let at = word.as_ptr() as usize - inner.as_ptr() as usize;
            let range = TextRange::at(
                start + TextSize::from(at as u32),
                TextSize::from(word.len() as u32),
            );
            (word.to_string(), range)
        })
        .collect();
    (!names.is_empty()).then_some(names)
}
