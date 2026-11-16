//! Reading `astli.ungram`, for the two tests that hold the tree to it.

use std::collections::BTreeSet;

use astli_syntax::SyntaxKind;
use ungrammar::{Grammar, Node, Rule};

/// The grammar, read from where the crate keeps it, whichever crate's tests
/// include this file.
pub fn load() -> Grammar {
    include_str!("../../astli.ungram")
        .parse()
        .expect("the grammar parses")
}

pub fn all_kinds() -> impl Iterator<Item = SyntaxKind> {
    (0..SyntaxKind::LAST as u16).map(SyntaxKind::from_raw)
}

/// `ModuleDecl` names `MODULE_DECL`, or nothing if the rule is a union.
pub fn node_kind(name: &str) -> Option<SyntaxKind> {
    let screaming = snake(name).to_uppercase();
    all_kinds().find(|kind| kind.is_node() && format!("{kind:?}") == screaming)
}

pub fn snake(name: &str) -> String {
    let mut out = String::new();
    for (at, c) in name.char_indices() {
        if c.is_uppercase() && at > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// The alternatives of a union, which must each be a node.
pub fn union_variants(grammar: &Grammar, node: Node) -> Vec<Node> {
    let data = &grammar[node];
    let alternatives = match &data.rule {
        Rule::Alt(alternatives) => alternatives.iter().collect(),
        rule => vec![rule],
    };
    alternatives
        .into_iter()
        .map(|rule| match rule {
            Rule::Node(node) => *node,
            _ => panic!("the union {} may only list nodes", data.name),
        })
        .collect()
}

/// The node kinds a grammar name can stand for.
pub fn expand(grammar: &Grammar, node: Node) -> BTreeSet<SyntaxKind> {
    match node_kind(&grammar[node].name) {
        Some(kind) => BTreeSet::from([kind]),
        None => union_variants(grammar, node)
            .into_iter()
            .flat_map(|variant| expand(grammar, variant))
            .collect(),
    }
}
