//! Expressions.

use astli_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken};
use rowan::NodeOrToken;

use super::{Writer, significant_children};
use crate::doc::Doc;

impl Writer<'_> {
    /// `min:typ:max`, written tight as a range in a dimension is.
    pub(super) fn min_typ_max(&mut self, expr: &SyntaxNode) -> Doc {
        let tight = std::mem::replace(&mut self.tight, true);
        let children = significant_children(expr);
        let doc = Doc::concat(children.iter().map(|it| self.element(it)));
        self.tight = tight;
        doc
    }

    /// `{` and elements between commas, then `}`, packed as
    /// [`Writer::bracketed`] says.
    pub(super) fn concat_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        let plain = match &children[..] {
            [open, inner @ .., close] => {
                open.kind() == L_BRACE
                    && close.kind() == R_BRACE
                    && inner.iter().enumerate().all(|(at, it)| match at % 2 {
                        0 => it.as_node().is_some(),
                        _ => it.kind() == COMMA,
                    })
            }
            _ => false,
        };
        if !plain {
            return self.verbatim(expr);
        }
        let open = self.element(&children[0]);
        let close = self.element(&children[children.len() - 1]);
        self.bracketed(
            Doc::nil(),
            open,
            &children[1..children.len() - 1],
            close,
            true,
        )
    }

    /// `{`, a count and the concatenation it repeats, then `}`, all against
    /// each other; in an assignment pattern, the `'{` and `}` are the
    /// pattern's. The count is written as inside `[…]`, since `W - 1{a}`
    /// reads as if `1` were the count.
    pub(super) fn replication_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        let (open, count, concat, close) = match &children[..] {
            [
                open,
                NodeOrToken::Node(count),
                NodeOrToken::Node(concat),
                close,
            ] if open.kind() == L_BRACE && close.kind() == R_BRACE => {
                (Some(open), count, concat, Some(close))
            }
            [NodeOrToken::Node(count), NodeOrToken::Node(concat)] => (None, count, concat, None),
            _ => return self.verbatim(expr),
        };
        if concat.kind() != CONCAT_EXPR {
            return self.verbatim(expr);
        }
        let open = open.map_or_else(Doc::nil, |it| self.element(it));
        let tight = std::mem::replace(&mut self.tight, true);
        let count = self.node(count);
        self.tight = tight;
        let concat = self.node(concat);
        let close = close.map_or_else(Doc::nil, |it| self.element(it));
        Doc::concat([open, count, concat, close])
    }

    /// An optional type, then `'{`, items between commas and `}`, laid out as
    /// [`Writer::bracketed`] says. Broken, it ends its line with `'{` and puts
    /// one item per line, a continuation in, as a struct's body is laid out:
    /// its items are usually a struct's fields. The corpus breaks patterns
    /// one per line 975 times to 122 packed, and after `'{` 761 to 336. The
    /// items are a table, so that their values line up as a struct's members
    /// do.
    pub(super) fn assignment_pattern(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        let open = usize::from(children.first().is_some_and(|it| it.as_node().is_some()));
        let plain = match &children[open..] {
            [brace, inner @ .., close] => {
                brace.kind() == APOSTROPHE_L_BRACE
                    && close.kind() == R_BRACE
                    && inner.iter().enumerate().all(|(at, it)| match at % 2 {
                        0 => it.kind() == PATTERN_ITEM,
                        _ => it.kind() == COMMA,
                    })
            }
            _ => false,
        };
        if !plain {
            return self.verbatim(expr);
        }
        let before = match open {
            1 => self.element(&children[0]),
            _ => Doc::nil(),
        };
        let brace = self.element(&children[open]);
        let close = self.element(&children[children.len() - 1]);
        Doc::table(self.bracketed(
            before,
            brace,
            &children[open + 1..children.len() - 1],
            close,
            false,
        ))
    }

    /// A value, or a key, `:` and a value, with no space before the `:` and
    /// the value lined up with the others of its pattern.
    pub(super) fn pattern_item(&mut self, item: &SyntaxNode) -> Doc {
        let children = significant_children(item);
        match &children[..] {
            [NodeOrToken::Node(value)] => self.node(value),
            [NodeOrToken::Node(key), colon, NodeOrToken::Node(value)] if colon.kind() == COLON => {
                Doc::concat([
                    self.node(key),
                    self.element(colon),
                    Doc::Cell(0),
                    Doc::Space,
                    self.node(value),
                ])
            }
            _ => self.verbatim(item),
        }
    }

    /// A base, `.` or `::`, and a member, with no space between, since they
    /// name one thing; nothing breaks there.
    pub(super) fn member(&mut self, expr: &SyntaxNode) -> Doc {
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
    pub(super) fn index_expr(&mut self, expr: &SyntaxNode) -> Doc {
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
    pub(super) fn adjacent(&mut self, node: &SyntaxNode) -> Doc {
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

    /// A condition, `?`, a value and `:`, then what is chosen otherwise; a
    /// ternary there continues the chain, a priority mux. Broken, each
    /// condition's arm ends its line after its `:`, the arms and the final
    /// value aligned under the first. A chain the input broke stays broken,
    /// since that layout is what lets it go without parentheses.
    pub(super) fn ternary_expr(&mut self, expr: &SyntaxNode) -> Doc {
        // The arms' conditions, `?`, values and `:`, and the final value.
        let mut arms = Vec::new();
        let mut last = expr.clone();
        loop {
            let children = significant_children(&last);
            let [
                NodeOrToken::Node(condition),
                NodeOrToken::Token(question),
                NodeOrToken::Node(value),
                NodeOrToken::Token(colon),
                NodeOrToken::Node(otherwise),
            ] = &children[..]
            else {
                break;
            };
            if question.kind() != QUESTION || colon.kind() != COLON {
                break;
            }
            arms.push((
                condition.clone(),
                question.clone(),
                value.clone(),
                colon.clone(),
            ));
            last = otherwise.clone();
            if last.kind() != TERNARY_EXPR {
                break;
            }
        }
        // A ternary of another shape ends the chain as its final value.
        if arms.is_empty() {
            return self.verbatim(expr);
        }
        let broken = arms.len() > 1
            && arms.iter().any(|(_, _, _, colon)| {
                colon
                    .next_token()
                    .is_some_and(|it| it.kind().is_trivia() && it.text().contains('\n'))
            });
        let separator = match (self.tight, broken) {
            (true, _) => Doc::Space,
            (false, true) => Doc::HardLine,
            (false, false) => Doc::Line,
        };

        let mut docs = Vec::new();
        for (at, (condition, question, value, colon)) in arms.iter().enumerate() {
            // A nested arm's own comments are the chain's.
            if at > 0 {
                let arm = condition.parent().expect("a ternary");
                docs.push(self.comments.leading(&arm));
            }
            docs.extend([
                self.node(condition),
                Doc::Space,
                self.token(question),
                Doc::Space,
                self.node(value),
                Doc::Space,
                self.token(colon),
                separator.clone(),
            ]);
        }
        docs.push(self.node(&last));
        for (condition, ..) in arms.iter().skip(1).rev() {
            let arm = condition.parent().expect("a ternary");
            docs.push(self.comments.trailing(&arm));
        }
        match self.tight {
            true => Doc::concat(docs),
            false => Doc::group(Doc::align(Doc::concat(docs))),
        }
    }

    /// A type or a size, `'`, and the expression in parentheses it casts, all
    /// against each other.
    pub(super) fn cast_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        match &children[..] {
            [NodeOrToken::Node(_), tick, NodeOrToken::Node(operand)]
                if tick.kind() == APOSTROPHE && operand.kind() == PAREN_EXPR =>
            {
                Doc::concat(children.iter().map(|it| self.element(it)))
            }
            _ => self.verbatim(expr),
        }
    }

    /// An operator against its operand, before or after it. One before keeps
    /// a space from an operand that starts with an operator too: `- -a` and
    /// `& &a` would run together into `--a` and `&&a`.
    pub(super) fn unary_expr(&mut self, expr: &SyntaxNode) -> Doc {
        match &significant_children(expr)[..] {
            [NodeOrToken::Node(operand), NodeOrToken::Token(op)] => {
                Doc::concat([self.node(operand), self.token(op)])
            }
            [NodeOrToken::Token(op), NodeOrToken::Node(operand)] => {
                let space = match operand.kind() {
                    UNARY_EXPR => Doc::Space,
                    _ => Doc::nil(),
                };
                Doc::concat([self.token(op), space, self.node(operand)])
            }
            _ => self.verbatim(expr),
        }
    }

    /// Operands and the operators between them, a space on either side of
    /// each. A chain of one operator is one group, which breaks after every
    /// operator or none, its operands aligned under the first; an operand
    /// with another operator is a group of its own. Inside `[…]`, nothing
    /// breaks and no space is written unless the tokens would run together.
    pub(super) fn bin_expr(&mut self, expr: &SyntaxNode) -> Doc {
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
    ///
    /// A loop down the left operands, not recursion: `a | b | c` is
    /// `(a | b) | c`, a level per operator, and generated code chains
    /// hundreds of operands.
    fn chain(&mut self, expr: &SyntaxNode, op: SyntaxKind, docs: &mut Vec<Doc>) {
        let mut links = vec![expr.clone()];
        loop {
            let (lhs, _, _) = link(&links[links.len() - 1]);
            if lhs.kind() == BIN_EXPR && operator(&lhs) == Some(op) {
                docs.push(self.comments.leading(&lhs));
                links.push(lhs);
            } else {
                docs.push(self.node(&lhs));
                break;
            }
        }
        for (at, expr) in links.iter().enumerate().rev() {
            let (_, token, rhs) = link(expr);
            let spaced = !self.tight || rhs.kind() == UNARY_EXPR;
            let (before, after) = match (spaced, self.tight) {
                (false, _) => (Doc::nil(), Doc::nil()),
                (true, true) => (Doc::Space, Doc::Space),
                (true, false) => (Doc::Space, Doc::Line),
            };
            docs.extend([before, self.token(&token), after]);
            self.operand(&rhs, op, docs);
            if at > 0 {
                docs.push(self.comments.trailing(expr));
            }
        }
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

    /// An expression in parentheses, with no space inside them, or values
    /// between commas, as a delay's are. An event list with an edge falls
    /// back.
    pub(super) fn paren_expr(&mut self, expr: &SyntaxNode) -> Doc {
        let children = significant_children(expr);
        match &children[..] {
            [open, NodeOrToken::Node(inner), close]
                if open.kind() == L_PAREN
                    && close.kind() == R_PAREN
                    && inner.kind() != VERBATIM =>
            {
                Doc::concat(children.iter().map(|it| self.element(it)))
            }
            [open, close] if open.kind() == L_PAREN && close.kind() == R_PAREN => {
                Doc::concat(children.iter().map(|it| self.element(it)))
            }
            // Delays, or the signals an event list names.
            [open, inner @ .., close]
                if open.kind() == L_PAREN
                    && close.kind() == R_PAREN
                    && inner.iter().enumerate().all(|(at, it)| match at % 2 {
                        0 => it.as_node().is_some_and(|it| it.kind() != VERBATIM),
                        _ => it.kind() == COMMA,
                    }) =>
            {
                let open = self.element(open);
                let close = self.element(close);
                self.bracketed(Doc::nil(), open, inner, close, true)
            }
            _ => self.verbatim(expr),
        }
    }
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

/// The operands of a binary expression and the operator between them.
fn link(expr: &SyntaxNode) -> (SyntaxNode, SyntaxToken, SyntaxNode) {
    match &significant_children(expr)[..] {
        [
            NodeOrToken::Node(lhs),
            NodeOrToken::Token(token),
            NodeOrToken::Node(rhs),
        ] => (lhs.clone(), token.clone(), rhs.clone()),
        _ => unreachable!("`operator` checked the shape"),
    }
}
