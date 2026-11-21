//! A file's and a scope's items, the shell a header, items and an end make,
//! and the bodies of `specify` and of a UDP's table.

use astli_syntax::{
    SyntaxElement, SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken, first_significant,
    significant_child_tokens,
};
use rowan::NodeOrToken;

use super::list::is_arg_list;
use super::stmt::is_empty_stmt;
use super::{Writer, separation, significant_children};
use crate::doc::Doc;

impl Writer<'_> {
    /// Items one after another, each on lines of its own.
    pub(super) fn source_file(&mut self, file: &SyntaxNode) -> Doc {
        let items = self.items(&significant_children(file));
        Doc::concat([self.comments.head(), items])
    }

    /// A module, interface, program, package, class, function or task: its
    /// header on one line, its items indented below, and its end on a line of
    /// its own. A prototype is its header alone.
    pub(super) fn scope(&mut self, scope: &SyntaxNode) -> Doc {
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
                    ATTRIBUTES
                        | IMPORT_DECL
                        | PARAM_PORT_LIST
                        | PORT_LIST
                        | TYPE_REF
                        | ARG_LIST
                        | EVENT_CONTROL
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

    /// `specify` or `table`, what it holds indented below, and `endspecify`
    /// or `endtable`.
    pub(super) fn keyword_block(&mut self, block: &SyntaxNode) -> Doc {
        let children = significant_children(block);
        match &children[..] {
            [open, items @ .., close]
                if open.as_token().is_some()
                    && close.as_token().is_some()
                    && items.iter().all(|it| it.as_node().is_some()) =>
            {
                self.shell(&children[..1], items, &children[children.len() - 1..])
            }
            _ => self.verbatim(block),
        }
    }

    /// A path on one line, its polarity against the arrow, as in `(a +=> y)`
    /// and `(a -*> y)`. A `+` apart from `=>` or `:` stays apart, since
    /// together they are read as `+=` and `>`, or as `+:`.
    pub(super) fn specify_path(&mut self, path: &SyntaxNode) -> Doc {
        let mut docs = Vec::new();
        let mut prev: Option<SyntaxElement> = None;
        for element in significant_children(path) {
            let polarity = prev.as_ref().is_some_and(|prev| {
                matches!(
                    (prev.kind(), element.kind()),
                    (PLUS_EQ | MINUS_EQ, GT) | (PLUS | MINUS, STAR_GT)
                )
            });
            if !polarity {
                docs.push(separation(prev.as_ref(), &element));
            }
            docs.push(self.element(&element));
            prev = Some(element);
        }
        Doc::concat(docs)
    }

    /// A row of a UDP's table, its symbols apart, each in a column of its
    /// own: the inputs, then after colons the current state and the output.
    pub(super) fn udp_entry(&mut self, entry: &SyntaxNode) -> Doc {
        let mut docs = Vec::new();
        for (column, element) in significant_children(entry).iter().enumerate() {
            if element.kind() == SEMICOLON {
                docs.push(self.element(element));
                continue;
            }
            if column > 0 {
                docs.push(Doc::Space);
            }
            docs.extend([self.element(element), Doc::Cell(column)]);
        }
        Doc::concat(docs)
    }

    /// `elements` with nothing between them, as an edge in a table, `(01)`.
    pub(super) fn tight(&mut self, elements: &[SyntaxElement]) -> Doc {
        Doc::concat(elements.iter().map(|element| self.element(element)))
    }

    /// A timing check, laid out as a call is, and its `;`.
    pub(super) fn timing_check(&mut self, check: &SyntaxNode) -> Doc {
        match &significant_children(check)[..] {
            [
                NodeOrToken::Token(name),
                NodeOrToken::Node(list),
                NodeOrToken::Token(semicolon),
            ] if is_arg_list(list, ARG) => {
                let name = self.token(name);
                let call = self.arguments(name, list);
                Doc::concat([call, self.token(semicolon)])
            }
            _ => self.verbatim(check),
        }
    }

    /// A header on one line, items indented below it, and an end on a line
    /// of its own. Neither `header` nor `end` is empty.
    pub(super) fn shell(
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

    /// Each of `elements` on lines of its own, and each run of consecutive
    /// declarations of one kind, of `assign`s, or of assignments with one
    /// operator a table. So is each run of other items that take a line or
    /// so, such as calls, whose only column is the comment after them; an item
    /// with a body ends it. A comma stays on the line of the
    /// item before it, and in its table; any other token would be stray.
    pub(super) fn items(&mut self, elements: &[SyntaxElement]) -> Doc {
        let mut docs = Vec::new();
        // What the run is made of, and what it wrote.
        let mut run: Option<(Run, Vec<Doc>)> = None;
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
                // The `;` after a property is written after its last line.
                NodeOrToken::Token(comma) if matches!(comma.kind(), COMMA | SEMICOLON) => {
                    self.token(comma)
                }
                NodeOrToken::Token(token) => Doc::concat([Doc::HardLine, self.token(token)]),
            };
            // An item written as it was read has no cells to line up.
            let of = match element {
                NodeOrToken::Node(item) if skipped(item) => None,
                _ => run_of(element),
            };
            let joins = run.as_ref().is_some_and(|(run, _)| {
                of == Some(*run) || matches!(element.kind(), COMMA | SEMICOLON)
            });
            if !joins {
                if let Some((_, rows)) = run.take() {
                    docs.push(Doc::table(Doc::concat(rows)));
                }
                run = of.map(|of| (of, Vec::new()));
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

    /// An item on lines of its own, after an empty line if it had one. An
    /// attribute on it can ask for it to be written as it was read.
    fn item(&mut self, item: &SyntaxNode) -> Doc {
        let blank_line =
            first_significant(item).map_or_else(Doc::nil, |token| blank_line_before(&token));
        let layout = if skipped(item) {
            self.as_written(item).unwrap_or_else(Doc::nil)
        } else {
            self.layout(item)
        };
        Doc::concat([
            Doc::HardLine,
            self.comments.leading(item),
            blank_line,
            layout,
            self.comments.trailing(item),
        ])
    }
}

/// What a run of items that line up is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Run {
    /// Declarations of one kind, or `assign`s, in columns.
    Of(SyntaxKind),
    /// Assignment statements with one operator, lined up on it.
    Assign(SyntaxKind),
    /// Items of a line or so each, with a column for comments alone.
    Lines,
}

/// The run `element` can be a row of, if any.
fn run_of(element: &SyntaxElement) -> Option<Run> {
    let NodeOrToken::Node(item) = element else {
        return None;
    };
    match item.kind() {
        kind @ (VAR_DECL | PARAM_DECL | PORT | STRUCT_MEMBER | ENUM_VARIANT | CONTINUOUS_ASSIGN
        | PROCEDURAL_ASSIGN | BINS | DEFPARAM | UDP_ENTRY) => Some(Run::Of(kind)),
        EXPR_STMT => match item.first_child().filter(|it| it.kind() == ASSIGNMENT) {
            Some(assignment) => {
                let op = significant_children(&assignment).get(1).map(|it| it.kind());
                Some(op.map_or(Run::Lines, Run::Assign))
            }
            None => Some(Run::Lines),
        },
        RETURN_STMT | DISABLE_STMT | IMPORT_DECL | MACRO_CALL | CONSTRAINT_EXPR | SOLVE_BEFORE
        | SPECIFY_PATH | TIMING_CHECK | PULSE_DECL => Some(Run::Lines),
        TYPEDEF
            if !item
                .children()
                .any(|it| matches!(it.kind(), ENUM_TYPE | STRUCT_TYPE | UNION_TYPE)) =>
        {
            Some(Run::Lines)
        }
        _ => None,
    }
}

/// An empty line if the input has one right before `token`.
///
/// Comments before it carry their own separation, so only the whitespace
/// between the last of them and the token is read.
pub(super) fn blank_line_before(token: &SyntaxToken) -> Doc {
    match token.prev_token() {
        Some(space) if space.kind() == WHITESPACE && space.text().matches('\n').count() > 1 => {
            Doc::BlankLine
        }
        _ => Doc::nil(),
    }
}

/// The attribute that asks for an item to be written as it was read.
const SKIP: &str = "astli_fmt_skip";

/// Whether `item` carries `(* astli_fmt_skip *)`. An attribute, rather than a
/// comment, is part of the item, so it cannot come loose from it when lines
/// move.
fn skipped(item: &SyntaxNode) -> bool {
    let specs = (item.children().filter(|it| it.kind() == ATTRIBUTES)).flat_map(|attributes| {
        attributes
            .children()
            .filter(|it| it.kind() == ATTRIBUTE_SPEC)
    });
    specs.into_iter().any(|spec| {
        significant_child_tokens(&spec)
            .next()
            .is_some_and(|name| name.text() == SKIP)
    })
}
