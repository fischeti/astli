//! Directives and macro calls, as written.

use astli_syntax::SyntaxKind::{
    BLOCK_COMMENT, CONDITIONAL_BRANCH, CONDITIONAL_REGION, EXPR_STMT, IDENT, LINE_COMMENT,
    SEMICOLON, TICK_IDENT, WHITESPACE,
};
use astli_syntax::ast::{AstNode, MacroCall};
use astli_syntax::{SyntaxNode, SyntaxToken};

use crate::rule::Cx;

/// An `` `endif `` far from its `` `ifdef `` says nothing of which one it
/// closes; a comment naming the macro does.
pub(crate) fn endif_comment(cx: &mut Cx) {
    for region in cx
        .root()
        .descendants()
        .filter(|it| it.kind() == CONDITIONAL_REGION)
    {
        let Some(name) = opening(&region) else {
            continue;
        };
        let Some(endif) = (region.children_with_tokens())
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == TICK_IDENT)
        else {
            continue;
        };
        if trailing_comment(&endif).is_some_and(|comment| names(&comment, name.text())) {
            continue;
        }
        let message = format!("`endif without a comment naming `{}`", name.text());
        let diagnostic = cx
            .diagnostic(endif.text_range(), message)
            .pointing(format!("add `// {}`", name.text()));
        cx.report(diagnostic);
    }
}

/// UVM's macros that expand to statements end in their own `;`, so one
/// written after the call is an empty statement, and is an error after a
/// macro that declares.
pub(crate) fn uvm_macro_semicolon(cx: &mut Cx) {
    for call in cx.root().descendants().filter_map(MacroCall::cast) {
        let Some(name) = (call.syntax().children_with_tokens())
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == TICK_IDENT)
        else {
            continue;
        };
        if !name.text().starts_with("`uvm_") {
            continue;
        }
        let empty = call.syntax().next_sibling().filter(|next| {
            next.kind() == EXPR_STMT
                && next.children().next().is_none()
                && (next.children_with_tokens())
                    .filter_map(|element| element.into_token())
                    .filter(|token| !token.kind().is_trivia())
                    .all(|token| token.kind() == SEMICOLON)
        });
        let Some(semicolon) = empty.and_then(|it| it.last_token()) else {
            continue;
        };
        let message = format!("`;` after {}", name.text());
        let diagnostic = cx
            .diagnostic(semicolon.text_range(), message)
            .pointing("the macro ends its statement itself");
        cx.report(diagnostic);
    }
}

/// The macro an `` `ifdef `` or `` `ifndef `` region opens on.
fn opening(region: &SyntaxNode) -> Option<SyntaxToken> {
    let branch = region
        .children()
        .find(|it| it.kind() == CONDITIONAL_BRANCH)?;
    let mut tokens = (branch.children_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    let directive = tokens.next()?;
    matches!(directive.text(), "`ifdef" | "`ifndef").then_some(())?;
    tokens.next().filter(|token| token.kind() == IDENT)
}

/// The comment on the same line after `token`, if there is one.
fn trailing_comment(token: &SyntaxToken) -> Option<SyntaxToken> {
    let mut next = token.next_token();
    while let Some(token) = next {
        match token.kind() {
            WHITESPACE if !token.text().contains('\n') => next = token.next_token(),
            LINE_COMMENT | BLOCK_COMMENT => return Some(token),
            _ => return None,
        }
    }
    None
}

/// Whether `comment` is the macro's name alone, as `// FOO` or `/* FOO */`.
fn names(comment: &SyntaxToken, name: &str) -> bool {
    let text = comment.text();
    let inner = (text.strip_prefix("//"))
        .or_else(|| text.strip_prefix("/*").and_then(|it| it.strip_suffix("*/")));
    inner.is_some_and(|inner| inner.trim() == name)
}
