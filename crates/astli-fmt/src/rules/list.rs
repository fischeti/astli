//! Lists in parentheses: a header's, a call's and an instance's, and the
//! packing of entries between brackets.

use astli_syntax::{SyntaxElement, SyntaxKind, SyntaxKind::*, SyntaxNode};
use rowan::NodeOrToken;

use super::{Writer, significant_children};
use crate::doc::Doc;

impl Writer<'_> {
    /// A list in parentheses, after a `#` if it is of parameters. Broken, it
    /// is laid out as a [`shell`](Self::shell) is, each entry on a line of
    /// its own and its ports and parameters aligned, and the `)` at the start
    /// of a line; otherwise it stays on the line if all of it fits. A list
    /// holding a directive is always broken, since the directive needs lines
    /// of its own.
    pub(super) fn list(&mut self, list: &SyntaxNode, broken: bool) -> Doc {
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
        let preprocessed = entries.iter().any(|it| {
            it.as_node().is_some() && !matches!(it.kind(), PORT | MODPORT_METHOD | PARAM_DECL | ARG)
        });
        let line = match broken || preprocessed {
            true => Doc::HardLine,
            false => Doc::SoftLine,
        };
        Doc::group(self.enclosed(header, entries, close, &line))
    }

    /// `bind`, where it binds, and the instantiation, which starts on the
    /// same line.
    pub(super) fn bind_directive(&mut self, bind: &SyntaxNode) -> Doc {
        let children = significant_children(bind);
        match &children[..] {
            [header @ .., NodeOrToken::Node(instantiation)]
                if instantiation.kind() == INSTANTIATION && header.len() >= 2 =>
            {
                Doc::concat([self.spaced(header), Doc::Space, self.node(instantiation)])
            }
            _ => self.verbatim(bind),
        }
    }

    /// A module, an interface, a UDP, a gate or a switch, its parameters or
    /// its strength and delay, and its instances.
    pub(super) fn instantiation(&mut self, instantiation: &SyntaxNode) -> Doc {
        let children = significant_children(instantiation);
        let mut kinds = children
            .iter()
            .map(|it| it.kind())
            .skip_while(|&it| it == ATTRIBUTES);
        // The order past the type is the parser's to check; the layout is the
        // same in any order.
        let plain = kinds
            .next()
            .is_some_and(|kind| kind == TYPE_REF || kind.is_keyword())
            && children.last().is_some_and(|it| it.kind() == SEMICOLON)
            && kinds.all(|kind| {
                matches!(
                    kind,
                    STRENGTH | DELAY_CONTROL | HASH | ARG_LIST | INSTANCE | COMMA | SEMICOLON
                )
            });
        match plain {
            true => self.spaced(&children),
            false => self.verbatim(instantiation),
        }
    }

    /// An instance's name, its dimensions, and its connections. A gate's or
    /// a UDP's may have no name.
    pub(super) fn instance(&mut self, instance: &SyntaxNode) -> Doc {
        let children = significant_children(instance);
        let plain = match &children[..] {
            [NodeOrToken::Node(connections)] => connections.kind() == ARG_LIST,
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
    /// or a value alone, with no space inside, after any attributes. The `(`
    /// of a named connection lines up with the others in its list.
    pub(super) fn arg(&mut self, arg: &SyntaxNode) -> Doc {
        let children = significant_children(arg);
        if children
            .first()
            .is_some_and(|it| matches!(it.kind(), POSEDGE_KW | NEGEDGE_KW | EDGE_KW))
        {
            return self.event_arg(arg, &children);
        }
        let attributes = children.iter().take_while(|it| it.kind() == ATTRIBUTES);
        let (attributes, children) = children.split_at(attributes.count());
        let plain = match children {
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
        ] = children
            && is_connection(arg)
        {
            docs.insert(2, Doc::Cell(0));
        }
        if !attributes.is_empty() {
            docs.insert(0, Doc::concat([self.spaced(attributes), Doc::Space]));
        }
        Doc::concat(docs)
    }

    /// An event in a timing check: its edge, then its signal. The transitions
    /// in `edge [01, 1x]` are written each as one word, since `0x` is lexed
    /// as two tokens.
    fn event_arg(&mut self, arg: &SyntaxNode, children: &[SyntaxElement]) -> Doc {
        let (edge, rest) = children.split_first().expect("an edge");
        let mut docs = vec![self.element(edge)];
        let mut rest = rest;
        if rest.first().is_some_and(|it| it.kind() == L_BRACK) {
            let Some(end) = rest.iter().position(|it| it.kind() == R_BRACK) else {
                return self.verbatim(arg);
            };
            let (transitions, after) = rest.split_at(end + 1);
            if transitions.iter().any(|it| it.as_node().is_some()) {
                return self.verbatim(arg);
            }
            docs.push(Doc::Space);
            for token in transitions {
                docs.push(self.element(token));
                if token.kind() == COMMA {
                    docs.push(Doc::Space);
                }
            }
            rest = after;
        }
        match rest {
            [NodeOrToken::Node(signal)] => {
                docs.extend([Doc::Space, self.node(signal)]);
                Doc::concat(docs)
            }
            _ => self.verbatim(arg),
        }
    }

    /// A callee and its arguments in parentheses, with no space between, laid
    /// out as [`Writer::arguments`] says. A parameterised callee or a `with`
    /// clause falls back.
    pub(super) fn call_expr(&mut self, expr: &SyntaxNode) -> Doc {
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
    pub(super) fn macro_call(&mut self, call: &SyntaxNode) -> Doc {
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

    /// `callee`, then `list` in parentheses, packed as [`Writer::bracketed`]
    /// says. The `(` stays on the callee's line, where a macro's arguments
    /// must start.
    pub(super) fn arguments(&mut self, callee: Doc, list: &SyntaxNode) -> Doc {
        let entries = significant_children(list);
        let (open, close) = (&entries[0], &entries[entries.len() - 1]);
        let open = Doc::concat([self.comments.leading(list), self.element(open)]);
        let close = Doc::concat([self.element(close), self.comments.trailing(list)]);
        self.bracketed(callee, open, &entries[1..entries.len() - 1], close, true)
    }

    /// `before`, then `entries` between `open` and `close` with no space
    /// inside them. Broken and `packed`, the entries go as many on a line as
    /// fit under the first; if a line would still pass the width, or start
    /// past half of it, they break after `open` instead, go a continuation
    /// in, and `close` goes on a line of its own. Broken and not `packed`,
    /// they always take that second form, one per line. Inside `[…]` nothing
    /// breaks.
    pub(super) fn bracketed(
        &mut self,
        before: Doc,
        open: Doc,
        entries: &[SyntaxElement],
        close: Doc,
        packed: bool,
    ) -> Doc {
        // An entry and the comma after it, then the separator.
        let mut parts = Vec::new();
        for entry in entries {
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
        if parts.is_empty() {
            return Doc::concat([before, open, close]);
        }
        if self.tight {
            let parts = parts.into_iter().map(|part| match part {
                Doc::Line => Doc::Space,
                part => part,
            });
            return Doc::concat([before, open, Doc::concat(parts), close]);
        }
        let entries = match packed {
            true => aligned_or_indented(open, Doc::Fill(parts.into()), close),
            false => indented(open, Doc::Concat(parts), close),
        };
        Doc::group(Doc::concat([before, entries]))
    }
}

/// `inner` between `open` and `close`. Broken, its lines start under its
/// first, unless one would then pass the width or start past half of it;
/// then it is laid out as [`indented`] says.
pub(super) fn aligned_or_indented(open: Doc, inner: Doc, close: Doc) -> Doc {
    let aligned = Doc::concat([open.clone(), Doc::align(inner.clone()), close.clone()]);
    Doc::prefer(aligned, indented(open, inner, close))
}

/// `inner` between `open` and `close`. Broken, it starts on the line after
/// `open`, a continuation in, and `close` goes on a line of its own.
fn indented(open: Doc, inner: Doc, close: Doc) -> Doc {
    Doc::concat([
        open,
        Doc::indent(Doc::indent(Doc::concat([Doc::SoftLine, inner]))),
        Doc::SoftLine,
        close,
    ])
}

/// Whether `list` is `(`, then entries of the kind `arg` between commas, then
/// `)`.
pub(super) fn is_arg_list(list: &SyntaxNode, arg: SyntaxKind) -> bool {
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
