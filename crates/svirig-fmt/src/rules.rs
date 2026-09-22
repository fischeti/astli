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

        self.shell(header, body, end)
    }

    /// `begin` or `fork` with its label, statements indented below, and the
    /// closer with its label on a line of its own.
    fn block(&mut self, block: &SyntaxNode) -> Doc {
        let children = significant_children(block);
        let attributes = children.iter().take_while(|it| it.kind() == ATTRIBUTES);
        let opener = attributes.count();
        // The opener, and its label if it has one.
        let mut body = opener + 1;
        if children.get(body).is_some_and(|it| it.kind() == COLON) {
            body += 2;
        }
        let nodes = children
            .iter()
            .skip(body)
            .take_while(|it| it.as_node().is_some());
        let end = body + nodes.count();
        let plain = end < children.len()
            && children[opener..body]
                .iter()
                .all(|it| it.as_token().is_some())
            && children[end..].iter().all(|it| it.as_token().is_some());
        if !plain {
            return self.verbatim(block);
        }
        self.shell(&children[..body], &children[body..end], &children[end..])
    }

    /// A header on one line, items indented below it, and an end on a line
    /// of its own. Neither `header` nor `end` is empty.
    fn shell(
        &mut self,
        header: &[SyntaxElement],
        items: &[SyntaxElement],
        end: &[SyntaxElement],
    ) -> Doc {
        let (last, header) = header.split_last().expect("a header");
        let mut docs = vec![self.spaced(header), separation(header.last(), last)];
        // A comment after the header on a line of its own is the first of the
        // body, so it is indented with it.
        let after = match last {
            NodeOrToken::Node(node) => {
                docs.push(self.node(node));
                Doc::nil()
            }
            NodeOrToken::Token(token) => {
                docs.push(Doc::token(token.text()));
                self.comments.after(token)
            }
        };
        let closer = end.first().and_then(|it| it.as_token());
        docs.extend([
            Doc::indent(Doc::concat([after, self.items(items)])),
            Doc::HardLine,
            closer.map_or_else(Doc::nil, blank_line_before),
            self.spaced(end),
        ]);
        Doc::concat(docs)
    }

    /// `always`, `initial` and the like, and the statement they run.
    fn procedural_block(&mut self, block: &SyntaxNode) -> Doc {
        match &significant_children(block)[..] {
            [NodeOrToken::Token(keyword), NodeOrToken::Node(body)] => {
                Doc::concat([self.token(keyword), self.body(body)])
            }
            _ => self.verbatim(block),
        }
    }

    /// A timing control, then the statement it delays or a `;`.
    fn timing_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        let attributes = children.iter().take_while(|it| it.kind() == ATTRIBUTES);
        let (attributes, rest) = children.split_at(attributes.count());
        let (control, rest) = match rest {
            [NodeOrToken::Node(control), rest @ ..]
                if matches!(control.kind(), EVENT_CONTROL | DELAY_CONTROL) =>
            {
                (control, rest)
            }
            _ => return self.verbatim(stmt),
        };
        let (body, semicolon) = match rest {
            [] => (None, None),
            [NodeOrToken::Node(body)] => (Some(body), None),
            [NodeOrToken::Token(semicolon)] => (None, Some(semicolon)),
            [NodeOrToken::Node(body), NodeOrToken::Token(semicolon)] => {
                (Some(body), Some(semicolon))
            }
            _ => return self.verbatim(stmt),
        };
        if semicolon.is_some_and(|it| it.kind() != SEMICOLON) {
            return self.verbatim(stmt);
        }

        Doc::concat([
            self.spaced(attributes),
            Doc::Space,
            self.node(control),
            body.map_or_else(Doc::nil, |body| self.body(body)),
            semicolon.map_or_else(Doc::nil, |semicolon| self.token(semicolon)),
        ])
    }

    /// `if`, its condition and branch, and the `else` branch. `end else`
    /// share a line unless the `end` has a label; an `else` after any other
    /// statement starts a line.
    fn if_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        let keywords = children.iter().take_while(|it| it.as_token().is_some());
        let (keywords, rest) = children.split_at(keywords.count());
        let (condition, then, rest) = match rest {
            [
                NodeOrToken::Node(condition),
                NodeOrToken::Node(then),
                rest @ ..,
            ] if condition.kind() == PAREN_EXPR && matches!(keywords.len(), 1 | 2) => {
                (condition, then, rest)
            }
            _ => return self.verbatim(stmt),
        };
        let otherwise = match rest {
            [] => None,
            [NodeOrToken::Token(keyword), NodeOrToken::Node(branch)]
                if keyword.kind() == ELSE_KW =>
            {
                Some((keyword, branch))
            }
            _ => return self.verbatim(stmt),
        };

        let mut docs = vec![
            self.spaced(keywords),
            Doc::Space,
            self.node(condition),
            self.body(then),
        ];
        if let Some((keyword, branch)) = otherwise {
            let labelled = last_token(then).is_some_and(|it| it.kind() == IDENT);
            docs.push(match then.kind() {
                BLOCK if !labelled => Doc::Space,
                _ => Doc::HardLine,
            });
            docs.push(self.token(keyword));
            docs.push(match branch.kind() {
                IF_STMT => Doc::concat([Doc::Space, self.node(branch)]),
                _ => self.body(branch),
            });
        }
        Doc::concat(docs)
    }

    /// `case`, its expression, and each item on a line of its own.
    fn case_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        let keywords = children.iter().take_while(|it| it.as_token().is_some());
        let keywords = keywords.count();
        // The expression, and `inside` or `matches` if either follows it.
        let mut items = keywords + 1;
        if children
            .get(items)
            .is_some_and(|it| it.as_token().is_some())
        {
            items += 1;
        }
        let nodes = children
            .iter()
            .skip(items)
            .take_while(|it| it.as_node().is_some());
        let end = items + nodes.count();
        let plain = matches!(keywords, 1 | 2)
            && children
                .get(keywords)
                .is_some_and(|it| it.kind() == PAREN_EXPR)
            && end + 1 == children.len()
            && children[end].kind() == ENDCASE_KW;
        if !plain {
            return self.verbatim(stmt);
        }
        self.shell(&children[..items], &children[items..end], &children[end..])
    }

    /// The values or `default`, a `:` straight after them, and the
    /// statement, which follows the rule for any statement.
    fn case_item(&mut self, item: &SyntaxNode) -> Doc {
        let children = significant_children(item);
        let Some((NodeOrToken::Node(body), values)) = children.split_last() else {
            return self.verbatim(item);
        };
        let (values, colon) = match values.split_last() {
            Some((NodeOrToken::Token(colon), values)) if colon.kind() == COLON => {
                (values, Some(colon))
            }
            _ => (values, None),
        };
        let plain = !values.is_empty()
            && values
                .iter()
                .all(|it| it.as_node().is_some() || matches!(it.kind(), COMMA | DEFAULT_KW));
        if !plain {
            return self.verbatim(item);
        }
        Doc::concat([
            self.spaced(values),
            colon.map_or_else(Doc::nil, |colon| self.token(colon)),
            self.body(body),
        ])
    }

    /// A list in parentheses, after a `#` if it is of parameters. Broken, it
    /// has each entry on a line of its own and the `)` at the start of one;
    /// otherwise it stays on the line if all of it fits.
    fn list(&mut self, list: &SyntaxNode, broken: bool) -> Doc {
        let children = significant_children(list);
        let opener = children.iter().position(|it| it.kind() == L_PAREN);
        let plain = opener.is_some_and(|opener| {
            children[..opener].iter().all(|it| it.kind() == HASH)
                && children.last().is_some_and(|it| it.kind() == R_PAREN)
                && opener < children.len() - 1
                && children[opener + 1..children.len() - 1]
                    .iter()
                    .all(|it| it.as_node().is_some() || it.kind() == COMMA)
        });
        let Some(opener) = opener.filter(|_| plain) else {
            return self.verbatim(list);
        };
        let (header, rest) = children.split_at(opener + 1);
        let (entries, close) = rest.split_at(rest.len() - 1);
        if entries.is_empty() {
            return self.spaced(&children);
        }
        if broken {
            return self.shell(header, entries, close);
        }

        let mut inner = vec![Doc::SoftLine];
        for entry in entries {
            match entry {
                NodeOrToken::Node(node) => inner.push(self.node(node)),
                NodeOrToken::Token(comma) => inner.extend([self.token(comma), Doc::Line]),
            }
        }
        Doc::group(Doc::concat([
            self.spaced(header),
            Doc::indent(Doc::concat(inner)),
            Doc::SoftLine,
            self.spaced(close),
        ]))
    }

    /// `@` or `#` and what follows it, with no space between.
    fn control(&mut self, control: &SyntaxNode) -> Doc {
        match &significant_children(control)[..] {
            [NodeOrToken::Token(op), operand] => {
                Doc::concat([self.token(op), self.element(operand)])
            }
            _ => self.verbatim(control),
        }
    }

    /// An assignment, a call or an expression, and its `;`.
    fn expr_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        match &children[..] {
            [NodeOrToken::Node(_), NodeOrToken::Token(semicolon)]
            | [NodeOrToken::Token(semicolon)]
                if semicolon.kind() == SEMICOLON =>
            {
                self.spaced(&children)
            }
            _ => self.verbatim(stmt),
        }
    }

    /// The statement a keyword or a header runs. A block, or a timing
    /// control whose own statement follows this rule, goes on the same line.
    /// Any other statement goes on the same line if all of it fits, and one
    /// level in on the next if not: the braces that would keep it on the
    /// line are tokens the formatter may not add.
    fn body(&mut self, body: &SyntaxNode) -> Doc {
        match body.kind() {
            BLOCK | TIMING_STMT => Doc::concat([Doc::Space, self.node(body)]),
            _ => Doc::group(Doc::indent(Doc::concat([Doc::Line, self.node(body)]))),
        }
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

    /// `assign`, an optional delay, and assignments between commas. A drive
    /// strength is taken as loose tokens, and falls back.
    fn continuous_assign(&mut self, assign: &SyntaxNode) -> Doc {
        let children = significant_children(assign);
        let plain = match &children[..] {
            [keyword, rest @ .., semicolon] => {
                keyword.kind() == ASSIGN_KW
                    && semicolon.kind() == SEMICOLON
                    && rest
                        .iter()
                        .all(|it| it.as_node().is_some() || it.kind() == COMMA)
            }
            _ => false,
        };
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(assign),
        }
    }

    /// A target, an operator, an optional delay and a value.
    fn assignment(&mut self, assignment: &SyntaxNode) -> Doc {
        let children = significant_children(assignment);
        let plain = match &children[..] {
            [
                NodeOrToken::Node(_),
                NodeOrToken::Token(_),
                NodeOrToken::Node(_),
            ] => true,
            [
                NodeOrToken::Node(_),
                NodeOrToken::Token(_),
                NodeOrToken::Node(delay),
                NodeOrToken::Node(_),
            ] => delay.kind() == DELAY_CONTROL,
            _ => false,
        };
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(assignment),
        }
    }

    /// Each of `elements` on lines of its own. Only a stray token would not be
    /// a node.
    /// A comma stays on the line of the item before it; any other token
    /// would be stray.
    fn items(&mut self, elements: &[SyntaxElement]) -> Doc {
        Doc::concat(elements.iter().map(|element| match element {
            NodeOrToken::Node(item) => self.item(item),
            NodeOrToken::Token(comma) if comma.kind() == COMMA => self.token(comma),
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
            CONTINUOUS_ASSIGN => self.continuous_assign(node),
            PROCEDURAL_BLOCK => self.procedural_block(node),
            BLOCK => self.block(node),
            TIMING_STMT => self.timing_stmt(node),
            EVENT_CONTROL | DELAY_CONTROL => self.control(node),
            EXPR_STMT => self.expr_stmt(node),
            IF_STMT => self.if_stmt(node),
            CASE_STMT => self.case_stmt(node),
            CASE_ITEM => self.case_item(node),
            ASSIGNMENT => self.assignment(node),
            PARAM_PORT_LIST | PORT_LIST
                if node.parent().is_some_and(|parent| {
                    matches!(parent.kind(), MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL)
                }) =>
            {
                self.list(node, true)
            }
            _ => self.verbatim(node),
        }
    }

    /// `elements` on one line, spaced as [`separation`] says.
    fn spaced(&mut self, elements: &[SyntaxElement]) -> Doc {
        let mut docs = Vec::new();
        let mut prev = None;
        for element in elements {
            docs.extend([separation(prev, element), self.element(element)]);
            prev = Some(element);
        }
        Doc::concat(docs)
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

/// A space between two elements on a line, but none before `,`, `;` or `)`
/// and none after `#` or `(`. What comes before the first is its parent's
/// to separate.
fn separation(prev: Option<&SyntaxElement>, next: &SyntaxElement) -> Doc {
    let tight = prev.is_none_or(|prev| matches!(prev.kind(), HASH | L_PAREN))
        || matches!(next.kind(), COMMA | SEMICOLON | R_PAREN);
    match tight {
        true => Doc::nil(),
        false => Doc::Space,
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

fn last_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    (node.descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .filter(|token| !token.kind().is_trivia())
        .last()
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
