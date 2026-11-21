//! Declarations, their types and names, ports, modports, constraints and
//! coverage.

use astli_syntax::{SyntaxElement, SyntaxKind::*, SyntaxNode};
use rowan::NodeOrToken;

use super::list::is_arg_list;
use super::preproc::is_preproc;
use super::{Writer, separation, significant_children};
use crate::doc::Doc;

impl Writer<'_> {
    /// A `constraint`'s qualifiers and name, and its block or `;`.
    pub(super) fn constraint_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        match &children[..] {
            [header @ .., last] if header.iter().all(|it| it.as_token().is_some()) => match last {
                NodeOrToken::Node(block) if block.kind() == CONSTRAINT_BLOCK => {
                    Doc::concat([self.spaced(header), Doc::Space, self.node(block)])
                }
                NodeOrToken::Token(semicolon) if semicolon.kind() == SEMICOLON => {
                    self.spaced(&children)
                }
                _ => self.verbatim(decl),
            },
            _ => self.verbatim(decl),
        }
    }

    /// `{`, constraints or bins indented below it, and `}` on a line of its
    /// own.
    pub(super) fn braces(&mut self, block: &SyntaxNode) -> Doc {
        let children = significant_children(block);
        match &children[..] {
            [open, close] if open.kind() == L_BRACE && close.kind() == R_BRACE => {
                Doc::concat([self.element(open), self.element(close)])
            }
            [open, items @ .., close]
                if open.kind() == L_BRACE
                    && close.kind() == R_BRACE
                    && items.iter().all(|it| it.as_node().is_some()) =>
            {
                self.shell(&children[..1], items, &children[children.len() - 1..])
            }
            _ => self.verbatim(block),
        }
    }

    /// A cover point or a cross: its label against its `:`, as 2657 in the
    /// corpus have it to 875, what it covers, and its bins or `;`.
    pub(super) fn cover_point(&mut self, point: &SyntaxNode) -> Doc {
        let children = significant_children(point);
        let labelled = matches!(&children[..], [label, colon, ..]
            if matches!(label.kind(), IDENT | ESCAPED_IDENT) && colon.kind() == COLON);
        let (label, rest) = children.split_at(if labelled { 2 } else { 0 });
        let mut docs = Vec::new();
        if let [label, colon] = label {
            docs.extend([self.element(label), self.element(colon), Doc::Space]);
        }
        match rest {
            [head @ .., NodeOrToken::Node(block)] if block.kind() == BINS_BLOCK => {
                docs.extend([self.spaced(head), Doc::Space, self.node(block)]);
            }
            [.., semicolon] if semicolon.kind() == SEMICOLON => docs.push(self.spaced(rest)),
            _ => return self.verbatim(point),
        }
        Doc::concat(docs)
    }

    /// A set of bins, its `[…]` against its name, and its `=` lined up with
    /// the others of its table, as 1503 pairs of consecutive bins in the
    /// corpus are to 1183.
    pub(super) fn bins(&mut self, bins: &SyntaxNode) -> Doc {
        let mut docs = Vec::new();
        let mut prev: Option<SyntaxElement> = None;
        for child in significant_children(bins) {
            let tight = matches!(child.kind(), L_BRACK | R_BRACK)
                || prev.as_ref().is_some_and(|it| it.kind() == L_BRACK);
            if child.kind() == EQ {
                docs.push(Doc::Cell(0));
            }
            docs.push(match tight {
                true => Doc::nil(),
                false => separation(prev.as_ref(), &child),
            });
            docs.push(self.element(&child));
            prev = Some(child);
        }
        Doc::concat(docs)
    }

    /// A condition, `->`, and what it implies, on the same line.
    pub(super) fn implication(&mut self, implication: &SyntaxNode) -> Doc {
        match &significant_children(implication)[..] {
            [
                NodeOrToken::Node(condition),
                NodeOrToken::Token(arrow),
                NodeOrToken::Node(body),
            ] => Doc::concat([
                self.node(condition),
                Doc::Space,
                self.token(arrow),
                self.body(body),
            ]),
            _ => self.verbatim(implication),
        }
    }

    /// `import` or `export`, the items it names between commas, and `;`.
    /// One that imports a function from C, a node of its own, falls back.
    pub(super) fn import_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        let plain = children.iter().all(|it| it.as_token().is_some())
            && children.last().is_some_and(|it| it.kind() == SEMICOLON);
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(decl),
        }
    }

    /// `modport`, its modports between commas, and `;`.
    pub(super) fn modport_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        let plain = children.first().is_some_and(|it| it.kind() == MODPORT_KW)
            && children.last().is_some_and(|it| it.kind() == SEMICOLON)
            && children[1..children.len() - 1]
                .iter()
                .all(|it| matches!(it.kind(), MODPORT | COMMA));
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(decl),
        }
    }

    /// A modport's name and its ports, which break one per line as a
    /// function's do.
    pub(super) fn modport(&mut self, modport: &SyntaxNode) -> Doc {
        match &significant_children(modport)[..] {
            [NodeOrToken::Token(name), ports] if ports.kind() == PORT_LIST => {
                let name = self.token(name);
                Doc::concat([name, Doc::Space, self.header_list(ports, false)])
            }
            _ => self.verbatim(modport),
        }
    }

    /// A subroutine a modport imports or exports, on one line: its keywords,
    /// its return type and its name, with its ports after it.
    pub(super) fn modport_method(&mut self, method: &SyntaxNode) -> Doc {
        let children = significant_children(method);
        match children.split_last() {
            Some((ports, words)) if ports.kind() == PORT_LIST => self.called(words, Some(ports)),
            _ => self.spaced(&children),
        }
    }

    /// A variable or net: its qualifiers and type, then its names. The names
    /// of consecutive declarations line up; a type that is itself a block,
    /// such as a `struct`, has nothing on its last line to line them up with.
    pub(super) fn var_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        let attributes = children.iter().take_while(|it| it.kind() == ATTRIBUTES);
        let attributes = attributes.count();
        let qualifiers = children[attributes..]
            .iter()
            .take_while(|it| it.kind().is_keyword() || it.kind() == STRENGTH);
        let names = declarators(&children, attributes + qualifiers.count());
        let Some((names, aligned)) = names.filter(|&(names, _)| names > attributes) else {
            return self.verbatim(decl);
        };
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
    pub(super) fn param_decl(&mut self, decl: &SyntaxNode) -> Doc {
        let children = significant_children(decl);
        let keyword = children
            .first()
            .is_some_and(|it| matches!(it.kind(), PARAMETER_KW | LOCALPARAM_KW | SPECPARAM_KW));
        let mut at = usize::from(keyword);
        if children.get(at).is_some_and(|it| it.kind() == TYPE_KW) {
            at += 1;
            // The kind of type it restricts the parameter to, if it names one.
            while children.get(at).is_some_and(|it| {
                matches!(
                    it.kind(),
                    ENUM_KW | STRUCT_KW | UNION_KW | INTERFACE_KW | CLASS_KW
                )
            }) {
                at += 1;
            }
        }
        let Some((names, aligned)) = declarators(&children, at) else {
            return self.verbatim(decl);
        };

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
                NodeOrToken::Node(first) if prev.is_none() && aligned => {
                    self.with_comments(first, |this, it| this.declarator(it, Some(2)))
                }
                _ => self.element(name),
            });
            prev = Some(name);
        }
        Doc::concat(docs)
    }

    /// `typedef`, the type, and its new name. A type with a body, an `enum`
    /// or a `struct`, ends its last line with the `}` the name follows.
    pub(super) fn typedef(&mut self, typedef: &SyntaxNode) -> Doc {
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
    pub(super) fn body_type(&mut self, body_type: &SyntaxNode) -> Doc {
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
    pub(super) fn enum_variant(&mut self, variant: &SyntaxNode) -> Doc {
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
    pub(super) fn port(&mut self, port: &SyntaxNode) -> Doc {
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
        // `.name(expr)`, a name given to what it stands for, lines up as a
        // name would, its expression against it as a connection's is.
        if let [NodeOrToken::Token(dot), NodeOrToken::Token(name), list @ ..] = &children[names..]
            && dot.kind() == DOT
            && !typed
        {
            let named = Doc::concat([self.token(dot), self.token(name)]);
            let named = match list {
                [] => named,
                [NodeOrToken::Node(list)] if is_arg_list(list, ARG) => self.arguments(named, list),
                _ => return self.verbatim(port),
            };
            let space = || match qualifiers {
                0 => Doc::nil(),
                _ => Doc::Space,
            };
            let qualifiers = self.spaced(&children[..qualifiers]);
            return Doc::concat([
                qualifiers,
                Doc::Cell(0),
                space(),
                Doc::Cell(1),
                space(),
                named,
            ]);
        }
        let [NodeOrToken::Node(name)] = &children[names..] else {
            return self.verbatim(port);
        };
        // A non-ANSI port may join several signals, `{a, b}`.
        let joined = name.kind() == CONCAT_EXPR && names == 0;
        if name.kind() != DECLARATOR && !joined {
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
        docs.push(match joined {
            true => self.node(name),
            false => self.with_comments(name, |this, it| this.declarator(it, Some(2))),
        });
        Doc::concat(docs)
    }

    /// A type's name and what qualifies it: a space between words and before
    /// the packed dimensions, and none around `::` or `.`, after `#`, or
    /// between dimensions.
    pub(super) fn type_ref(&mut self, type_ref: &SyntaxNode) -> Doc {
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
    /// whose `=` ends the cell `eq` of its table, if it is given one.
    pub(super) fn declarator(&mut self, declarator: &SyntaxNode, eq: Option<usize>) -> Doc {
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
            docs.extend([
                eq.map_or_else(Doc::nil, Doc::Cell),
                Doc::Space,
                self.spaced(value),
            ]);
        }
        Doc::concat(docs)
    }

    /// `[`, a size, a range or a type, and `]`, with no space inside.
    pub(super) fn dimension(&mut self, dimension: &SyntaxNode) -> Doc {
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
        self.tightly(|this| Doc::concat(children.iter().map(|it| this.element(it))))
    }
}

/// Where the names of a declaration whose type would be its child `at`
/// start, and whether they can line up after the type: unless it is a block,
/// such as a `struct`, which has nothing on its last line to line them up
/// with. `None` unless the names are declarators between commas, with or
/// without a `;` after them.
fn declarators(children: &[SyntaxElement], at: usize) -> Option<(usize, bool)> {
    let typed = children
        .get(at)
        .is_some_and(|it| matches!(it.kind(), TYPE_REF | ENUM_TYPE | STRUCT_TYPE | UNION_TYPE));
    let aligned = !typed || children[at].kind() == TYPE_REF;
    let names = at + usize::from(typed);
    let rest = match children.last() {
        Some(it) if it.kind() == SEMICOLON => &children[names..children.len() - 1],
        _ => &children[names..],
    };
    let plain = rest.len() % 2 == 1
        && rest.iter().enumerate().all(|(index, it)| match index % 2 {
            0 => it.kind() == DECLARATOR,
            _ => it.kind() == COMMA,
        });
    plain.then_some((names, aligned))
}
