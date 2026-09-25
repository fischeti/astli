//! What one file declares and uses at the top level, read off its tree.

use std::fmt;
use std::path::{Path, PathBuf};

use astli_parse::Parsed;
use astli_preproc::Session;
use astli_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken};
use astli_text::{LineCol, SourceId};
use rowan::NodeOrToken;

/// What a top-level declaration declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Declares {
    Module,
    Interface,
    Program,
    Package,
    Class,
}

/// How a reference uses the name it names, which says what it may resolve to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uses {
    /// The type of an instantiation: a module, interface or program.
    Instance,
    /// The package of an `import` or `export`.
    Import,
    /// The name left of `::`: a package, or a class that may be local.
    Scope,
    /// The head of a type: an interface, a class, or a typedef that may be
    /// local.
    Type,
    /// A name in text the grammar left `VERBATIM`, shaped like an
    /// instantiation's type.
    Unparsed,
}

impl Uses {
    /// Whether the name must be declared at the top level of some file, so
    /// that finding it nowhere is a problem.
    pub fn is_global(self) -> bool {
        matches!(self, Uses::Instance | Uses::Import)
    }
}

/// Where a name is written: the file, and the line and column in it. A name
/// a macro wrote is where the outermost call was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub at: LineCol,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path.display(), self.at)
    }
}

/// A module, interface, program, package or class declared at the top level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub declares: Declares,
    pub location: Location,
}

/// A name used where it can only mean something declared at the top level,
/// or might.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    pub uses: Uses,
    pub location: Location,
}

/// The top-level names one file declares and uses, and the headers it read.
///
/// It holds no span, so it outlives the session it was read from and can be
/// sent across threads: an [`Index`](crate::Index) is built from summaries
/// made in parallel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub path: PathBuf,
    /// In the order they are written, headers included where they are.
    pub declarations: Vec<Declaration>,
    /// Every use, in the order written; one name may be used many times.
    pub references: Vec<Reference>,
    /// Every file an `` `include `` read, directly or through another, in the
    /// order first read.
    pub includes: Vec<PathBuf>,
}

impl Summary {
    /// Reads the summary of `file` off `parsed`, its tree in either mode.
    ///
    /// In expanded mode what the macros write and the headers declare is part
    /// of it, and only the branches the build takes. In raw mode neither is,
    /// and every branch is.
    pub fn new(session: &Session, file: SourceId, parsed: &Parsed) -> Summary {
        let origins = session.origins();
        let locate = |token: &SyntaxToken| {
            let span = parsed.span(token)?;
            let at = origins.spelled(origins.reported_at(span));
            Some(Location {
                path: origins.path(at.src_id)?.to_path_buf(),
                at: origins.line_col(at.src_id, at.start),
            })
        };

        let mut declarations = Vec::new();
        for node in top_level(&parsed.root) {
            let found = match (node.kind(), declares(node.kind())) {
                (_, Some(declares)) => {
                    let name = tokens(&node).find(|token| is_name(token.kind()));
                    name.map(|name| (declares, name)).into_iter().collect()
                }
                (VERBATIM, None) => unparsed_declarations(&node),
                _ => Vec::new(),
            };
            for (declares, name) in found {
                if let Some(location) = locate(&name) {
                    declarations.push(Declaration {
                        name: text(&name),
                        declares,
                        location,
                    });
                }
            }
        }

        let mut references = Vec::new();
        let mut push = |token: &SyntaxToken, uses| {
            if let Some(location) = locate(token) {
                references.push(Reference {
                    name: text(token),
                    uses,
                    location,
                });
            }
        };
        for element in parsed.root.descendants_with_tokens() {
            match element {
                NodeOrToken::Token(token) => {
                    if let Some(uses) = scope(&token) {
                        push(&token, uses);
                    }
                }
                NodeOrToken::Node(node) => match node.kind() {
                    TYPE_REF => {
                        if let Some(head) = head(&node) {
                            let instance = node.parent().is_some_and(|parent| {
                                parent.kind() == INSTANTIATION
                                    && parent.first_child().as_ref() == Some(&node)
                            });
                            push(&head, if instance { Uses::Instance } else { Uses::Type });
                        }
                    }
                    VERBATIM if node.parent().is_none_or(|p| p.kind() != VERBATIM) => {
                        for name in unparsed(&node) {
                            push(&name, Uses::Unparsed);
                        }
                    }
                    _ => {}
                },
            }
        }

        let path = origins
            .path(file)
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let includes = origins
            .files()
            .filter(|&other| {
                let root = origins.include_trace(other).last();
                root.is_some_and(|site| origins.spelled(site).src_id == file)
            })
            .filter_map(|other| origins.path(other).map(Path::to_path_buf))
            .collect();

        Summary {
            path,
            declarations,
            references,
            includes,
        }
    }
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

fn is_name(kind: SyntaxKind) -> bool {
    matches!(kind, IDENT | ESCAPED_IDENT)
}

/// A name as the language compares it: `\cpu3 ` and `cpu3` are one name.
fn text(token: &SyntaxToken) -> String {
    match token.text().strip_prefix('\\') {
        // The whitespace that ends an escaped name is part of its token.
        Some(escaped) => escaped.trim_end().to_string(),
        None => token.text().to_string(),
    }
}

/// A node's own tokens and its descendants', trivia left out.
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

/// What reads as declarations in unparsed text: a keyword that opens one,
/// and the name after it.
///
/// A rule that gives up on something inside a module leaves the whole module
/// unparsed, and a declaration missed would drop a file a design needs.
fn unparsed_declarations(node: &SyntaxNode) -> Vec<(Declares, SyntaxToken)> {
    let tokens: Vec<SyntaxToken> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let kind = |at: usize| tokens.get(at).map_or(EOF, SyntaxToken::kind);

    let mut found = Vec::new();
    for at in 0..tokens.len() {
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
            found.push((declares, tokens[name].clone()));
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
