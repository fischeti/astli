//! Holding a tree to `astli.ungram`.
//!
//! Each node's children must be what its rule names, in that order. Only child
//! *nodes* are checked: tokens are erased from both the rule and the node,
//! because several rules take tokens loosely on purpose. What this catches is
//! the thing a round-trip cannot: a node nested where it does not belong, one
//! that is missing, or one too many.

// Each test binary compiles its own copy of this module, so a helper only one
// of them needs looks unused to the others.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use astli_syntax::{SyntaxKind, SyntaxNode};
use ungrammar::{Grammar, Node, Rule};

/// A rule with its tokens erased, over node kinds.
enum Pattern {
    Empty,
    Kinds(BTreeSet<SyntaxKind>),
    Seq(Vec<Pattern>),
    Alt(Vec<Pattern>),
    Opt(Box<Pattern>),
    Rep(Box<Pattern>),
}

pub struct Shapes {
    rules: HashMap<SyntaxKind, Pattern>,
}

/// Where a node did not match its rule, collected by the node's kind and the
/// kinds of its children.
#[derive(Default)]
pub struct Mismatches {
    pub found: BTreeMap<(SyntaxKind, Vec<SyntaxKind>), (usize, String)>,
}

impl Mismatches {
    pub fn total(&self) -> usize {
        self.found.values().map(|(count, _)| count).sum()
    }

    /// The most frequent shapes first, one line each.
    pub fn report(&self, limit: usize) -> String {
        let mut rows: Vec<_> = self.found.iter().collect();
        rows.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
        rows.iter()
            .take(limit)
            .map(|((kind, children), (count, first))| {
                format!("{count:6}  {kind:?} over {children:?}, first in {first}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Shapes {
    pub fn load() -> Shapes {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../astli-syntax/astli.ungram");
        let text = std::fs::read_to_string(path).expect("the grammar");
        let grammar: Grammar = text.parse().expect("the grammar parses");

        let mut rules = HashMap::new();
        for node in grammar.iter() {
            if let Some(kind) = node_kind(&grammar[node].name) {
                rules.insert(kind, compile(&grammar, &grammar[node].rule));
            }
        }
        Shapes { rules }
    }

    /// Checks `root` and every node under it.
    pub fn check(&self, root: &SyntaxNode, place: &str, out: &mut Mismatches) {
        for node in root.descendants() {
            let children: Vec<SyntaxKind> = node.children().map(|child| child.kind()).collect();
            let rule = self
                .rules
                .get(&node.kind())
                .unwrap_or_else(|| panic!("no rule for {:?}", node.kind()));
            if !ends(rule, &children, 0).contains(&children.len()) {
                let entry = out
                    .found
                    .entry((node.kind(), children))
                    .or_insert_with(|| (0, place.to_string()));
                entry.0 += 1;
            }
        }
    }
}

fn node_kind(name: &str) -> Option<SyntaxKind> {
    let mut screaming = String::new();
    for (at, c) in name.char_indices() {
        if c.is_uppercase() && at > 0 {
            screaming.push('_');
        }
        screaming.push(c.to_ascii_uppercase());
    }
    (0..SyntaxKind::LAST as u16)
        .map(SyntaxKind::from_raw)
        .find(|kind| kind.is_node() && format!("{kind:?}") == screaming)
}

/// The kinds a grammar name stands for: itself, or a union's members.
fn expand(grammar: &Grammar, node: Node) -> BTreeSet<SyntaxKind> {
    if let Some(kind) = node_kind(&grammar[node].name) {
        return BTreeSet::from([kind]);
    }
    match &grammar[node].rule {
        Rule::Alt(arms) => arms
            .iter()
            .flat_map(|arm| match arm {
                Rule::Node(node) => expand(grammar, *node),
                _ => panic!("the union {} may only list nodes", grammar[node].name),
            })
            .collect(),
        Rule::Node(inner) => expand(grammar, *inner),
        _ => panic!("the union {} may only list nodes", grammar[node].name),
    }
}

fn compile(grammar: &Grammar, rule: &Rule) -> Pattern {
    match rule {
        Rule::Labeled { rule, .. } => compile(grammar, rule),
        Rule::Node(node) => Pattern::Kinds(expand(grammar, *node)),
        Rule::Token(_) => Pattern::Empty,
        Rule::Seq(rules) => Pattern::Seq(rules.iter().map(|rule| compile(grammar, rule)).collect()),
        Rule::Alt(arms) => Pattern::Alt(arms.iter().map(|arm| compile(grammar, arm)).collect()),
        Rule::Opt(rule) => Pattern::Opt(Box::new(compile(grammar, rule))),
        Rule::Rep(rule) => Pattern::Rep(Box::new(compile(grammar, rule))),
    }
}

/// Every position `pattern` can end at when it starts matching `children` at
/// `at`. Sets rather than backtracking, so a long body costs a pass, not a
/// search.
fn ends(pattern: &Pattern, children: &[SyntaxKind], at: usize) -> BTreeSet<usize> {
    match pattern {
        Pattern::Empty => BTreeSet::from([at]),
        Pattern::Kinds(kinds) => match children.get(at) {
            Some(kind) if kinds.contains(kind) => BTreeSet::from([at + 1]),
            _ => BTreeSet::new(),
        },
        Pattern::Seq(parts) => parts.iter().fold(BTreeSet::from([at]), |starts, part| {
            starts
                .into_iter()
                .flat_map(|start| ends(part, children, start))
                .collect()
        }),
        Pattern::Alt(arms) => arms
            .iter()
            .flat_map(|arm| ends(arm, children, at))
            .collect(),
        Pattern::Opt(inner) => {
            let mut out = ends(inner, children, at);
            out.insert(at);
            out
        }
        Pattern::Rep(inner) => {
            let mut reached = BTreeSet::from([at]);
            let mut frontier = vec![at];
            while let Some(start) = frontier.pop() {
                for end in ends(inner, children, start) {
                    if reached.insert(end) {
                        frontier.push(end);
                    }
                }
            }
            reached
        }
    }
}
