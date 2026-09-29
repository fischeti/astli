//! A `begin` after every statement that takes a body.

use astli_syntax::SyntaxKind::{BEGIN_KW, BLOCK, IF_STMT, MACRO_CALL, VERBATIM};
use astli_syntax::ast::{
    AstNode, ForStmt, ForeachStmt, ForeverStmt, IfStmt, Item, ProceduralBlock, TimingStmt,
    WhileStmt,
};
use astli_syntax::{SyntaxNode, SyntaxToken};

use crate::rule::Cx;

/// A body without `begin` takes one statement, so a second one added below
/// it runs outside the `if` or the loop, however it is indented. `else if`
/// is one construct, and needs no `begin` between its words.
pub(crate) fn explicit_begin(cx: &mut Cx) {
    for node in cx.root().descendants() {
        let bodies: Vec<(Option<SyntaxToken>, Option<Item>)> =
            if let Some(it) = IfStmt::cast(node.clone()) {
                let otherwise = it
                    .else_branch()
                    .filter(|body| body.syntax().kind() != IF_STMT);
                vec![
                    (it.if_token(), it.then_branch()),
                    (it.else_token(), otherwise),
                ]
            } else if let Some(it) = ProceduralBlock::cast(node.clone()) {
                let body = it
                    .body()
                    .map(|body| match TimingStmt::cast(body.syntax().clone()) {
                        Some(timed) => timed.item(),
                        None => Some(body),
                    });
                vec![(it.keyword(), body.flatten())]
            } else if let Some(it) = ForStmt::cast(node.clone()) {
                vec![(it.for_token(), it.body())]
            } else if let Some(it) = ForeachStmt::cast(node.clone()) {
                vec![(it.foreach_token(), it.body())]
            } else if let Some(it) = WhileStmt::cast(node.clone()) {
                vec![(it.while_token(), it.body())]
            } else if let Some(it) = ForeverStmt::cast(node.clone()) {
                vec![(it.forever_token(), it.body())]
            } else {
                continue;
            };
        for (keyword, body) in bodies {
            let (Some(keyword), Some(body)) = (keyword, body) else {
                continue;
            };
            if !begins(body.syntax()) {
                let message = format!("`{}` without `begin`", keyword.text());
                cx.report(
                    cx.diagnostic(keyword.text_range(), message)
                        .pointing("add `begin` and `end`"),
                );
            }
        }
    }
}

/// Whether `body` is a `begin`-`end` block, or a macro call or an unparsed
/// run that may write one.
fn begins(body: &SyntaxNode) -> bool {
    match body.kind() {
        BLOCK => (body.children_with_tokens())
            .filter_map(|element| element.into_token())
            .any(|token| token.kind() == BEGIN_KW),
        MACRO_CALL | VERBATIM => true,
        _ => false,
    }
}
