//! `begin`-`end` blocks outside procedural code: generate blocks, and those
//! with nothing to generate.

use astli_syntax::SyntaxKind::{
    BEGIN_KW, CASE_ITEM, CLASS_DECL, CLOCKING_DECL, COLON, CONCURRENT_ASSERTION,
    CONDITIONAL_BRANCH, CONDITIONAL_REGION, CONSTRAINT_DECL, COVERGROUP_DECL, FOR_STMT,
    FUNCTION_DECL, GENERATE_REGION, IDENT, IF_STMT, IMMEDIATE_ASSERTION, INTERFACE_DECL,
    LABELED_STMT, MODULE_DECL, PACKAGE_DECL, PROCEDURAL_BLOCK, PROGRAM_DECL, PROPERTY_DECL,
    SEQUENCE_DECL, TASK_DECL,
};
use astli_syntax::ast::{AstNode, Block, LabeledStmt};
use astli_syntax::{SyntaxNode, SyntaxToken};

use crate::rule::Cx;

/// A generate block's label names its scope in the hierarchy; without one,
/// a tool makes up `genblk1`, which moves when a block is added above it.
pub(crate) fn generate_label(cx: &mut Cx) {
    for (block, begin) in generate_blocks(cx.root()) {
        if label(&block).is_none() {
            let diagnostic = cx
                .diagnostic(begin.text_range(), "a generate block without a label")
                .pointing("name it: `begin : gen_name`");
            cx.report(diagnostic);
        }
    }
}

/// A generate block's label starts with `gen_` or `g_`, so that a path in
/// the hierarchy shows which scopes are generated.
pub(crate) fn generate_label_prefix(cx: &mut Cx) {
    for (block, _) in generate_blocks(cx.root()) {
        let Some(label) = label(&block) else {
            continue;
        };
        let text = label.text();
        if !text.starts_with("gen_") && !text.starts_with("g_") {
            let message = format!("generate block `{text}` does not start with `gen_` or `g_`");
            cx.report(cx.diagnostic(label.text_range(), message));
        }
    }
}

/// A `begin` directly inside `generate` ... `endgenerate` generates nothing:
/// it is Verilog-2001's way of grouping, and makes a scope nobody named.
pub(crate) fn v2001_generate_begin(cx: &mut Cx) {
    for block in cx.root().descendants().filter_map(Block::cast) {
        if parent(block.syntax()).is_some_and(|it| it.kind() == GENERATE_REGION)
            && let Some(begin) = begin(&block)
        {
            let diagnostic = cx
                .diagnostic(
                    begin.text_range(),
                    "a `begin` block directly inside `generate`",
                )
                .pointing("drop the `begin` and `end`");
            cx.report(diagnostic);
        }
    }
}

/// A `begin` directly in a module groups nothing a module needs grouped, and
/// is a generate block with no loop or condition.
pub(crate) fn module_begin_block(cx: &mut Cx) {
    for block in cx.root().descendants().filter_map(Block::cast) {
        if parent(block.syntax()).is_some_and(|it| it.kind() == MODULE_DECL)
            && let Some(begin) = begin(&block)
        {
            let diagnostic = cx
                .diagnostic(begin.text_range(), "a `begin` block directly in a module")
                .pointing("drop the `begin` and `end`");
            cx.report(diagnostic);
        }
    }
}

/// The body of each generate `if`, `for` or `case` item under `root`, with
/// its `begin`.
fn generate_blocks(root: &SyntaxNode) -> impl Iterator<Item = (Block, SyntaxToken)> {
    root.descendants()
        .filter_map(Block::cast)
        .filter_map(|block| {
            let owner = parent(block.syntax())?;
            let construct = matches!(owner.kind(), IF_STMT | FOR_STMT | CASE_ITEM);
            (construct && generated(block.syntax())).then_some(())?;
            let begin = begin(&block)?;
            Some((block, begin))
        })
}

/// Whether `node` stands in a design element's generate scope rather than
/// in procedural code, a class or a package.
fn generated(node: &SyntaxNode) -> bool {
    scope(node).is_some_and(|it| matches!(it.kind(), MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL))
}

/// The construct whose scope `node` stands in: a design element, past any
/// generate construct, or the procedure, subroutine, class, package or
/// assertion nearer it.
pub(crate) fn scope(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.ancestors().skip(1).find(|it| {
        matches!(
            it.kind(),
            MODULE_DECL
                | INTERFACE_DECL
                | PROGRAM_DECL
                | PROCEDURAL_BLOCK
                | FUNCTION_DECL
                | TASK_DECL
                | CLASS_DECL
                | PACKAGE_DECL
                | CONCURRENT_ASSERTION
                | IMMEDIATE_ASSERTION
                | PROPERTY_DECL
                | SEQUENCE_DECL
                | COVERGROUP_DECL
                | CONSTRAINT_DECL
                | CLOCKING_DECL
        )
    })
}

/// The node `node` stands in, past any `` `ifdef `` around it and a label
/// before it.
fn parent(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.ancestors().skip(1).find(|it| {
        !matches!(
            it.kind(),
            CONDITIONAL_BRANCH | CONDITIONAL_REGION | LABELED_STMT
        )
    })
}

fn begin(block: &Block) -> Option<SyntaxToken> {
    block.open().filter(|it| it.kind() == BEGIN_KW)
}

/// A block's label, after its `begin` or before it: `begin : gen_a` or
/// `gen_a : begin`.
fn label(block: &Block) -> Option<SyntaxToken> {
    let mut tokens = (block.syntax().children_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .skip_while(|token| token.kind() != BEGIN_KW)
        .skip(1);
    let after = match tokens.next() {
        Some(colon) if colon.kind() == COLON => tokens.next().filter(|it| it.kind() == IDENT),
        _ => None,
    };
    let before = (block.syntax().parent())
        .and_then(LabeledStmt::cast)
        .and_then(|it| it.label());
    after.or(before)
}
