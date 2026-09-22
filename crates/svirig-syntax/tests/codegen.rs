//! Generates `src/ast/generated.rs` from `svirig.ungram`, and fails if what
//! is checked in differs. `UPDATE_EXPECT=1` writes it.
//!
//! The generated code is checked in rather than built by a build script so
//! that it can be read, reviewed as a diff, and found by go-to-definition.
//!
//! # What an accessor becomes
//!
//! A node's rule is flattened into the children it may hold. A token becomes a
//! method returning the first token of its kind, named after the token or
//! after its label. A node referenced under `*` becomes a method returning
//! every child of that type. Any other node becomes a method returning the
//! first child of that type -- unless another child could be of the same type,
//! in which case only a label names it, and the label resolves by position:
//! the `k`th of the children that could be either. That is only sound if no
//! child before it may be missing, and generation refuses a label where it is
//! not; the accessor is then written by hand in `src/ast/ext.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use expect_test::expect_file;
use svirig_syntax::{SyntaxKind, tokenize};
use ungrammar::{Grammar, Node, Rule};

#[test]
fn codegen() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("svirig.ungram")).expect("the grammar");
    let grammar: Grammar = text.parse().expect("the grammar parses");
    let code = rustfmt(&generate(&grammar));
    expect_file![root.join("src/ast/generated.rs")].assert_eq(&code);
}

// ---------------------------------------------------------------- vocabulary

fn all_kinds() -> impl Iterator<Item = SyntaxKind> {
    (0..SyntaxKind::LAST as u16).map(SyntaxKind::from_raw)
}

/// `ModuleDecl` names `MODULE_DECL`, or nothing if the rule is a union.
fn node_kind(name: &str) -> Option<SyntaxKind> {
    let mut screaming = String::new();
    for (at, c) in name.char_indices() {
        if c.is_uppercase() && at > 0 {
            screaming.push('_');
        }
        screaming.push(c.to_ascii_uppercase());
    }
    all_kinds().find(|kind| kind.is_node() && format!("{kind:?}") == screaming)
}

/// What a quoted token in the grammar stands for, or `None` for `'...'`.
fn token_kind(text: &str) -> Option<SyntaxKind> {
    if text == "..." {
        return None;
    }
    if text.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
        let kind = all_kinds()
            .find(|kind| kind.is_token() && format!("{kind:?}") == text)
            .unwrap_or_else(|| panic!("'{text}' names no token kind"));
        return Some(kind);
    }
    let tokens: Vec<SyntaxKind> = tokenize(text)
        .into_iter()
        .map(|token| token.kind)
        .filter(|&kind| kind != SyntaxKind::EOF)
        .collect();
    match tokens.as_slice() {
        [kind] => Some(*kind),
        _ => panic!("'{text}' lexes as {tokens:?}, not one token"),
    }
}

