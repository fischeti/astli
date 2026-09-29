//! The items of a `case` statement.

use astli_syntax::SyntaxKind::{
    CONDITIONAL_BRANCH, CONDITIONAL_REGION, DEFAULT_KW, MACRO_CALL, RANDCASE_KW, STRING_LITERAL,
    UNIQUE_KW, UNIQUE0_KW, VERBATIM,
};
use astli_syntax::ast::{AstNode, CaseItem, CaseStmt};
use astli_syntax::{SyntaxNode, SyntaxToken};
use rowan::TextRange;
use rustc_hash::FxHashMap;

use crate::rule::Cx;

/// A value no item matches leaves every variable the `case` assigns as it
/// was, which in combinational logic is a latch. A `unique` or `unique0`
/// case says no other value occurs, so it needs no `default`.
pub(crate) fn case_missing_default(cx: &mut Cx) {
    for case in cx.root().descendants().filter_map(CaseStmt::cast) {
        let qualified =
            (case.qualifier()).is_some_and(|it| matches!(it.kind(), UNIQUE_KW | UNIQUE0_KW));
        let random = case.keyword().is_some_and(|it| it.kind() == RANDCASE_KW);
        let (Some(keyword), false, false) = (case.keyword(), qualified, random) else {
            continue;
        };
        let arms = arms(case.syntax());
        // A macro call among the items, or items left unparsed, may be the
        // `default`.
        if arms.hidden || arms.items.iter().any(|(item, _)| default(item).is_some()) {
            continue;
        }
        let diagnostic = cx
            .diagnostic(keyword.text_range(), "`case` without a `default` item")
            .pointing("add `default:`, or make it `unique case` if no other value occurs")
            .note("a value no item matches leaves what the `case` assigns unchanged, which in combinational logic is a latch");
        cx.report(diagnostic);
    }
}

/// Two items with the same label: the second is never taken. Labels are
/// compared as written, spacing aside, so `1` and `'d1` are not found to be
/// the same.
pub(crate) fn duplicate_case_item(cx: &mut Cx) {
    for case in cx.root().descendants().filter_map(CaseStmt::cast) {
        // A `randcase`'s labels are weights, which may repeat.
        if case.keyword().is_some_and(|it| it.kind() == RANDCASE_KW) {
            continue;
        }
        let arms = arms(case.syntax());
        let mut seen: FxHashMap<String, Vec<(TextRange, &[SyntaxNode])>> = FxHashMap::default();
        for (item, branches) in &arms.items {
            for label in item.exprs() {
                let range = Cx::range(label.syntax());
                let earlier = seen.entry(spelled(label.syntax())).or_default();
                let first = earlier
                    .iter()
                    .find(|(_, before)| !exclusive(before, branches))
                    .map(|(first, _)| *first);
                if let Some(first) = first {
                    let text = &cx.tree.source()[range];
                    let diagnostic = cx
                        .diagnostic(range, format!("`{text}` is already an item of this `case`"))
                        .pointing("never matched here")
                        .label(cx.tree.span(first), "matched here first");
                    cx.report(diagnostic);
                }
                earlier.push((range, branches));
            }
        }
    }
}

/// A `case`'s items, including those inside `` `ifdef `` branches, each with
/// the branches it stands in, outermost first.
struct Arms {
    items: Vec<(CaseItem, Vec<SyntaxNode>)>,
    /// Whether a macro call or an unparsed run stands among them.
    hidden: bool,
}

fn arms(case: &SyntaxNode) -> Arms {
    fn walk(node: &SyntaxNode, branches: &mut Vec<SyntaxNode>, arms: &mut Arms) {
        for child in node.children() {
            match child.kind() {
                CONDITIONAL_REGION => walk(&child, branches, arms),
                CONDITIONAL_BRANCH => {
                    branches.push(child.clone());
                    walk(&child, branches, arms);
                    branches.pop();
                }
                MACRO_CALL | VERBATIM => arms.hidden = true,
                _ => {
                    if let Some(item) = CaseItem::cast(child) {
                        arms.items.push((item, branches.clone()));
                    }
                }
            }
        }
    }
    let mut arms = Arms {
        items: Vec::new(),
        hidden: false,
    };
    walk(case, &mut Vec::new(), &mut arms);
    arms
}

/// Whether two items stand in different branches of one `` `ifdef ``, so no
/// build has both.
fn exclusive(one: &[SyntaxNode], other: &[SyntaxNode]) -> bool {
    one.iter().any(|mine| {
        other
            .iter()
            .any(|theirs| mine != theirs && mine.parent() == theirs.parent())
    })
}

fn default(item: &CaseItem) -> Option<SyntaxToken> {
    (item.syntax().children_with_tokens())
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == DEFAULT_KW)
}

/// A node's tokens without trivia, run together: the same label whatever
/// its spacing, which a number may have inside it (`2 'd 1`). Only a string
/// keeps its spaces.
fn spelled(node: &SyntaxNode) -> String {
    let mut text = String::new();
    let tokens = (node.descendants_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    for token in tokens {
        match token.kind() {
            STRING_LITERAL => text.push_str(token.text()),
            _ => text.extend(token.text().chars().filter(|c| !c.is_whitespace())),
        }
    }
    text
}
