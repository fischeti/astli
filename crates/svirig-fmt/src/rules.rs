//! The rules, one per construct, and the fallback that writes a node no rule
//! lays out as it was read.
//!
//! A rule writes every token of its node, and takes every comment the comment
//! map gives to the node, its descendants or its tokens.

use rowan::NodeOrToken;
use svirig_syntax::{SyntaxElement, SyntaxKind::*, SyntaxNode, SyntaxToken};

use crate::comments::Comments;
use crate::doc::Doc;
use crate::verbatim::verbatim;

/// The document for the file `root` holds, and the nodes no rule laid out.
pub(crate) fn write(root: &SyntaxNode, source: &str) -> (Doc, Vec<SyntaxNode>) {
    let mut writer = Writer {
        source,
        comments: Comments::new(root, source),
        unformatted: Vec::new(),
    };
    let doc = writer.source_file(root);
    debug_assert!(
        writer.comments.untaken().is_none(),
        "no rule wrote {:?}",
        writer.comments.untaken()
    );
    (doc, writer.unformatted)
}

struct Writer<'a> {
    source: &'a str,
    comments: Comments,
    /// The outermost of the nodes written as they were read.
    unformatted: Vec<SyntaxNode>,
}

impl Writer<'_> {
    /// Items one after another, each on lines of its own.
    fn source_file(&mut self, file: &SyntaxNode) -> Doc {
        let items = self.items(&significant_children(file));
        Doc::concat([self.comments.head(), items])
    }

    /// A module, interface, program or package: its header on one line, its
    /// items indented below, and its end on a line of its own.
    fn design_unit(&mut self, unit: &SyntaxNode) -> Doc {
        let children = significant_children(unit);
        let semicolon = children.iter().position(|it| it.kind() == SEMICOLON);
        let Some(semicolon) = semicolon else {
            return self.verbatim(unit);
        };
        let (header, rest) = children.split_at(semicolon + 1);
        let end = rest.iter().position(|it| it.as_token().is_some());
        let Some((body, end)) = end.map(|end| rest.split_at(end)) else {
            return self.verbatim(unit);
        };
        // A preprocessor construct in the header would need its line kept.
        let header_is_plain = header.iter().all(|it| {
            it.as_node().is_none_or(|node| {
                matches!(
                    node.kind(),
                    ATTRIBUTES | IMPORT_DECL | PARAM_PORT_LIST | PORT_LIST
                )
            })
        });
        if !header_is_plain || end.iter().any(|it| it.as_node().is_some()) {
            return self.verbatim(unit);
        }

        let mut docs = Vec::new();
        for element in &header[..semicolon] {
            docs.extend([Doc::Space, self.element(element)]);
        }
        let semicolon = header[semicolon].as_token().expect("a `;` is a token");
        // A comment after the `;` on its own line is the first of the body.
        docs.extend([
            Doc::text(";"),
            Doc::indent(Doc::concat([
                self.comments.after(semicolon),
                self.items(body),
            ])),
            Doc::HardLine,
        ]);
        let keyword = end[0]
            .as_token()
            .expect("the end is the first token after the body");
        docs.push(blank_line_before(keyword));
        for element in end {
            docs.extend([Doc::Space, self.element(element)]);
        }
        Doc::concat(docs)
    }

    /// A conditional region: each directive on a line of its own at the
    /// margin, and each branch's items where they would be without them.
    fn conditional_region(&mut self, region: &SyntaxNode) -> Doc {
        let children = significant_children(region);
        let last = children.len().saturating_sub(1);
        let plain = children.iter().enumerate().all(|(at, child)| match child {
            NodeOrToken::Node(branch) => branch.kind() == CONDITIONAL_BRANCH && is_plain(branch),
            NodeOrToken::Token(_) => at == last,
        });
        if !plain {
            return self.verbatim(region);
        }

        let mut docs = Vec::new();
        for child in &children {
            match child {
                NodeOrToken::Node(branch) => {
                    let children = significant_children(branch);
                    let items = children.iter().position(|it| it.as_node().is_some());
                    let (directive, items) = children.split_at(items.unwrap_or(children.len()));
                    docs.extend([
                        self.comments.leading(branch),
                        self.directive(directive),
                        self.items(items),
                        self.comments.trailing(branch),
                    ]);
                }
                NodeOrToken::Token(_) => docs.push(self.directive(std::slice::from_ref(child))),
            }
        }
        return Doc::concat(docs);

        /// A directive and its condition, then the branch's items.
        fn is_plain(branch: &SyntaxNode) -> bool {
            let children = significant_children(branch);
            let tokens = children.iter().take_while(|it| it.as_token().is_some());
            let tokens = tokens.count();
            (1..=2).contains(&tokens) && children[tokens..].iter().all(|it| it.as_node().is_some())
        }
    }

    /// The tokens of a directive on a line of their own at the margin.
    fn directive(&mut self, tokens: &[SyntaxElement]) -> Doc {
        let mut docs = vec![Doc::HardLine];
        if let Some(NodeOrToken::Token(first)) = tokens.first() {
            docs.push(blank_line_before(first));
        }
        for token in tokens {
            docs.extend([Doc::Space, self.element(token)]);
        }
        Doc::margin(Doc::concat(docs))
    }

    /// Each of `elements` on lines of its own. Only a stray token would not be
    /// a node.
    fn items(&mut self, elements: &[SyntaxElement]) -> Doc {
        Doc::concat(elements.iter().map(|element| match element {
            NodeOrToken::Node(item) => self.item(item),
            NodeOrToken::Token(token) => Doc::concat([Doc::HardLine, self.token(token)]),
        }))
    }

    /// An item on lines of its own, after an empty line if it had one.
    fn item(&mut self, item: &SyntaxNode) -> Doc {
        let blank_line = first_token(item).map_or_else(Doc::nil, |token| blank_line_before(&token));
        Doc::concat([
            Doc::HardLine,
            self.comments.leading(item),
            blank_line,
            self.layout(item),
            self.comments.trailing(item),
        ])
    }

    /// `node` with the comments around it.
    fn node(&mut self, node: &SyntaxNode) -> Doc {
        Doc::concat([
            self.comments.leading(node),
            self.layout(node),
            self.comments.trailing(node),
        ])
    }

    /// `node` without the comments around it.
    fn layout(&mut self, node: &SyntaxNode) -> Doc {
        match node.kind() {
            MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL => self.design_unit(node),
            CONDITIONAL_REGION => self.conditional_region(node),
            _ => self.verbatim(node),
        }
    }

    fn element(&mut self, element: &SyntaxElement) -> Doc {
        match element {
            NodeOrToken::Node(node) => self.node(node),
            NodeOrToken::Token(token) => self.token(token),
        }
    }

    fn token(&mut self, token: &SyntaxToken) -> Doc {
        Doc::concat([Doc::token(token.text()), self.comments.after(token)])
    }

    fn verbatim(&mut self, node: &SyntaxNode) -> Doc {
        let Some((verbatim, covers)) = verbatim(node, self.source) else {
            return Doc::nil();
        };
        self.comments.within(covers);
        self.unformatted.push(node.clone());
        Doc::Verbatim(verbatim)
    }
}

/// The child nodes, and the child tokens other than trivia, in order.
fn significant_children(node: &SyntaxNode) -> Vec<SyntaxElement> {
    (node.children_with_tokens())
        .filter(|it| !it.kind().is_trivia())
        .collect()
}

fn first_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    (node.descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .find(|token| !token.kind().is_trivia())
}

/// An empty line if the input has one right before `token`.
///
/// Comments before it carry their own separation, so only the whitespace
/// between the last of them and the token is read.
fn blank_line_before(token: &SyntaxToken) -> Doc {
    match token.prev_token() {
        Some(space) if space.kind() == WHITESPACE && space.text().matches('\n').count() > 1 => {
            Doc::BlankLine
        }
        _ => Doc::nil(),
    }
}
