//! Generates `src/ast/generated.rs` from `astli.ungram`, and fails if what
//! is checked in differs. `UPDATE_EXPECT=1` writes it.
//!
//! The generated code is checked in rather than built by a build script so
//! that it can be read, reviewed as a diff, and found by go-to-definition.
//!
//! # What an accessor becomes
//!
//! A node's rule is flattened into the children it may hold. A token becomes a
//! method returning the first token of its kind, named after the token or
//! after its label. A node under `*` becomes a method returning every child of
//! that type, and any other node a method returning the one child of that
//! type. Where two children could be of one type -- the operands of `a + b` --
//! only their position tells them apart, and those accessors are written by
//! hand in `src/ast/ext.rs`.
//!
//! This test links the crate, so the checked-in file has to compile before it
//! can be regenerated. A change that removes something it uses, or that moves
//! an accessor into `ext.rs`, needs `ext.rs` emptied for one run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use astli_syntax::{SyntaxKind, tokenize};
use expect_test::expect_file;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use ungrammar::{Grammar, Node, Rule};

#[test]
fn codegen() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("astli.ungram")).expect("the grammar");
    let grammar: Grammar = text.parse().expect("the grammar parses");

    let file = syn::parse2(generate(&grammar)).expect("the generated code parses");
    let code = format!(
        "// Generated from `astli.ungram` by `tests/codegen.rs`; do not edit.\n\
         // `UPDATE_EXPECT=1 cargo nextest run -p astli-syntax codegen` rewrites it.\n\n{}",
        prettyplease::unparse(&file)
    );
    expect_file![root.join("src/ast/generated.rs")].assert_eq(&code);
}

// ---------------------------------------------------------------- vocabulary

fn all_kinds() -> impl Iterator<Item = SyntaxKind> {
    (0..SyntaxKind::LAST as u16).map(SyntaxKind::from_raw)
}

