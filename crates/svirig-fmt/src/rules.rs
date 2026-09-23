//! The rules, one per construct, and the fallback that writes a node no rule
//! lays out as it was read.
//!
//! A rule writes every token of its node, and takes every comment the comment
//! map gives to the node, its descendants or its tokens.

use rowan::NodeOrToken;
use svirig_syntax::{SyntaxElement, SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken};

use crate::comments::Comments;
use crate::doc::Doc;
use crate::verbatim::verbatim;

/// The document for the file `root` holds, and the nodes no rule laid out.
pub(crate) fn write(root: &SyntaxNode, source: &str) -> (Doc, Vec<SyntaxNode>) {
    let mut writer = Writer {
        source,
        comments: Comments::new(root, source),
        unformatted: Vec::new(),
        tight: false,
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
    /// Whether what is being written is inside `[…]`, where operators take
    /// no space and nothing breaks.
    tight: bool,
}

impl Writer<'_> {
    /// Items one after another, each on lines of its own.
    fn source_file(&mut self, file: &SyntaxNode) -> Doc {
        let items = self.items(&significant_children(file));
        Doc::concat([self.comments.head(), items])
    }

    /// A module, interface, program, package, class, function or task: its
    /// header on one line, its items indented below, and its end on a line of
    /// its own. A prototype is its header alone.
    fn scope(&mut self, scope: &SyntaxNode) -> Doc {
        let children = significant_children(scope);
        let semicolon = children.iter().position(|it| it.kind() == SEMICOLON);
        let Some(semicolon) = semicolon else {
            return self.verbatim(scope);
        };
        let (header, rest) = children.split_at(semicolon + 1);
        // A preprocessor construct in the header would need its line kept.
        let header_is_plain = header.iter().all(|it| {
            it.as_node().is_none_or(|node| {
                matches!(
                    node.kind(),
                    ATTRIBUTES | IMPORT_DECL | PARAM_PORT_LIST | PORT_LIST | TYPE_REF | ARG_LIST
                )
            })
        });
        if !header_is_plain {
            return self.verbatim(scope);
        }
        if rest.is_empty() {
            return self.spaced(header);
        }
        let end = rest.iter().position(|it| it.as_token().is_some());
        let Some((body, end)) = end.map(|end| rest.split_at(end)) else {
            return self.verbatim(scope);
        };
        if end.iter().any(|it| it.as_node().is_some()) {
            return self.verbatim(scope);
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
    /// otherwise it stays on the line if all of it fits. A list holding a
    /// directive is always broken, since the directive needs lines of its own.
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
        let preprocessed = entries
            .iter()
            .any(|it| it.as_node().is_some() && !matches!(it.kind(), PORT | PARAM_DECL | ARG));
        if broken || preprocessed {
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

    /// A module or interface, its parameters, and its instances.
    fn instantiation(&mut self, instantiation: &SyntaxNode) -> Doc {
        let children = significant_children(instantiation);
        let mut kinds = children
            .iter()
            .map(|it| it.kind())
            .skip_while(|&it| it == ATTRIBUTES);
        // The order past the type is the parser's to check; the layout is the
        // same in any order.
        let plain = kinds.next() == Some(TYPE_REF)
            && children.last().is_some_and(|it| it.kind() == SEMICOLON)
            && kinds.all(|kind| matches!(kind, HASH | ARG_LIST | INSTANCE | COMMA | SEMICOLON));
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(instantiation),
        }
    }

    /// An instance's name, its dimensions, and its connections.
    fn instance(&mut self, instance: &SyntaxNode) -> Doc {
        let children = significant_children(instance);
        let plain = match &children[..] {
            [
                NodeOrToken::Token(_),
                rest @ ..,
                NodeOrToken::Node(connections),
            ] => connections.kind() == ARG_LIST && rest.iter().all(|it| it.kind() == DIMENSION),
            _ => false,
        };
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(instance),
        }
    }

    /// A connection or a parameter's value: `.name(value)`, `.name`, `.*`,
    /// or a value alone, with no space inside. The `(` of a named connection
    /// lines up with the others in its list.
    fn arg(&mut self, arg: &SyntaxNode) -> Doc {
        let children = significant_children(arg);
        let plain = match &children[..] {
            [] | [NodeOrToken::Node(_)] => true,
            [NodeOrToken::Token(dot), NodeOrToken::Token(_), rest @ ..] => {
                dot.kind() == DOT && matches!(rest, [] | [NodeOrToken::Node(_)])
            }
            _ => false,
        };
        if !plain {
            return self.verbatim(arg);
        }
        let mut docs: Vec<Doc> = children.iter().map(|it| self.element(it)).collect();
        if let [
            NodeOrToken::Token(_),
            NodeOrToken::Token(_),
            NodeOrToken::Node(_),
        ] = &children[..]
            && is_connection(arg)
        {
            docs.insert(2, Doc::Cell(0));
        }
        Doc::concat(docs)
    }

    /// A variable or net: its qualifiers and type, then its names. The names
    /// of consecutive declarations line up; a type that is itself a block,
    /// such as a `struct`, has nothing on its last line to line them up with.
    fn var_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        let attributes = children.iter().take_while(|it| it.kind() == ATTRIBUTES);
        let attributes = attributes.count();
        let qualifiers = children[attributes..]
            .iter()
            .take_while(|it| it.kind().is_keyword());
        let mut names = attributes + qualifiers.count();
        let typed = children
            .get(names)
            .is_some_and(|it| matches!(it.kind(), TYPE_REF | ENUM_TYPE | STRUCT_TYPE | UNION_TYPE));
        let aligned = !typed || children[names].kind() == TYPE_REF;
        names += usize::from(typed);
        let rest = match children.last() {
            Some(it) if it.kind() == SEMICOLON => &children[names..children.len() - 1],
            _ => &children[names..],
        };
        let plain = names > attributes
            && !rest.is_empty()
            && rest.iter().enumerate().all(|(at, it)| match at % 2 {
                0 => it.kind() == DECLARATOR,
                _ => it.kind() == COMMA,
            })
            && rest.len() % 2 == 1;
        if !plain {
            return self.verbatim(decl);
        }
        let (head, names) = children.split_at(names);
        Doc::concat([
            self.spaced(head),
            if aligned { Doc::Cell(0) } else { Doc::nil() },
            Doc::Space,
            self.spaced(names),
        ])
    }

    /// A `parameter` or `localparam`: its keyword, its type, and its names
    /// and values. Consecutive ones line up in four columns: the keyword, the
    /// type, the name, and the `=`. A type that is itself a block has nothing
    /// on its last line to line them up with.
    fn param_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        let keyword = children
            .first()
            .is_some_and(|it| matches!(it.kind(), PARAMETER_KW | LOCALPARAM_KW));
        let mut names = usize::from(keyword);
        if children.get(names).is_some_and(|it| it.kind() == TYPE_KW) {
            names += 1;
        }
        let typed = children
            .get(names)
            .is_some_and(|it| matches!(it.kind(), TYPE_REF | ENUM_TYPE | STRUCT_TYPE | UNION_TYPE));
        let aligned = !typed || children[names].kind() == TYPE_REF;
        names += usize::from(typed);
        let rest = match children.last() {
            Some(it) if it.kind() == SEMICOLON => &children[names..children.len() - 1],
            _ => &children[names..],
        };
        let plain = !rest.is_empty()
            && rest.len() % 2 == 1
            && rest.iter().enumerate().all(|(at, it)| match at % 2 {
                0 => it.kind() == DECLARATOR,
                _ => it.kind() == COMMA,
            });
        if !plain {
            return self.verbatim(decl);
        }

        let (head, names) = children.split_at(names);
        let (keyword, kind) = head.split_at(usize::from(keyword));
        let mut docs = vec![self.spaced(keyword)];
        if aligned && !keyword.is_empty() {
            docs.push(Doc::Cell(0));
        }
        // Separation before the first element is the parent's to request.
        if !keyword.is_empty() {
            docs.push(Doc::Space);
        }
        docs.push(self.spaced(kind));
        if aligned {
            docs.push(Doc::Cell(1));
        }
        if !head.is_empty() {
            docs.push(Doc::Space);
        }
        let mut prev = None;
        for name in names {
            docs.push(separation(prev, name));
            docs.push(match name {
                NodeOrToken::Node(first) if prev.is_none() && aligned => Doc::concat([
                    self.comments.leading(first),
                    self.declarator(first, true),
                    self.comments.trailing(first),
                ]),
                _ => self.element(name),
            });
            prev = Some(name);
        }
        Doc::concat(docs)
    }

    /// `typedef`, the type, and its new name. A type with a body, an `enum`
    /// or a `struct`, ends its last line with the `}` the name follows.
    fn typedef(&mut self, typedef: &SyntaxNode) -> Doc {
        let children = significant_children(typedef);
        match &children[..] {
            [keyword, .., name, semicolon]
                if keyword.kind() == TYPEDEF_KW
                    && name.kind() == DECLARATOR
                    && semicolon.kind() == SEMICOLON =>
            {
                self.spaced(&children)
            }
            _ => self.verbatim(typedef),
        }
    }

    /// An `enum`, `struct` or `union`: its keyword and what qualifies it on
    /// the line of the `{`, then one variant or member per line, then `}`.
    /// Members line up as declarations do, and variants on their `=`.
    fn body_type(&mut self, body_type: &SyntaxNode) -> Doc {
        let children = significant_children(body_type);
        let Some(open) = children.iter().position(|it| it.kind() == L_BRACE) else {
            return self.verbatim(body_type);
        };
        let Some(close) = children.iter().position(|it| it.kind() == R_BRACE) else {
            return self.verbatim(body_type);
        };
        let entry = match body_type.kind() {
            ENUM_TYPE => ENUM_VARIANT,
            _ => STRUCT_MEMBER,
        };
        let plain = open < close
            && children[open + 1..close].iter().all(|it| match it {
                NodeOrToken::Node(node) => node.kind() == entry || is_preproc(node),
                NodeOrToken::Token(token) => entry == ENUM_VARIANT && token.kind() == COMMA,
            })
            && children[close + 1..]
                .iter()
                .all(|it| it.kind() == DIMENSION);
        if !plain {
            return self.verbatim(body_type);
        }
        self.shell(
            &children[..=open],
            &children[open + 1..close],
            &children[close..],
        )
    }

    /// A variant's name, and its value after an `=` that lines up with the
    /// others of its `enum`; or the preprocessor construct that stands for
    /// variants.
    fn enum_variant(&mut self, variant: &SyntaxNode) -> Doc {
        let children = significant_children(variant);
        if let [NodeOrToken::Node(preproc)] = &children[..] {
            return self.node(preproc);
        }
        let value = children.iter().position(|it| it.kind() == EQ);
        let name = &children[..value.unwrap_or(children.len())];
        let plain = name.first().is_some_and(|it| it.kind() == IDENT)
            && name[1..].iter().all(|it| it.kind() == DIMENSION)
            && value.is_none_or(|value| matches!(&children[value + 1..], [NodeOrToken::Node(_)]));
        if !plain {
            return self.verbatim(variant);
        }
        let mut docs: Vec<Doc> = name.iter().map(|it| self.element(it)).collect();
        if let Some(value) = value {
            docs.extend([Doc::Cell(0), Doc::Space, self.spaced(&children[value..])]);
        }
        Doc::concat(docs)
    }

    /// A port: its direction and whatever else qualifies it, its type, and
    /// its name. Consecutive ports line up in three columns, the direction
    /// padded so that the types start together, as three in four port lists
    /// in the corpus have them. A type that is itself a block falls back.
    fn port(&mut self, port: &SyntaxNode) -> Doc {
        let children = significant_children(port);
        let attributes = children.iter().take_while(|it| it.kind() == ATTRIBUTES);
        let attributes = attributes.count();
        let qualifiers = children[attributes..]
            .iter()
            .take_while(|it| it.kind().is_keyword());
        let qualifiers = attributes + qualifiers.count();
        let typed = children
            .get(qualifiers)
            .is_some_and(|it| it.kind() == TYPE_REF);
        let names = qualifiers + usize::from(typed);
        let [NodeOrToken::Node(name)] = &children[names..] else {
            return self.verbatim(port);
        };
        if name.kind() != DECLARATOR {
            return self.verbatim(port);
        }
        let (qualifiers, kind) = children[..names].split_at(qualifiers);
        let mut docs = vec![self.spaced(qualifiers), Doc::Cell(0)];
        // Separation before the first element is the parent's to request.
        if !qualifiers.is_empty() {
            docs.push(Doc::Space);
        }
        docs.extend([self.spaced(kind), Doc::Cell(1)]);
        if names > 0 {
            docs.push(Doc::Space);
        }
        docs.extend([
            self.comments.leading(name),
            self.declarator(name, true),
            self.comments.trailing(name),
        ]);
        Doc::concat(docs)
    }

    /// A type's name and what qualifies it: a space between words and before
    /// the packed dimensions, and none around `::` or `.`, after `#`, or
    /// between dimensions.
    fn type_ref(&mut self, type_ref: &SyntaxNode) -> Doc {
        let children = significant_children(type_ref);
        let plain = children.iter().all(|it| match it {
            NodeOrToken::Node(node) => matches!(node.kind(), ARG_LIST | DIMENSION),
            NodeOrToken::Token(_) => true,
        });
        if !plain {
            return self.verbatim(type_ref);
        }
        let mut docs = Vec::new();
        let mut prev: Option<&SyntaxElement> = None;
        for child in &children {
            let tight = prev.is_some_and(|prev| {
                (prev.kind() == DIMENSION && child.kind() == DIMENSION)
                    || prev.kind() == DOT
                    || child.kind() == DOT
            });
            docs.push(match tight {
                true => Doc::nil(),
                false => separation(prev, child),
            });
            docs.push(self.element(child));
            prev = Some(child);
        }
        Doc::concat(docs)
    }

    /// A name, its unpacked dimensions against it, and its initial value,
    /// whose `=` lines up with the others of its table if `aligned`.
    fn declarator(&mut self, declarator: &SyntaxNode, aligned: bool) -> Doc {
        let children = significant_children(declarator);
        let dimensions = children
            .iter()
            .skip(1)
            .take_while(|it| it.kind() == DIMENSION);
        let value = 1 + dimensions.count();
        let plain = children
            .first()
            .is_some_and(|it| matches!(it.kind(), IDENT | ESCAPED_IDENT))
            && match &children[value..] {
                [] => true,
                [eq, NodeOrToken::Node(_)] => eq.kind() == EQ,
                _ => false,
            };
        if !plain {
            return self.verbatim(declarator);
        }
        let (name, value) = children.split_at(value);
        let mut docs: Vec<Doc> = name.iter().map(|it| self.element(it)).collect();
        if !value.is_empty() {
            if aligned {
                docs.push(Doc::Cell(2));
            }
            docs.extend([Doc::Space, self.spaced(value)]);
        }
        Doc::concat(docs)
    }

    /// `[`, a size, a range or a type, and `]`, with no space inside.
    fn dimension(&mut self, dimension: &SyntaxNode) -> Doc {
        let children = significant_children(dimension);
        let plain = match &children[..] {
            [open, inner @ .., close] => {
                open.kind() == L_BRACK
                    && close.kind() == R_BRACK
                    && match inner {
                        [] | [NodeOrToken::Node(_)] => true,
                        [NodeOrToken::Token(star)] => star.kind() == STAR,
                        [NodeOrToken::Node(_), colon, NodeOrToken::Node(_)] => {
                            colon.kind() == COLON
                        }
                        _ => false,
                    }
            }
            _ => false,
        };
        if !plain {
            return self.verbatim(dimension);
        }
        let tight = std::mem::replace(&mut self.tight, true);
        let doc = Doc::concat(children.iter().map(|it| self.element(it)));
        self.tight = tight;
        doc
    }

    /// A callee and its arguments in parentheses, with no space between, laid
    /// out as [`Writer::arguments`] says. A parameterised callee or a `with`
    /// clause falls back.
    fn call_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        match &children[..] {
            [NodeOrToken::Node(callee), NodeOrToken::Node(list)]
                if list.kind() == ARG_LIST && is_arg_list(list, ARG) =>
            {
                let callee = self.node(callee);
                self.arguments(callee, list)
            }
            _ => self.verbatim(expr),
        }
    }

    /// A macro's name and its arguments, laid out as a call's are. An
    /// argument is text the grammar never parsed, so it is written as it was
    /// read, from its first token to its last.
    fn macro_call(&mut self, call: &SyntaxNode) -> Doc {
        let children = significant_children(call);
        match &children[..] {
            [NodeOrToken::Token(name)] => self.token(name),
            [NodeOrToken::Token(name), NodeOrToken::Node(list)] if is_arg_list(list, MACRO_ARG) => {
                let name = self.token(name);
                self.arguments(name, list)
            }
            _ => self.verbatim(call),
        }
    }

    /// `callee`, then `list` in parentheses with no space inside them. A
    /// broken list packs its arguments under the first; if a line would still
    /// pass the width, or start past half of it, it breaks after `(` instead,
    /// packs them a continuation in, and puts `)` on a line of its own. The
    /// `(` stays on the callee's line, where a macro's arguments must start.
    fn arguments(&mut self, callee: Doc, list: &SyntaxNode) -> Doc {
        let entries = significant_children(list);
        let (open, close) = (&entries[0], &entries[entries.len() - 1]);
        let open = Doc::concat([self.comments.leading(list), self.element(open)]);

        // An argument and the comma after it, then the separator.
        let mut parts = Vec::new();
        for entry in &entries[1..entries.len() - 1] {
            match entry {
                NodeOrToken::Node(arg) => {
                    if !parts.is_empty() {
                        parts.push(Doc::Line);
                    }
                    parts.push(self.node(arg));
                }
                NodeOrToken::Token(comma) => {
                    let arg = parts.pop().unwrap_or_else(Doc::nil);
                    parts.push(Doc::concat([arg, self.token(comma)]));
                }
            }
        }
        let close = Doc::concat([self.element(close), self.comments.trailing(list)]);
        if parts.is_empty() {
            return Doc::concat([callee, open, close]);
        }
        if self.tight {
            let parts = parts.into_iter().map(|part| match part {
                Doc::Line => Doc::Space,
                part => part,
            });
            return Doc::concat([callee, open, Doc::concat(parts), close]);
        }
        let aligned = Doc::concat([
            open.clone(),
            Doc::align(Doc::Fill(parts.clone())),
            close.clone(),
        ]);
        let indented = Doc::concat([
            open,
            Doc::indent(Doc::indent(Doc::concat([Doc::Line, Doc::Fill(parts)]))),
            Doc::SoftLine,
            close,
        ]);
        Doc::group(Doc::concat([callee, Doc::prefer(aligned, indented)]))
    }

    /// A base, `.` or `::`, and a member, with no space between, since they
    /// name one thing; nothing breaks there.
    fn member(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        match &children[..] {
            [NodeOrToken::Node(_), op, NodeOrToken::Token(_)]
                if matches!(op.kind(), DOT | COLON_COLON) =>
            {
                Doc::concat(children.iter().map(|it| self.element(it)))
            }
            _ => self.verbatim(expr),
        }
    }

    /// A base and a select in `[…]`, with no space inside the brackets but
    /// one on either side of `+:` or `-:`, which would otherwise read as
    /// part of an operand.
    fn index_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        let plain = match &children[..] {
            [NodeOrToken::Node(_), open, NodeOrToken::Node(_), close] => {
                open.kind() == L_BRACK && close.kind() == R_BRACK
            }
            [
                NodeOrToken::Node(_),
                open,
                NodeOrToken::Node(_),
                op,
                NodeOrToken::Node(_),
                close,
            ] => {
                open.kind() == L_BRACK
                    && matches!(op.kind(), COLON | PLUS_COLON | MINUS_COLON)
                    && close.kind() == R_BRACK
            }
            _ => false,
        };
        if !plain {
            return self.verbatim(expr);
        }
        let base = self.element(&children[0]);
        let tight = std::mem::replace(&mut self.tight, true);
        let mut docs = vec![base];
        for child in &children[1..] {
            let spaced = matches!(child.kind(), PLUS_COLON | MINUS_COLON);
            let space = || if spaced { Doc::Space } else { Doc::nil() };
            docs.extend([space(), self.element(child), space()]);
        }
        self.tight = tight;
        Doc::concat(docs)
    }

    /// A name or a literal, its tokens as they are, or the macro call that
    /// stands for it. One written with space inside, such as a sized literal
    /// split after its base, falls back, so that its pieces are not joined
    /// into something that lexes otherwise.
    fn adjacent(&mut self, node: &SyntaxNode) -> Doc {
        let children = significant_children(node);
        if let [NodeOrToken::Node(call)] = &children[..] {
            return self.node(call);
        }
        let adjacent = children.windows(2).all(|pair| {
            pair[0]
                .as_token()
                .zip(pair[1].as_token())
                .is_some_and(|(prev, next)| prev.text_range().end() == next.text_range().start())
        });
        match adjacent && children.iter().all(|it| it.as_token().is_some()) {
            true => Doc::concat(children.iter().map(|it| self.element(it))),
            false => self.verbatim(node),
        }
    }

    /// Operands and the operators between them, a space on either side of
    /// each. A chain of one operator is one group, which breaks after every
    /// operator or none, its operands aligned under the first; an operand
    /// with another operator is a group of its own. Inside `[…]`, nothing
    /// breaks and no space is written unless the tokens would run together.
    fn bin_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let Some(op) = operator(expr) else {
            return self.verbatim(expr);
        };
        let mut docs = Vec::new();
        self.chain(expr, op, &mut docs);
        match self.tight {
            true => Doc::concat(docs),
            false => Doc::group(Doc::align(Doc::concat(docs))),
        }
    }

    /// The operands of `expr` and those of its operands with the operator
    /// `op` too, in order, with the operators between them.
    fn chain(&mut self, expr: &SyntaxNode, op: SyntaxKind, docs: &mut Vec<Doc>) {
        let children = significant_children(expr);
        let [
            NodeOrToken::Node(lhs),
            NodeOrToken::Token(token),
            NodeOrToken::Node(rhs),
        ] = &children[..]
        else {
            unreachable!("`operator` checked the shape");
        };
        self.operand(lhs, op, docs);
        let spaced = !self.tight || rhs.kind() == UNARY_EXPR;
        let (before, after) = match (spaced, self.tight) {
            (false, _) => (Doc::nil(), Doc::nil()),
            (true, true) => (Doc::Space, Doc::Space),
            (true, false) => (Doc::Space, Doc::Line),
        };
        docs.extend([before, self.token(token), after]);
        self.operand(rhs, op, docs);
    }

    fn operand(&mut self, operand: &SyntaxNode, op: SyntaxKind, docs: &mut Vec<Doc>) {
        if operand.kind() == BIN_EXPR && operator(operand) == Some(op) {
            docs.push(self.comments.leading(operand));
            self.chain(operand, op, docs);
            docs.push(self.comments.trailing(operand));
        } else {
            docs.push(self.node(operand));
        }
    }

    /// `for`, `foreach`, `while`, `repeat` or `forever`, its header if it has
    /// one, and the statement it repeats.
    fn loop_stmt(&mut self, stmt: &SyntaxNode) -> Doc {
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
    fn for_header(&mut self, header: &SyntaxNode) -> Doc {
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
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(header),
        }
    }

    /// The array and, in brackets, the loop's variables: a space after each
    /// comma with a name after it, and none elsewhere.
    fn foreach_header(&mut self, header: &SyntaxNode) -> Doc {
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

    /// An expression in parentheses, with no space inside them. An event
    /// list falls back.
    fn paren_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        match &children[..] {
            [open, NodeOrToken::Node(_), close] | [open, close]
                if open.kind() == L_PAREN && close.kind() == R_PAREN =>
            {
                Doc::concat(children.iter().map(|it| self.element(it)))
            }
            _ => self.verbatim(expr),
        }
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

    /// Each of `elements` on lines of its own, and each run of consecutive
    /// declarations of one kind a table. A comma stays on the line of the
    /// item before it, and in its table; any other token would be stray.
    fn items(&mut self, elements: &[SyntaxElement]) -> Doc {
        let mut docs = Vec::new();
        // The kind of the declarations in the run, and what they wrote.
        let mut run: Option<(SyntaxKind, Vec<Doc>)> = None;
        let mut prev: Option<&SyntaxElement> = None;
        for element in elements {
            let doc = match element {
                // A macro that stands for a statement is often written with a
                // `;` after it, which the grammar reads as a statement of its
                // own.
                NodeOrToken::Node(item)
                    if is_empty_stmt(item) && prev.is_some_and(|it| it.kind() == MACRO_CALL) =>
                {
                    self.node(item)
                }
                NodeOrToken::Node(item) => self.item(item),
                NodeOrToken::Token(comma) if comma.kind() == COMMA => self.token(comma),
                NodeOrToken::Token(token) => Doc::concat([Doc::HardLine, self.token(token)]),
            };
            let kind = element.kind();
            let joins = run
                .as_ref()
                .is_some_and(|(run, _)| kind == *run || kind == COMMA);
            if !joins {
                if let Some((_, rows)) = run.take() {
                    docs.push(Doc::table(Doc::concat(rows)));
                }
                if matches!(
                    kind,
                    VAR_DECL | PARAM_DECL | PORT | STRUCT_MEMBER | ENUM_VARIANT
                ) {
                    run = Some((kind, Vec::new()));
                }
            }
            match &mut run {
                Some((_, rows)) => rows.push(doc),
                None => docs.push(doc),
            }
            prev = Some(element);
        }
        if let Some((_, rows)) = run {
            docs.push(Doc::table(Doc::concat(rows)));
        }
        Doc::concat(docs)
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
            MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL | CLASS_DECL
            | FUNCTION_DECL | TASK_DECL => self.scope(node),
            CONDITIONAL_REGION => self.conditional_region(node),
            CONTINUOUS_ASSIGN => self.continuous_assign(node),
            PROCEDURAL_BLOCK => self.procedural_block(node),
            BLOCK => self.block(node),
            TIMING_STMT => self.timing_stmt(node),
            EVENT_CONTROL | DELAY_CONTROL => self.control(node),
            EXPR_STMT => self.expr_stmt(node),
            IF_STMT => self.if_stmt(node),
            FOR_STMT | FOREACH_STMT | WHILE_STMT | REPEAT_STMT | FOREVER_STMT => {
                self.loop_stmt(node)
            }
            FOREACH_HEADER => self.foreach_header(node),
            CASE_STMT => self.case_stmt(node),
            CASE_ITEM => self.case_item(node),
            ASSIGNMENT => self.assignment(node),
            VAR_DECL | STRUCT_MEMBER => self.var_decl(node),
            TYPEDEF => self.typedef(node),
            ENUM_TYPE | STRUCT_TYPE | UNION_TYPE => self.body_type(node),
            ENUM_VARIANT => self.enum_variant(node),
            TYPE_REF => self.type_ref(node),
            PARAM_DECL => self.param_decl(node),
            DECLARATOR => self.declarator(node, false),
            PORT => self.port(node),
            DIMENSION => self.dimension(node),
            BIN_EXPR => self.bin_expr(node),
            CALL_EXPR => self.call_expr(node),
            MACRO_CALL => self.macro_call(node),
            FIELD_EXPR | SCOPE_EXPR => self.member(node),
            INDEX_EXPR => self.index_expr(node),
            NAME_REF | LITERAL_EXPR => self.adjacent(node),
            PARAM_PORT_LIST | PORT_LIST
                if node.parent().is_some_and(|parent| {
                    matches!(parent.kind(), MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL)
                }) =>
            {
                self.list(node, true)
            }
            // A class's parameters, the arguments to its base's constructor,
            // and a function's or task's arguments.
            PARAM_PORT_LIST | ARG_LIST | PORT_LIST
                if node.parent().is_some_and(|parent| {
                    matches!(parent.kind(), CLASS_DECL | FUNCTION_DECL | TASK_DECL)
                }) =>
            {
                self.list(node, false)
            }
            INSTANTIATION => self.instantiation(node),
            INSTANCE => self.instance(node),
            // Named connections go one per line, so that they can be aligned.
            ARG_LIST
                if node
                    .parent()
                    .is_some_and(|parent| matches!(parent.kind(), INSTANCE | INSTANTIATION)) =>
            {
                let named = node
                    .children()
                    .any(|arg| first_token(&arg).is_some_and(|token| token.kind() == DOT));
                Doc::table(self.list(node, named))
            }
            ARG => self.arg(node),
            PAREN_EXPR
                if node
                    .parent()
                    .is_some_and(|parent| parent.kind() == FOR_STMT) =>
            {
                self.for_header(node)
            }
            PAREN_EXPR => self.paren_expr(node),
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

/// A space between two elements on a line, but none before `,`, `;` or `)`,
/// none after `#` or `(`, and none around `::`. None either between a base
/// class and the arguments to its constructor, or a function or task and its
/// arguments, which read as calls. What comes before the first is its
/// parent's to separate.
fn separation(prev: Option<&SyntaxElement>, next: &SyntaxElement) -> Doc {
    let Some(prev) = prev else {
        return Doc::nil();
    };
    let call = match next.kind() {
        ARG_LIST => prev.kind() == TYPE_REF,
        PORT_LIST => next
            .parent()
            .is_some_and(|parent| matches!(parent.kind(), FUNCTION_DECL | TASK_DECL)),
        _ => false,
    };
    let tight = call
        || matches!(prev.kind(), HASH | L_PAREN | COLON_COLON)
        || matches!(next.kind(), COMMA | SEMICOLON | R_PAREN | COLON_COLON);
    match tight {
        true => Doc::nil(),
        false => Doc::Space,
    }
}

/// Whether `list` is `(`, then entries of the kind `arg` between commas, then
/// `)`.
fn is_arg_list(list: &SyntaxNode, arg: SyntaxKind) -> bool {
    match &significant_children(list)[..] {
        [open, inner @ .., close] => {
            open.kind() == L_PAREN
                && close.kind() == R_PAREN
                && inner
                    .iter()
                    .all(|it| it.kind() == arg || it.kind() == COMMA)
        }
        _ => false,
    }
}

/// Whether `node` stands where the preprocessor, not the grammar, decides
/// what goes.
fn is_preproc(node: &SyntaxNode) -> bool {
    matches!(node.kind(), MACRO_CALL | DIRECTIVE | CONDITIONAL_REGION)
}

/// Whether `node` is a statement of nothing but `;`.
fn is_empty_stmt(node: &SyntaxNode) -> bool {
    node.kind() == EXPR_STMT
        && matches!(&significant_children(node)[..], [semicolon] if semicolon.kind() == SEMICOLON)
}

/// The operator of a binary expression with nothing but its operands and
/// the operator between them.
fn operator(expr: &SyntaxNode) -> Option<SyntaxKind> {
    match &significant_children(expr)[..] {
        [
            NodeOrToken::Node(_),
            NodeOrToken::Token(op),
            NodeOrToken::Node(_),
        ] => Some(op.kind()),
        _ => None,
    }
}

/// The child nodes, and the child tokens other than trivia, in order.
fn significant_children(node: &SyntaxNode) -> Vec<SyntaxElement> {
    (node.children_with_tokens())
        .filter(|it| !it.kind().is_trivia())
        .collect()
}

/// Whether `arg` connects a port or a parameter of an instance, as opposed to
/// being an argument to a call. A directive between it and its list does not
/// change which it is.
fn is_connection(arg: &SyntaxNode) -> bool {
    arg.ancestors()
        .skip(1)
        .find(|it| !matches!(it.kind(), CONDITIONAL_BRANCH | CONDITIONAL_REGION))
        .filter(|it| it.kind() == ARG_LIST)
        .and_then(|list| list.parent())
        .is_some_and(|it| matches!(it.kind(), INSTANCE | INSTANTIATION))
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
