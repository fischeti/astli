//! The rules, one per construct, and the fallback that writes a node no rule
//! lays out as it was read.
//!
//! A rule writes every token of its node, and takes every comment the comment
//! map gives to the node, its descendants or its tokens.
//!
//! `layout` picks the rule for a node; the rules are in a module per area.

use astli_syntax::{
    SyntaxElement, SyntaxKind::*, SyntaxNode, SyntaxToken, first_significant, last_significant,
};
use rowan::NodeOrToken;

use crate::comments::{Comments, line_directive};
use crate::doc::Doc;
use crate::verbatim::verbatim;

mod decl;
mod expr;
mod header;
mod items;
mod list;
mod preproc;
mod stmt;

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
    /// no space; nothing breaks there either, which [`Writer::tightly`]
    /// leaves to the printer.
    tight: bool,
}

impl Writer<'_> {
    /// What `write` writes, as inside `[…]`: see [`Writer::tight`].
    fn tightly(&mut self, write: impl FnOnce(&mut Self) -> Doc) -> Doc {
        let tight = std::mem::replace(&mut self.tight, true);
        let doc = write(self);
        self.tight = tight;
        Doc::flat(doc)
    }

    /// `node` with the comments around it.
    fn node(&mut self, node: &SyntaxNode) -> Doc {
        self.with_comments(node, Self::layout)
    }

    /// `node` laid out by `rule`, with the comments around it: for a rule
    /// that `layout` cannot pick, since it takes more than the node.
    fn with_comments(
        &mut self,
        node: &SyntaxNode,
        rule: impl FnOnce(&mut Self, &SyntaxNode) -> Doc,
    ) -> Doc {
        Doc::concat([
            self.comments.leading(node),
            rule(self, node),
            self.comments.trailing(node),
        ])
    }

    /// `node` without the comments around it.
    fn layout(&mut self, node: &SyntaxNode) -> Doc {
        match node.kind() {
            // An attribute on a design unit goes on a line of its own, as the
            // corpus has every one, but one on a prototype stays on its line.
            MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL | UDP_DECL => self
                .scope(node, |this, header| {
                    this.unit_header(header, &Doc::HardLine)
                }),
            EXTERN_DECL => self.scope(node, |this, header| this.unit_header(header, &Doc::Space)),
            CLASS_DECL => self.scope(node, Self::class_header),
            FUNCTION_DECL | TASK_DECL | CHECKER_DECL => self.scope(node, Self::subroutine_header),
            COVERGROUP_DECL => self.scope(node, Self::covergroup_header),
            CLOCKING_DECL | CONFIG_DECL => self.scope(node, Self::clocking_header),
            CLOCKING_ITEM | CONFIG_RULE => self.spaced(&significant_children(node)),
            MODPORT_METHOD => self.modport_method(node),
            SPECIFY_BLOCK | UDP_TABLE => self.keyword_block(node),
            UDP_ENTRY => self.udp_entry(node),
            UDP_EDGE => self.unspaced(&significant_children(node)),
            SPECIFY_PATH => self.specify_path(node),
            TIMING_CHECK => self.timing_check(node),
            PULSE_DECL => self.spaced(&significant_children(node)),
            CONDITIONAL_REGION => self.conditional_region(node),
            CONTINUOUS_ASSIGN => self.continuous_assign(node),
            PROCEDURAL_BLOCK => self.procedural_block(node),
            BLOCK => self.block(node),
            TIMING_STMT => self.timing_stmt(node),
            EVENT_CONTROL | DELAY_CONTROL => self.control(node),
            EXPR_STMT => self.expr_stmt(node),
            PROCEDURAL_ASSIGN => self.aligning(&significant_children(node)),
            IF_STMT => self.if_stmt(node),
            IMMEDIATE_ASSERTION | WAIT_ORDER_STMT => self.immediate_assertion(node),
            CONCURRENT_ASSERTION => self.concurrent_assertion(node),
            PROPERTY_DECL | SEQUENCE_DECL => self.assertion_decl(node),
            DEFAULT_DISABLE | TIMEUNIT_DECL | NET_ALIAS | TAGGED_EXPR | COPY_EXPR | STRENGTH => {
                self.spaced(&significant_children(node))
            }
            RETURN_STMT | DISABLE_STMT => self.keyword_stmt(node),
            WAIT_STMT => self.wait_stmt(node),
            DO_WHILE_STMT => self.do_while_stmt(node),
            LABELED_STMT => self.labeled_stmt(node),
            IMPORT_DECL => self.import_decl(node),
            MODPORT_DECL => self.modport_decl(node),
            CONSTRAINT_DECL => self.constraint_decl(node),
            CONSTRAINT_BLOCK | BINS_BLOCK => self.braces(node),
            COVERPOINT | CROSS => self.cover_point(node),
            BINS => self.bins(node),
            CONSTRAINT_EXPR | SOLVE_BEFORE => self.spaced(&significant_children(node)),
            IMPLICATION => self.implication(node),
            MODPORT => self.modport(node),
            FOR_STMT | FOREACH_STMT | WHILE_STMT | REPEAT_STMT | FOREVER_STMT => {
                self.loop_stmt(node)
            }
            FOREACH_HEADER => self.foreach_header(node),
            CASE_STMT => self.case_stmt(node),
            CASE_ITEM => self.case_item(node),
            ASSIGNMENT => self.assignment(node, None),
            VAR_DECL | STRUCT_MEMBER => self.var_decl(node),
            TYPEDEF => self.typedef(node),
            ENUM_TYPE | STRUCT_TYPE | UNION_TYPE => self.body_type(node),
            ENUM_VARIANT => self.enum_variant(node),
            TYPE_REF => self.type_ref(node),
            PARAM_DECL => self.param_decl(node),
            DECLARATOR => self.declarator(node, None),
            PORT => self.port(node),
            DIMENSION => self.dimension(node),
            MIN_TYP_MAX_EXPR => self.min_typ_max(node),
            BIN_EXPR => self.bin_expr(node),
            UNARY_EXPR | POSTFIX_EXPR => self.unary_expr(node),
            TERNARY_EXPR => self.ternary_expr(node),
            CAST_EXPR => self.cast_expr(node),
            CONCAT_EXPR => self.concat_expr(node),
            REPLICATION_EXPR => self.replication_expr(node),
            ASSIGNMENT_PATTERN => self.assignment_pattern(node),
            PATTERN_ITEM => self.pattern_item(node),
            CALL_EXPR => self.call_expr(node),
            MACRO_CALL => self.macro_call(node),
            FIELD_EXPR | SCOPE_EXPR => self.member(node),
            INDEX_EXPR => self.index_expr(node),
            NAME_REF | LITERAL_EXPR => self.adjacent(node),
            BIND_PATTERN => Doc::concat(
                significant_children(node)
                    .iter()
                    .map(|it| self.element(it))
                    .collect::<Vec<_>>(),
            ),
            INSTANTIATION | GATE_INSTANTIATION => self.instantiation(node),
            DEFPARAM => self.continuous_assign(node),
            BIND_DIRECTIVE => self.bind_directive(node),
            INSTANCE => self.instance(node),
            // The parameters a configuration overrides stay on its line if
            // they fit.
            ARG_LIST
                if node
                    .parent()
                    .is_some_and(|parent| parent.kind() == CONFIG_RULE) =>
            {
                self.list(node, false)
            }
            // Named connections go one per line, so that they can be aligned.
            ARG_LIST
                if node
                    .parent()
                    .is_some_and(|parent| matches!(parent.kind(), INSTANCE | INSTANTIATION)) =>
            {
                let named = node
                    .children()
                    .any(|arg| first_significant(&arg).is_some_and(|token| token.kind() == DOT));
                // The guide requires connections aligned, however far one
                // sticks out.
                Doc::strict_table(self.list(node, named))
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

    /// `node` as it was read, because no rule lays it out.
    fn verbatim(&mut self, node: &SyntaxNode) -> Doc {
        self.as_written(node).map_or_else(Doc::nil, |doc| {
            self.unformatted.push(node.clone());
            doc
        })
    }

    /// `node` as it was read, or `None` if it has no significant token.
    fn as_written(&mut self, node: &SyntaxNode) -> Option<Doc> {
        let until = (last_significant(node).and_then(|token| line_directive(&token)))
            .map_or(0, |directive| directive.text_range().end().into());
        let (verbatim, covers) = verbatim(node, self.source, until)?;
        self.comments.within(covers);
        Some(Doc::Verbatim(verbatim))
    }
}

/// A space between two elements on a line, but none before `,`, `;` or `)`,
/// none after `#` or `(`, and none around `::` or `.`. What comes before the
/// first is its parent's to separate.
fn separation(prev: Option<&SyntaxElement>, next: &SyntaxElement) -> Doc {
    let Some(prev) = prev else {
        return Doc::nil();
    };
    let tight = matches!(prev.kind(), HASH | L_PAREN | COLON_COLON | DOT)
        || matches!(next.kind(), COMMA | SEMICOLON | R_PAREN | COLON_COLON | DOT);
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