/// `ModuleDecl` names `MODULE_DECL`, or nothing if the rule is a union.
fn node_kind(name: &str) -> Option<SyntaxKind> {
    let screaming = snake(name).to_uppercase();
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

/// A keyword or punctuation, as against a class of tokens like `IDENT`, which
/// may stand for several things in one node and so needs a label.
fn is_spelled(kind: SyntaxKind) -> bool {
    use SyntaxKind::*;
    !matches!(
        kind,
        IDENT | ESCAPED_IDENT | SYSTEM_IDENT | TICK_IDENT | INT_LITERAL | STRING_LITERAL
    )
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

fn kind_ident(kind: SyntaxKind) -> Ident {
    format_ident!("{kind:?}")
}

// ------------------------------------------------------------------- lowering

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

/// The node kinds a grammar name can stand for.
fn expand(grammar: &Grammar, node: Node) -> BTreeSet<SyntaxKind> {
    match node_kind(&grammar[node].name) {
        Some(kind) => BTreeSet::from([kind]),
        None => union_variants(grammar, node)
            .into_iter()
            .flat_map(|variant| expand(grammar, variant))
            .collect(),
    }
}

enum What {
    Node(Node),
    Tokens(Vec<SyntaxKind>),
}

/// One place in a rule where a child may stand.
struct Occurrence {
    label: Option<String>,
    what: What,
    /// Under `*`.
    repeated: bool,
}

fn occurrences(
    grammar: &Grammar,
    rule: &Rule,
    label: Option<&str>,
    repeated: bool,
    out: &mut Vec<Occurrence>,
) {
    let mut push = |what| {
        out.push(Occurrence {
            label: label.map(str::to_string),
            what,
            repeated,
        })
    };
    match rule {
        Rule::Labeled { label, rule } => occurrences(grammar, rule, Some(label), repeated, out),
        Rule::Node(node) => push(What::Node(*node)),
        Rule::Token(token) => {
            if let Some(kind) = token_kind(&grammar[*token].name) {
                push(What::Tokens(vec![kind]));
            }
        }
        // A label over a choice of tokens is one accessor for any of them.
        Rule::Alt(arms) if label.is_some() => {
            let kinds = arms
                .iter()
                .filter_map(|arm| match arm {
                    Rule::Token(token) => token_kind(&grammar[*token].name),
                    _ => panic!("a label may name a node or tokens, not {arm:?}"),
                })
                .collect();
            push(What::Tokens(kinds));
        }
        Rule::Seq(rules) | Rule::Alt(rules) => {
            for rule in rules {
                occurrences(grammar, rule, None, repeated, out);
            }
        }
        // `label:X?` labels the X.
        Rule::Opt(rule) => occurrences(grammar, rule, label, repeated, out),
        Rule::Rep(rule) => occurrences(grammar, rule, None, true, out),
    }
}

/// The methods of a node's view.
fn accessors(grammar: &Grammar, node: Node) -> Vec<TokenStream> {
    let mut found = Vec::new();
    occurrences(grammar, &grammar[node].rule, None, false, &mut found);

    let mut methods = BTreeMap::new();
    for occurrence in &found {
        let What::Tokens(kinds) = &occurrence.what else {
            continue;
        };
        // Unlabeled, a token under `*` is a separator or one of a run.
        let name = match (&occurrence.label, kinds.as_slice()) {
            (Some(label), _) => label.clone(),
            (None, [kind]) if !occurrence.repeated && is_spelled(*kind) => {
                let name = format!("{kind:?}").to_lowercase();
                format!("{}_token", name.strip_suffix("_kw").unwrap_or(&name))
            }
            (None, _) => continue,
        };
        let kinds = kinds.iter().map(|&kind| kind_ident(kind));
        let ident = format_ident!("{name}");
        methods.entry(name).or_insert(quote! {
            pub fn #ident(&self) -> Option<SyntaxToken> {
                support::token(&self.syntax, &[#(#kinds),*])
            }
        });
    }

    // The node children, by the name each would get: a label, or the type's
    // name. Unlabeled occurrences of one type share a name.
    struct Field {
        ty: Node,
        count: usize,
        repeated: bool,
        kinds: BTreeSet<SyntaxKind>,
    }
    let mut fields: BTreeMap<String, Field> = BTreeMap::new();
    for occurrence in &found {
        let What::Node(ty) = occurrence.what else {
            continue;
        };
        let name = occurrence
            .label
            .clone()
            .unwrap_or_else(|| snake(&grammar[ty].name));
        let field = fields.entry(name).or_insert(Field {
            ty,
            count: 0,
            repeated: false,
            kinds: expand(grammar, ty),
        });
        field.count += 1;
        field.repeated |= occurrence.repeated;
    }

    for (name, field) in &fields {
        let ty = format_ident!("{}", grammar[field.ty].name);
        if field.repeated {
            let ident = format_ident!("{}", plural(name));
            methods.entry(plural(name)).or_insert(quote! {
                pub fn #ident(&self) -> AstChildren<#ty> {
                    support::children(&self.syntax)
                }
            });
            continue;
        }
        // Only a child no other could be mistaken for is found by its type.
        let alone = field.count == 1
            && fields
                .values()
                .filter(|other| !other.kinds.is_disjoint(&field.kinds))
                .count()
                == 1;
        if alone {
            let ident = format_ident!("{name}");
            methods.entry(name.clone()).or_insert(quote! {
                pub fn #ident(&self) -> Option<#ty> {
                    support::child(&self.syntax)
                }
            });
        }
    }

    methods.into_values().collect()
}

// ----------------------------------------------------------------- generation

fn generate(grammar: &Grammar) -> TokenStream {
    let (nodes, unions): (Vec<Node>, Vec<Node>) = grammar
        .iter()
        .partition(|&node| node_kind(&grammar[node].name).is_some());

    // Every node kind has a rule, so that none is left without a view.
    let described: BTreeSet<SyntaxKind> = nodes
        .iter()
        .filter_map(|&node| node_kind(&grammar[node].name))
        .collect();
    let missing: Vec<SyntaxKind> = all_kinds()
        .filter(|kind| kind.is_node() && !described.contains(kind))
        .collect();
    assert!(missing.is_empty(), "no rule for {missing:?}");

    let structs = nodes.iter().map(|&node| {
        let kind = node_kind(&grammar[node].name).expect("a node kind");
        let doc = format!(" A `{kind:?}` node.");
        let name = format_ident!("{}", grammar[node].name);
        let kind = kind_ident(kind);
        let methods = accessors(grammar, node);
        let methods = (!methods.is_empty()).then(|| quote! { impl #name { #(#methods)* } });
        quote! {
            #[doc = #doc]
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub struct #name {
                syntax: SyntaxNode,
            }

            impl AstNode for #name {
                fn can_cast(kind: SyntaxKind) -> bool {
                    kind == #kind
                }
                fn cast(syntax: SyntaxNode) -> Option<Self> {
                    Self::can_cast(syntax.kind()).then_some(#name { syntax })
                }
                fn syntax(&self) -> &SyntaxNode {
                    &self.syntax
                }
            }

            #methods
        }
    });

    let enums = unions.iter().map(|&node| {
        let name = format_ident!("{}", grammar[node].name);
        let variants = union_variants(grammar, node);
        let all: Vec<Ident> = variants
            .iter()
            .map(|&variant| format_ident!("{}", grammar[variant].name))
            .collect();
        let doc = format!(
            " Any of {}.",
            variants
                .iter()
                .map(|&variant| format!("[`{}`]", grammar[variant].name))
                .collect::<Vec<_>>()
                .join(", ")
        );

        // A variant that is a node is matched by its kind; one that is itself
        // a union asks that union.
        let (direct, nested): (Vec<Node>, Vec<Node>) = variants
            .iter()
            .partition(|&&variant| node_kind(&grammar[variant].name).is_some());
        let direct_kinds: Vec<Ident> = direct
            .iter()
            .map(|&variant| kind_ident(node_kind(&grammar[variant].name).expect("a node kind")))
            .collect();
        let direct: Vec<Ident> = direct
            .iter()
            .map(|&variant| format_ident!("{}", grammar[variant].name))
            .collect();
        let nested: Vec<Ident> = nested
            .iter()
            .map(|&variant| format_ident!("{}", grammar[variant].name))
            .collect();

        let direct_check = (!direct_kinds.is_empty()).then(|| quote! { matches!(kind, #(#direct_kinds)|*) });
        let can_cast = direct_check
            .into_iter()
            .chain(nested.iter().map(|variant| quote! { #variant::can_cast(kind) }))
            .reduce(|left, right| quote! { #left || #right })
            .expect("a union has variants");

        quote! {
            #[doc = #doc]
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum #name {
                #(#all(#all),)*
            }

            impl AstNode for #name {
                fn can_cast(kind: SyntaxKind) -> bool {
                    #can_cast
                }
                fn cast(syntax: SyntaxNode) -> Option<Self> {
                    match syntax.kind() {
                        #(#direct_kinds => Some(Self::#direct(#direct { syntax })),)*
                        #(kind if #nested::can_cast(kind) => #nested::cast(syntax).map(Self::#nested),)*
                        _ => None,
                    }
                }
                fn syntax(&self) -> &SyntaxNode {
                    match self {
                        #(Self::#all(it) => it.syntax(),)*
                    }
                }
            }
        }
    });

    quote! {
        use super::{AstChildren, AstNode, support};
        use crate::SyntaxKind::{self, *};
        use crate::{SyntaxNode, SyntaxToken};

        #(#structs)*
        #(#enums)*
    }
}
