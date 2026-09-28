//! The names in a tree that declare, close or use a top-level declaration,
//! and the tree's text with some of them replaced.

use astli_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextSize};

use crate::summary::{Declares, Uses};

/// What a name does where it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Declares a module, interface, program, package or class at the top
    /// level.
    Declares(Declares),
    /// Repeats a declaration's name after its end keyword: `core` in
    /// `endmodule : core`.
    Closes(Declares),
    /// Uses a name that means, or might mean, a top-level declaration.
    Uses(Uses),
}

/// A name written in a tree, and what it does there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name {
    pub token: SyntaxToken,
    pub role: Role,
}

impl Name {
    /// The name as the language compares it: `\cpu3 ` and `cpu3` are one
    /// name.
    pub fn text(&self) -> &str {
        let text = self.token.text();
        match text.strip_prefix('\\') {
            // The whitespace that ends an escaped name is part of its token.
            Some(escaped) => escaped.trim_end(),
            None => text,
        }
    }
}

/// Every name in the tree at `root` that declares, closes or uses a
/// top-level declaration, in the order written.
///
/// A token is named more than once if it plays more than one part, as the
/// head of a scoped name in text no rule read does.
pub fn names(root: &SyntaxNode) -> Vec<Name> {
    let mut names = Vec::new();
    let mut push = |token: SyntaxToken, role| names.push(Name { token, role });

    for node in top_level(root) {
        match (node.kind(), declares(node.kind())) {
            (_, Some(declares)) => {
                if let Some(name) = tokens(&node).find(|token| is_name(token.kind())) {
                    push(name, Role::Declares(declares));
                }
                if let Some(label) = label(&node) {
                    push(label, Role::Closes(declares));
                }
            }
            (VERBATIM, None) => {
                for (token, role) in unparsed_declarations(&node) {
                    push(token, role);
                }
            }
            _ => {}
        }
    }

    for element in root.descendants_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                if let Some(uses) = scope(&token) {
                    push(token, Role::Uses(uses));
                }
            }
            NodeOrToken::Node(node) => match node.kind() {
                TYPE_REF => {
                    if let Some(head) = head(&node) {
                        let instance = node.parent().is_some_and(|parent| {
                            parent.kind() == INSTANTIATION
                                && parent.first_child().as_ref() == Some(&node)
                        });
                        let uses = if instance { Uses::Instance } else { Uses::Type };
                        push(head, Role::Uses(uses));
                    }
                }
                BIND_DIRECTIVE => {
                    if let Some(target) = target(&node) {
                        push(target, Role::Uses(Uses::Bind));
                    }
                }
                VERBATIM if node.parent().is_none_or(|p| p.kind() != VERBATIM) => {
                    for name in unparsed(&node) {
                        push(name, Role::Uses(Uses::Unparsed));
                    }
                }
                _ => {}
            },
        }
    }

    names.sort_by_key(|name| name.token.text_range().start());
    names
}

/// The text of the tree at `root`, each of its [`names`] replaced by what
/// `rename` gives for it, if anything. An escaped name stays escaped.
///
/// A name is replaced only where it declares, closes or uses a top-level
/// declaration, so a signal that shares a module's name keeps its own.
pub fn renamed(root: &SyntaxNode, rename: impl Fn(&str) -> Option<String>) -> String {
    let text = root.text().to_string();
    let base = root.text_range().start();
    let offset = |at: TextSize| usize::from(at - base);

    let mut out = String::with_capacity(text.len());
    let mut written = 0;
    for name in names(root) {
        let range = name.token.text_range();
        let (start, end) = (offset(range.start()), offset(range.end()));
        if start < written {
            continue;
        }
        let Some(new) = rename(name.text()) else {
            continue;
        };
        out.push_str(&text[written..start]);
        match name.token.kind() {
            ESCAPED_IDENT => {
                let spelled = name.token.text();
                out.push('\\');
                out.push_str(&new);
                out.push_str(&spelled[spelled.trim_end().len()..]);
            }
            _ => out.push_str(&new),
        }
        written = end;
    }
    out.push_str(&text[written..]);
    out
}

/// The items at the top level of a file, those in each branch of a raw
/// tree's conditionals included.
fn top_level(root: &SyntaxNode) -> Vec<SyntaxNode> {
    let mut items = Vec::new();
    let mut open: Vec<SyntaxNode> = root.children().collect();
    open.reverse();
    while let Some(node) = open.pop() {
        match node.kind() {
            CONDITIONAL_REGION | CONDITIONAL_BRANCH => {
                open.extend(node.children().collect::<Vec<_>>().into_iter().rev());
            }
            _ => items.push(node),
        }
    }
    items
}

fn declares(kind: SyntaxKind) -> Option<Declares> {
    Some(match kind {
        MODULE_DECL => Declares::Module,
        INTERFACE_DECL => Declares::Interface,
        PROGRAM_DECL => Declares::Program,
        PACKAGE_DECL => Declares::Package,
        CLASS_DECL => Declares::Class,
        _ => return None,
    })
}

/// What the keyword that ends a declaration ends.
fn closes(kind: SyntaxKind) -> Option<Declares> {
    Some(match kind {
        ENDMODULE_KW => Declares::Module,
        ENDINTERFACE_KW => Declares::Interface,
        ENDPROGRAM_KW => Declares::Program,
        ENDPACKAGE_KW => Declares::Package,
        ENDCLASS_KW => Declares::Class,
        _ => return None,
    })
}

