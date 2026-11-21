//! Statements and what they run, assertions, and assignments.

use astli_syntax::{SyntaxElement, SyntaxKind::*, SyntaxNode, last_significant};
use rowan::NodeOrToken;

use super::{Writer, separation, significant_children};
use crate::doc::Doc;

impl Writer<'_> {
    /// `begin` or `fork` with its label, statements indented below, and the
    /// closer with its label on a line of its own.
    pub(super) fn block(&mut self, block: &SyntaxNode) -> Doc {
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

    /// `always`, `initial` and the like, and the statement they run.
    pub(super) fn procedural_block(&mut self, block: &SyntaxNode) -> Doc {
        match &significant_children(block)[..] {
            [NodeOrToken::Token(keyword), NodeOrToken::Node(body)] => {
                Doc::concat([self.token(keyword), self.body(body)])
            }
            _ => self.verbatim(block),
        }
    }

    /// A timing control, then the statement it delays or a `;`.
    pub(super) fn timing_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
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

    /// `if`, its condition and branch, and the `else` branch. `end else` and
    /// `} else` share a line unless the `end` has a label; an `else` after
    /// any other statement starts a line.
    pub(super) fn if_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
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
            let labelled = last_significant(then).is_some_and(|it| it.kind() == IDENT);
            docs.push(match then.kind() {
                BLOCK if !labelled => Doc::Space,
                CONSTRAINT_BLOCK => Doc::Space,
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

    /// `assert`, `assume` or `cover` and its deferral, the condition, what it
    /// runs when the condition holds, and `else` with what it runs when not;
    /// so for `wait_order` and its events.
    /// `else` shares the line of the condition, of an `end`, or of a `;` that
    /// runs nothing, as 518 assertions in the corpus have it to 241 that
    /// start a line with it.
    pub(super) fn immediate_assertion(&mut self, assertion: &SyntaxNode) -> Doc {
        let children = significant_children(assertion);
        let condition = children.iter().position(|it| it.as_node().is_some());
        let Some(condition) = condition else {
            return self.verbatim(assertion);
        };
        let (keywords, rest) = children.split_at(condition);
        let (condition, rest) = match rest {
            [NodeOrToken::Node(condition), rest @ ..] if condition.kind() == PAREN_EXPR => {
                (condition, rest)
            }
            _ => return self.verbatim(assertion),
        };
        let head = Doc::concat([self.spaced(keywords), Doc::Space, self.node(condition)]);
        self.action_block(assertion, head, rest)
    }

    /// `assert property` and the like, the property in parentheses, and what
    /// it runs as an immediate assertion does. The property keeps the lines
    /// it was written on: it is broken by hand around its operators far more
    /// often than a rule could say where.
    pub(super) fn concurrent_assertion(&mut self, assertion: &SyntaxNode) -> Doc {
        let children = significant_children(assertion);
        let open = children.iter().position(|it| it.kind() == L_PAREN);
        let Some(open) = open else {
            return self.verbatim(assertion);
        };
        let (keywords, rest) = children.split_at(open);
        let (inner, rest) = match rest {
            [open, NodeOrToken::Node(spec), close, rest @ ..]
                if spec.kind() == PROPERTY_SPEC && close.kind() == R_PAREN =>
            {
                ([open, close], (spec, rest))
            }
            _ => return self.verbatim(assertion),
        };
        let (spec, rest) = rest;
        let head = Doc::concat([
            self.spaced(keywords),
            Doc::Space,
            self.element(inner[0]),
            self.node(spec),
            self.element(inner[1]),
        ]);
        self.action_block(assertion, head, rest)
    }

    /// `head`, then what an assertion runs when it holds and `else` with
    /// what it runs when not, from `rest`.
    fn action_block(&mut self, assertion: &SyntaxNode, head: Doc, rest: &[SyntaxElement]) -> Doc {
        let (pass, fail) = match rest {
            [] => (None, None),
            [NodeOrToken::Node(pass)] => (Some(pass), None),
            [NodeOrToken::Token(keyword), NodeOrToken::Node(fail)] => (None, Some((keyword, fail))),
            [
                NodeOrToken::Node(pass),
                NodeOrToken::Token(keyword),
                NodeOrToken::Node(fail),
            ] => (Some(pass), Some((keyword, fail))),
            _ => return self.verbatim(assertion),
        };
        if fail.is_some_and(|(keyword, _)| keyword.kind() != ELSE_KW) {
            return self.verbatim(assertion);
        }

        let mut docs = vec![head];
        docs.push(match pass {
            Some(pass) if is_empty_stmt(pass) => self.node(pass),
            Some(pass) => self.body(pass),
            None => Doc::nil(),
        });
        if let Some((keyword, fail)) = fail {
            let shares = pass.is_none_or(|pass| {
                is_empty_stmt(pass)
                    || (pass.kind() == BLOCK
                        && last_significant(pass).is_none_or(|it| it.kind() != IDENT))
            });
            if pass.is_none_or(is_empty_stmt) && fail.kind() != BLOCK {
                // One that does not fit puts `else` on the next line, one
                // level in, before breaking inside what it runs.
                let otherwise = Doc::concat([Doc::Line, self.token(keyword), self.body(fail)]);
                docs.push(Doc::indent(otherwise));
                return Doc::group(Doc::concat(docs));
            }
            docs.extend([
                if shares { Doc::Space } else { Doc::HardLine },
                self.token(keyword),
                self.body(fail),
            ]);
        }
        Doc::concat(docs)
    }

    /// A `property` or `sequence` header on one line, its variables and what
    /// it is indented below, with the `;` after it, and its closer on a line
    /// of its own.
    pub(super) fn assertion_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        let header = children.iter().position(|it| it.kind() == SEMICOLON);
        let closer = children
            .iter()
            .position(|it| matches!(it.kind(), ENDPROPERTY_KW | ENDSEQUENCE_KW));
        let (Some(header), Some(closer)) = (header, closer) else {
            return self.verbatim(decl);
        };
        let plain = children[..header]
            .iter()
            .all(|it| it.as_token().is_some() || it.kind() == PORT_LIST)
            && children[closer..].iter().all(|it| it.as_token().is_some());
        if !plain || header >= closer {
            return self.verbatim(decl);
        }
        self.shell(
            &children[..=header],
            &children[header + 1..closer],
            &children[closer..],
        )
    }

    /// `return` or `disable`, what it returns or ends, and `;`.
    pub(super) fn keyword_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        match &children[..] {
            [NodeOrToken::Token(_), rest @ .., semicolon]
                if semicolon.kind() == SEMICOLON && rest.len() <= 1 =>
            {
                self.spaced(&children)
            }
            _ => self.verbatim(stmt),
        }
    }

    /// `wait fork;`, or `wait`, a condition, and the statement it runs or
    /// `;`.
    pub(super) fn wait_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        match &children[..] {
            [NodeOrToken::Token(_), fork, semicolon]
                if fork.kind() == FORK_KW && semicolon.kind() == SEMICOLON =>
            {
                self.spaced(&children)
            }
            [
                NodeOrToken::Token(_),
                NodeOrToken::Node(condition),
                semicolon,
            ] if condition.kind() == PAREN_EXPR && semicolon.kind() == SEMICOLON => {
                self.spaced(&children)
            }
            [
                NodeOrToken::Token(keyword),
                NodeOrToken::Node(condition),
                NodeOrToken::Node(body),
            ] if condition.kind() == PAREN_EXPR => Doc::concat([
                self.token(keyword),
                Doc::Space,
                self.node(condition),
                self.body(body),
            ]),
            _ => self.verbatim(stmt),
        }
    }

    /// `do`, the statement it repeats, then `while`, its condition and `;`:
    /// after the `end` of a block, or on a line of its own after anything
    /// else.
    pub(super) fn do_while_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        let [
            NodeOrToken::Token(keyword),
            NodeOrToken::Node(body),
            NodeOrToken::Token(while_kw),
            NodeOrToken::Node(condition),
            NodeOrToken::Token(semicolon),
        ] = &children[..]
        else {
            return self.verbatim(stmt);
        };
        if while_kw.kind() != WHILE_KW
            || condition.kind() != PAREN_EXPR
            || semicolon.kind() != SEMICOLON
        {
            return self.verbatim(stmt);
        }
        let labelled = last_significant(body).is_some_and(|it| it.kind() == IDENT);
        Doc::concat([
            self.token(keyword),
            self.body(body),
            match body.kind() {
                BLOCK if !labelled => Doc::Space,
                _ => Doc::HardLine,
            },
            self.token(while_kw),
            Doc::Space,
            self.node(condition),
            self.token(semicolon),
        ])
    }

    /// A label, `:`, and the statement it names, on the same line.
    pub(super) fn labeled_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        match &children[..] {
            [
                NodeOrToken::Token(label),
                colon,
                NodeOrToken::Node(labelled),
            ] if colon.kind() == COLON => Doc::concat([
                self.token(label),
                self.element(colon),
                Doc::Space,
                self.node(labelled),
            ]),
            _ => self.verbatim(stmt),
        }
    }

    /// `case`, its expression, and each item on a line of its own.
    pub(super) fn case_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        let keywords = children.iter().take_while(|it| it.as_token().is_some());
        let keywords = keywords.count();
        let random = children.first().is_some_and(|it| it.kind() == RANDCASE_KW);
        // The expression, and `inside` or `matches` if either follows it.
        let mut items = keywords + usize::from(!random);
        if !random
            && children
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
            && (random
                || children
                    .get(keywords)
                    .is_some_and(|it| it.kind() == PAREN_EXPR))
            && end + 1 == children.len()
            && children[end].kind() == ENDCASE_KW;
        if !plain {
            return self.verbatim(stmt);
        }
        self.shell(&children[..items], &children[items..end], &children[end..])
    }

    /// The values or `default`, a `:` straight after them, and the
    /// statement, which follows the rule for any statement.
    pub(super) fn case_item(&mut self, item: &SyntaxNode) -> Doc {
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

    /// `for`, `foreach`, `while`, `repeat` or `forever`, its header if it has
    /// one, and the statement it repeats.
    pub(super) fn loop_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        match &significant_children(stmt)[..] {
            [NodeOrToken::Token(keyword), NodeOrToken::Node(body)]
                if stmt.kind() == FOREVER_STMT =>
            {
                Doc::concat([self.token(keyword), self.body(body)])
            }
            [
                NodeOrToken::Token(keyword),
                NodeOrToken::Node(header),
                NodeOrToken::Node(body),
            ] if matches!(header.kind(), PAREN_EXPR | FOREACH_HEADER) => Doc::concat([
                self.token(keyword),
                Doc::Space,
                self.node(header),
                self.body(body),
            ]),
            _ => self.verbatim(stmt),
        }
    }

    /// The initialisation, condition and step of a `for`, with a space after
    /// each `;` that has a clause after it, and none inside the parentheses.
    pub(super) fn for_header(&mut self, header: &SyntaxNode) -> Doc {
        let children = significant_children(header);
        let plain = match &children[..] {
            [open, inner @ .., close] => {
                open.kind() == L_PAREN
                    && close.kind() == R_PAREN
                    && inner.iter().filter(|it| it.kind() == SEMICOLON).count() == 2
                    && inner
                        .iter()
                        .all(|it| it.as_node().is_some() || matches!(it.kind(), COMMA | SEMICOLON))
            }
            _ => false,
        };
        if !plain {
            return self.verbatim(header);
        }

        // Spaced as one line, but a clause after a `;` may start a line of
        // its own: all three break together, under the first, or a
        // continuation in if that would pass the width.
        let (open, inner, close) = (
            &children[0],
            &children[1..children.len() - 1],
            &children[children.len() - 1],
        );
        let mut clauses = Vec::new();
        let mut prev = None;
        for element in inner {
            let separator = match prev {
                Some(prev)
                    if SyntaxElement::kind(prev) == SEMICOLON && element.kind() != SEMICOLON =>
                {
                    Doc::Line
                }
                prev => separation(prev, element),
            };
            clauses.extend([separator, self.element(element)]);
            prev = Some(element);
        }
        let clauses = Doc::concat(clauses);
        let (open, close) = (self.element(open), self.element(close));
        let aligned = Doc::concat([open.clone(), Doc::align(clauses.clone()), close.clone()]);
        let indented = Doc::concat([
            open,
            Doc::indent(Doc::indent(Doc::concat([Doc::SoftLine, clauses]))),
            Doc::SoftLine,
            close,
        ]);
        Doc::group(Doc::prefer(aligned, indented))
    }

    /// The array and, in brackets, the loop's variables: a space after each
    /// comma with a name after it, and none elsewhere.
    pub(super) fn foreach_header(&mut self, header: &SyntaxNode) -> Doc {
        let children = significant_children(header);
        let plain = match &children[..] {
            [open, NodeOrToken::Node(_), bracket, names @ .., end, close] => {
                open.kind() == L_PAREN
                    && bracket.kind() == L_BRACK
                    && end.kind() == R_BRACK
                    && close.kind() == R_PAREN
                    && names
                        .iter()
                        .all(|it| matches!(it.kind(), IDENT | ESCAPED_IDENT | COMMA))
            }
            _ => false,
        };
        if !plain {
            return self.verbatim(header);
        }
        let mut docs = Vec::new();
        let mut prev = None;
        for child in &children {
            if prev == Some(COMMA) && child.kind() != R_BRACK {
                docs.push(Doc::Space);
            }
            docs.push(self.element(child));
            prev = Some(child.kind());
        }
        Doc::concat(docs)
    }

    /// `@` or `#` and what follows it, with no space between.
    pub(super) fn control(&mut self, control: &SyntaxNode) -> Doc {
        match &significant_children(control)[..] {
            [NodeOrToken::Token(op), operand] => {
                Doc::concat([self.token(op), self.element(operand)])
            }
            _ => self.verbatim(control),
        }
    }

    /// An assignment, a call or an expression, and its `;`.
    pub(super) fn expr_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
        let children = significant_children(stmt);
        match &children[..] {
            [NodeOrToken::Node(_), NodeOrToken::Token(semicolon)]
            | [NodeOrToken::Token(semicolon)]
                if semicolon.kind() == SEMICOLON =>
            {
                self.aligning(&children)
            }
            _ => self.verbatim(stmt),
        }
    }

    /// The statement a keyword or a header runs, on the same line. One that
    /// does not fit breaks inside the header or itself rather than below the
    /// header: the guide lets a statement wrap there only inside `begin` and
    /// `end`, which are tokens the formatter may not add. A comment before a
    /// statement that is not a block puts it one level in, on the line after.
    pub(super) fn body(&mut self, body: &SyntaxNode) -> Doc {
        match body.kind() {
            BLOCK | TIMING_STMT | CONSTRAINT_BLOCK => Doc::concat([Doc::Space, self.node(body)]),
            _ => Doc::concat([
                Doc::Space,
                match self.comments.breaks_before(body) {
                    true => Doc::indent(Doc::HardLine),
                    false => Doc::nil(),
                },
                Doc::indent(self.comments.leading(body)),
                self.layout(body),
                self.comments.trailing(body),
            ]),
        }
    }

    /// `assign`, an optional strength and delay, and assignments between
    /// commas; or `defparam` and its overrides.
    pub(super) fn continuous_assign(&mut self, assign: &SyntaxNode) -> Doc {
        let children = significant_children(assign);
        let plain = match &children[..] {
            [keyword, rest @ .., semicolon] => {
                matches!(keyword.kind(), ASSIGN_KW | DEFPARAM_KW)
                    && semicolon.kind() == SEMICOLON
                    && rest
                        .iter()
                        .all(|it| it.as_node().is_some() || it.kind() == COMMA)
            }
            _ => false,
        };
        match plain {
            true => self.aligning(&children),
            false => self.verbatim(assign),
        }
    }

    /// `elements` spaced, the operator of the first assignment among them
    /// lined up with the others of its table.
    pub(super) fn aligning(&mut self, elements: &[SyntaxElement]) -> Doc {
        let mut docs = Vec::new();
        let mut prev = None;
        let mut aligned = false;
        for element in elements {
            docs.push(separation(prev, element));
            docs.push(match element {
                NodeOrToken::Node(node) if node.kind() == ASSIGNMENT && !aligned => {
                    aligned = true;
                    Doc::concat([
                        self.comments.leading(node),
                        self.assignment(node, Some(0)),
                        self.comments.trailing(node),
                    ])
                }
                _ => self.element(element),
            });
            prev = Some(element);
        }
        Doc::concat(docs)
    }

    /// A target, an operator, an optional delay and a value, the target
    /// ending the cell `op` of its table, if it is given one.
    pub(super) fn assignment(&mut self, assignment: &SyntaxNode, op: Option<usize>) -> Doc {
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
        if !plain {
            return self.verbatim(assignment);
        }
        let (target, rest) = children.split_at(1);
        Doc::concat([
            self.spaced(target),
            op.map_or_else(Doc::nil, Doc::Cell),
            separation(target.first(), &rest[0]),
            self.spaced(rest),
        ])
    }
}

/// Whether `node` is a statement of nothing but `;`.
pub(super) fn is_empty_stmt(node: &SyntaxNode) -> bool {
    node.kind() == EXPR_STMT
        && matches!(&significant_children(node)[..], [semicolon] if semicolon.kind() == SEMICOLON)
}