/// What a method returning a token of `kind` is called, before `_token`.
fn token_name(kind: SyntaxKind) -> String {
    let name = format!("{kind:?}").to_lowercase();
    name.strip_suffix("_kw").unwrap_or(&name).to_string()
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (at, c) in name.char_indices() {
        if c.is_uppercase() && at > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn plural(name: &str) -> String {
    if name.ends_with("ch") || name.ends_with('s') || name.ends_with('x') {
        format!("{name}es")
    } else {
        format!("{name}s")
    }
}

// ------------------------------------------------------------------- lowering

/// The node kinds a grammar name can stand for.
fn expand(grammar: &Grammar, node: Node) -> BTreeSet<SyntaxKind> {
    let data = &grammar[node];
    if let Some(kind) = node_kind(&data.name) {
        return BTreeSet::from([kind]);
    }
    union_variants(grammar, node)
        .into_iter()
        .flat_map(|variant| expand(grammar, variant))
        .collect()
}

/// The alternatives of a union, which must each be a node.
fn union_variants(grammar: &Grammar, node: Node) -> Vec<Node> {
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

#[derive(Debug, Clone)]
enum What {
    Node(Node),
    Tokens(Vec<SyntaxKind>),
}

/// One place in a rule where a child may stand.
#[derive(Debug, Clone)]
struct Occurrence {
    label: Option<String>,
    what: What,
    /// Under `*`.
    repeated: bool,
    /// Under `?` or one arm of `|`: it may be absent.
    optional: bool,
}

fn occurrences(grammar: &Grammar, rule: &Rule, out: &mut Vec<Occurrence>) {
    fn walk(
        grammar: &Grammar,
        rule: &Rule,
        label: Option<&str>,
        repeated: bool,
        optional: bool,
        out: &mut Vec<Occurrence>,
    ) {
        let occurrence = |what| Occurrence {
            label: label.map(str::to_string),
            what,
            repeated,
            optional,
        };
        match rule {
            Rule::Labeled { label, rule } => {
                walk(grammar, rule, Some(label), repeated, optional, out);
            }
            Rule::Node(node) => out.push(occurrence(What::Node(*node))),
            Rule::Token(token) => {
                if let Some(kind) = token_kind(&grammar[*token].name) {
                    out.push(occurrence(What::Tokens(vec![kind])));
                }
            }
            // A label over a choice of tokens is one accessor for any of them.
            Rule::Alt(arms) if label.is_some() && arms.iter().all(is_token) => {
                let kinds = arms
                    .iter()
                    .filter_map(|arm| match arm {
                        Rule::Token(token) => token_kind(&grammar[*token].name),
                        _ => None,
                    })
                    .collect();
                out.push(occurrence(What::Tokens(kinds)));
            }
            Rule::Seq(rules) => {
                for rule in rules {
                    walk(grammar, rule, None, repeated, optional, out);
                }
            }
            Rule::Alt(arms) => {
                for arm in arms {
                    walk(grammar, arm, None, repeated, true, out);
                }
            }
            // `label:X?` labels the X.
            Rule::Opt(rule) => walk(grammar, rule, label, repeated, true, out),
            Rule::Rep(rule) => walk(grammar, rule, None, true, true, out),
        }
        if label.is_some()
            && !matches!(
                rule,
                Rule::Labeled { .. } | Rule::Node(_) | Rule::Token(_) | Rule::Alt(_) | Rule::Opt(_)
            )
        {
            panic!("a label may name a node or tokens, not {rule:?}");
        }
    }

    fn is_token(rule: &Rule) -> bool {
        matches!(rule, Rule::Token(_))
    }

    walk(grammar, rule, None, false, false, out);
}

/// A node field: every labeled occurrence is one, and the unlabeled
/// occurrences of one type are one between them.
struct Field {
    name: String,
    ty: Node,
    labeled: bool,
    count: usize,
    repeated: bool,
    optional: bool,
    kinds: BTreeSet<SyntaxKind>,
}

/// The methods of a node's view.
fn accessors(grammar: &Grammar, node: Node) -> Vec<String> {
    let data = &grammar[node];
    let mut found = Vec::new();
    occurrences(grammar, &data.rule, &mut found);

    let mut methods = Vec::new();
    let mut named = BTreeSet::new();

    for occurrence in &found {
        let What::Tokens(kinds) = &occurrence.what else {
            continue;
        };
        // Unlabeled, a token under `*` is a separator or one of a run, and a
        // class of tokens ('IDENT') may stand for several things in one node.
        let name = match (&occurrence.label, kinds.as_slice()) {
            (Some(label), _) => label.clone(),
            (None, [kind]) if !occurrence.repeated && is_spelled(*kind) => {
                format!("{}_token", token_name(*kind))
            }
            (None, _) => continue,
        };
        if named.insert(name.clone()) {
            let kinds: Vec<String> = kinds.iter().map(|kind| format!("{kind:?}")).collect();
            methods.push(format!(
                "pub fn {name}(&self) -> Option<SyntaxToken> {{ support::token(&self.syntax, &[{}]) }}",
                kinds.join(", ")
            ));
        }
    }

    let mut fields: Vec<Field> = Vec::new();
    for occurrence in &found {
        let What::Node(ty) = occurrence.what else {
            continue;
        };
        let existing = match &occurrence.label {
            Some(_) => None,
            None => fields
                .iter_mut()
                .find(|field| !field.labeled && field.ty == ty),
        };
        match existing {
            Some(field) => {
                field.count += 1;
                field.repeated |= occurrence.repeated;
                field.optional |= occurrence.optional;
            }
            None => fields.push(Field {
                name: occurrence
                    .label
                    .clone()
                    .unwrap_or_else(|| snake(&grammar[ty].name)),
                ty,
                labeled: occurrence.label.is_some(),
                count: 1,
                repeated: occurrence.repeated,
                optional: occurrence.optional,
                kinds: expand(grammar, ty),
            }),
        }
    }

    for (at, field) in fields.iter().enumerate() {
        let ty = &grammar[field.ty].name;
        if field.repeated {
            assert!(
                !field.labeled,
                "{}.{}: a label cannot name a repeated node",
                data.name, field.name
            );
            let name = plural(&field.name);
            if named.insert(name.clone()) {
                methods.push(format!(
                    "pub fn {name}(&self) -> AstChildren<{ty}> {{ support::children(&self.syntax) }}"
                ));
            }
            continue;
        }
        // Two unlabeled places for one type, and nothing to tell them apart.
        if field.count > 1 {
            continue;
        }

        let overlapping: Vec<(usize, &Field)> = fields
            .iter()
            .enumerate()
            .filter(|(_, other)| !other.kinds.is_disjoint(&field.kinds))
            .collect();
        let body = if overlapping.len() == 1 {
            "support::child(&self.syntax)".to_string()
        } else if field.labeled {
            let position = overlapping
                .iter()
                .position(|&(other, _)| other == at)
                .expect("a field overlaps itself");
            let sound = overlapping
                .iter()
                .all(|(_, other)| !other.repeated && other.count == 1)
                && overlapping[..position]
                    .iter()
                    .all(|(_, other)| !other.optional)
                && (!field.optional || position == overlapping.len() - 1);
            assert!(
                sound,
                "{}.{}: which child this is cannot be told by position; \
                 remove the label and write the accessor by hand",
                data.name, field.name
            );
            let among: BTreeSet<&str> = overlapping
                .iter()
                .map(|(_, other)| grammar[other.ty].name.as_str())
                .collect();
            let among: Vec<&str> = among.into_iter().collect();
            let among = match among.as_slice() {
                [only] => format!("{only}::can_cast"),
                many => format!(
                    "|kind| {}",
                    many.iter()
                        .map(|ty| format!("{ty}::can_cast(kind)"))
                        .collect::<Vec<_>>()
                        .join(" || ")
                ),
            };
            format!("support::nth(&self.syntax, {position}, {among})")
        } else {
            continue;
        };
        if named.insert(field.name.clone()) {
            methods.push(format!(
                "pub fn {}(&self) -> Option<{ty}> {{ {body} }}",
                field.name
            ));
        }
    }

    methods
}

/// A keyword or punctuation, as against a class of tokens like `IDENT`.
fn is_spelled(kind: SyntaxKind) -> bool {
    use SyntaxKind::*;
    !matches!(
        kind,
        IDENT | ESCAPED_IDENT | SYSTEM_IDENT | TICK_IDENT | INT_LITERAL | STRING_LITERAL
    )
}

// ----------------------------------------------------------------- generation

fn generate(grammar: &Grammar) -> String {
    let mut nodes = BTreeMap::new();
    let mut unions = BTreeMap::new();
    for node in grammar.iter() {
        let name = grammar[node].name.clone();
        match node_kind(&name) {
            Some(kind) => {
                nodes.insert(name, (node, kind));
            }
            None => {
                unions.insert(name, node);
            }
        }
    }

    // Every node kind has a rule, so that no kind is left without a view.
    let described: BTreeSet<SyntaxKind> = nodes.values().map(|&(_, kind)| kind).collect();
    let missing: Vec<String> = all_kinds()
        .filter(|kind| kind.is_node() && !described.contains(kind))
        .map(|kind| format!("{kind:?}"))
        .collect();
    assert!(missing.is_empty(), "no rule for {}", missing.join(", "));

    let mut out = String::new();
    out.push_str(
        "//! Generated from `svirig.ungram` by `tests/codegen.rs`; do not edit.\n\
         //! `UPDATE_EXPECT=1 cargo nextest run -p svirig-syntax codegen` rewrites it.\n\n\
         use super::{AstChildren, AstNode, support};\n\
         use crate::SyntaxKind::{self, *};\n\
         use crate::{SyntaxNode, SyntaxToken};\n\n",
    );

    for (name, &(node, kind)) in &nodes {
        writeln!(
            out,
            "/// A `{kind:?}` node.\n\
             #[derive(Debug, Clone, PartialEq, Eq, Hash)]\n\
             pub struct {name} {{ syntax: SyntaxNode }}\n\n\
             impl AstNode for {name} {{\n\
                 fn can_cast(kind: SyntaxKind) -> bool {{ kind == {kind:?} }}\n\
                 fn cast(syntax: SyntaxNode) -> Option<Self> {{\n\
                     Self::can_cast(syntax.kind()).then_some({name} {{ syntax }})\n\
                 }}\n\
                 fn syntax(&self) -> &SyntaxNode {{ &self.syntax }}\n\
             }}\n"
        )
        .expect("writing to a string");

        let methods = accessors(grammar, node);
        if !methods.is_empty() {
            writeln!(out, "impl {name} {{").expect("writing to a string");
            for method in methods {
                writeln!(out, "{method}").expect("writing to a string");
            }
            writeln!(out, "}}\n").expect("writing to a string");
        }
    }

    for (name, &node) in &unions {
        let variants = union_variants(grammar, node);
        let variant_names: Vec<&str> = variants.iter().map(|&v| grammar[v].name.as_str()).collect();
        let (direct, nested): (Vec<Node>, Vec<Node>) = variants
            .iter()
            .partition(|&&variant| node_kind(&grammar[variant].name).is_some());

        writeln!(
            out,
            "/// Any of {}.\n#[derive(Debug, Clone, PartialEq, Eq, Hash)]\npub enum {name} {{ {} }}\n",
            variant_names
                .iter()
                .map(|variant| format!("[`{variant}`]"))
                .collect::<Vec<_>>()
                .join(", "),
            variant_names
                .iter()
                .map(|variant| format!("{variant}({variant})"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .expect("writing to a string");

        let direct_kinds: Vec<String> = direct
            .iter()
            .map(|&variant| {
                format!(
                    "{:?}",
                    node_kind(&grammar[variant].name).expect("a node kind")
                )
            })
            .collect();
        let mut can_cast = Vec::new();
        if !direct_kinds.is_empty() {
            can_cast.push(format!("matches!(kind, {})", direct_kinds.join(" | ")));
        }
        for &variant in &nested {
            can_cast.push(format!("{}::can_cast(kind)", grammar[variant].name));
        }

        let mut arms = String::new();
        for &variant in &direct {
            let variant = &grammar[variant].name;
            let kind = node_kind(variant).expect("a node kind");
            writeln!(
                arms,
                "{kind:?} => Some({name}::{variant}({variant} {{ syntax }})),"
            )
            .expect("writing to a string");
        }
        for &variant in &nested {
            let variant = &grammar[variant].name;
            writeln!(arms, "kind if {variant}::can_cast(kind) => {variant}::cast(syntax).map({name}::{variant}),")
                .expect("writing to a string");
        }
        arms.push_str("_ => None,\n");

        let syntax_arms: String = variant_names
            .iter()
            .map(|variant| format!("{name}::{variant}(it) => it.syntax(),\n"))
            .collect();

        writeln!(
            out,
            "impl AstNode for {name} {{\n\
                 fn can_cast(kind: SyntaxKind) -> bool {{ {} }}\n\
                 fn cast(syntax: SyntaxNode) -> Option<Self> {{\n\
                     match syntax.kind() {{\n{arms}}}\n\
                 }}\n\
                 fn syntax(&self) -> &SyntaxNode {{\n\
                     match self {{\n{syntax_arms}}}\n\
                 }}\n\
             }}\n",
            can_cast.join(" || ")
        )
        .expect("writing to a string");
    }

    out
}

fn rustfmt(code: &str) -> String {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("rustfmt runs");
    child
        .stdin
        .take()
        .expect("a stdin")
        .write_all(code.as_bytes())
        .expect("writing to rustfmt");
    let output = child.wait_with_output().expect("rustfmt finishes");
    assert!(
        output.status.success(),
        "rustfmt rejected the generated code"
    );
    String::from_utf8(output.stdout).expect("rustfmt writes UTF-8")
}