fn is_name(kind: SyntaxKind) -> bool {
    matches!(kind, IDENT | ESCAPED_IDENT)
}

/// A node's own tokens, trivia left out.
fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

fn next(token: &SyntaxToken) -> Option<SyntaxToken> {
    std::iter::successors(token.next_token(), SyntaxToken::next_token)
        .find(|token| !token.kind().is_trivia())
}

fn previous(token: &SyntaxToken) -> Option<SyntaxToken> {
    std::iter::successors(token.prev_token(), SyntaxToken::prev_token)
        .find(|token| !token.kind().is_trivia())
}

/// The name after a declaration's end keyword and `:`.
fn label(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut after = tokens(node)
        .skip_while(|token| closes(token.kind()).is_none())
        .skip(1);
    if after.next()?.kind() != COLON {
        return None;
    }
    after.next().filter(|token| is_name(token.kind()))
}

/// A name that opens a scoped name, `pkg` in `pkg::a::b`: a package's
/// wherever it is written, even in text no rule read.
fn scope(token: &SyntaxToken) -> Option<Uses> {
    if !is_name(token.kind()) || next(token)?.kind() != COLON_COLON {
        return None;
    }
    if previous(token).is_some_and(|before| before.kind() == COLON_COLON) {
        return None;
    }
    let import = token
        .parent_ancestors()
        .any(|node| node.kind() == IMPORT_DECL);
    Some(if import { Uses::Import } else { Uses::Scope })
}

/// The name a type is written as, `bus_if` in `virtual interface bus_if.mp`,
/// unless it is scoped and so already a [`scope`].
fn head(node: &SyntaxNode) -> Option<SyntaxToken> {
    let first = node
        .children_with_tokens()
        .find(|element| {
            !element.kind().is_trivia() && !matches!(element.kind(), VIRTUAL_KW | INTERFACE_KW)
        })?
        .into_token()?;
    let scoped = next(&first).is_some_and(|after| after.kind() == COLON_COLON);
    (is_name(first.kind()) && !scoped).then_some(first)
}

/// The name a `bind` is written against, `top` in `bind top.u_core …`: a
/// module or interface, or the head of an instance's path.
fn target(node: &SyntaxNode) -> Option<SyntaxToken> {
    let target = node.first_child()?;
    let first = target
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())?;
    is_name(first.kind()).then_some(first)
}

/// What reads as declarations in unparsed text: a keyword that opens one,
/// and the name after it, or one that closes one, `:` and the name.
///
/// A rule that gives up on something inside a module leaves the whole module
/// unparsed, and a declaration missed would drop a file a design needs.
fn unparsed_declarations(node: &SyntaxNode) -> Vec<(SyntaxToken, Role)> {
    let tokens: Vec<SyntaxToken> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let kind = |at: usize| tokens.get(at).map_or(EOF, SyntaxToken::kind);

    let mut found = Vec::new();
    for at in 0..tokens.len() {
        if let Some(declares) = closes(kind(at)) {
            if kind(at + 1) == COLON && is_name(kind(at + 2)) {
                found.push((tokens[at + 2].clone(), Role::Closes(declares)));
            }
            continue;
        }
        // `virtual interface`, `typedef class` and `extern module` only
        // name what is declared elsewhere.
        if matches!(
            kind(at.wrapping_sub(1)),
            VIRTUAL_KW | TYPEDEF_KW | EXTERN_KW
        ) {
            continue;
        }
        let declares = match kind(at) {
            MODULE_KW | MACROMODULE_KW => Declares::Module,
            INTERFACE_KW if kind(at + 1) != CLASS_KW => Declares::Interface,
            PROGRAM_KW => Declares::Program,
            PACKAGE_KW => Declares::Package,
            CLASS_KW => Declares::Class,
            _ => continue,
        };
        let mut name = at + 1;
        while matches!(kind(name), AUTOMATIC_KW | STATIC_KW) {
            name += 1;
        }
        if is_name(kind(name)) {
            found.push((tokens[name].clone(), Role::Declares(declares)));
        }
    }
    found
}

/// The types of what reads as instantiations in unparsed text: a name, an
/// optional `#( … )`, a name and `(`.
fn unparsed(node: &SyntaxNode) -> Vec<SyntaxToken> {
    let tokens: Vec<SyntaxToken> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let kind = |at: usize| tokens.get(at).map_or(EOF, SyntaxToken::kind);

    let mut found = Vec::new();
    for at in 0..tokens.len() {
        if !is_name(kind(at)) || matches!(kind(at.wrapping_sub(1)), DOT | COLON_COLON) {
            continue;
        }
        let mut after = at + 1;
        if kind(after) == HASH && kind(after + 1) == L_PAREN {
            let mut depth = 0;
            after += 1;
            while after < tokens.len() {
                match kind(after) {
                    L_PAREN => depth += 1,
                    R_PAREN => depth -= 1,
                    _ => {}
                }
                after += 1;
                if depth == 0 {
                    break;
                }
            }
        }
        if is_name(kind(after)) && kind(after + 1) == L_PAREN {
            found.push(tokens[at].clone());
        }
    }
    found
}
