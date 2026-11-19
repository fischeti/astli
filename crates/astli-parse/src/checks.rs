//! What can only be judged on the finished tree.

use crate::{Parsed, diagnostics};
use astli_syntax::SyntaxKind::*;
use astli_text::{Diagnostic, Span};
use rowan::WalkEvent;

/// `parsed`, with a `` `resetall `` inside a design element reported.
///
/// The parser does not see it: it is trivia in an expanded tree, and its
/// place is that of the trivia, which only building the tree decides. Trivia
/// belongs to the node after it, so a `` `resetall `` written before a
/// `module` is inside the node; it is inside the element only once past the
/// element's first token.
pub(crate) fn checked(mut parsed: Parsed) -> Parsed {
    if !parsed.holds.resetall {
        return parsed;
    }
    let misplaced: Vec<Diagnostic> = (parsed.root.descendants_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| {
            matches!(token.kind(), TICK_IDENT | DIRECTIVE_TRIVIA) && token.text() == "`resetall"
        })
        .filter(|token| {
            token.parent_ancestors().any(|node| {
                matches!(
                    node.kind(),
                    MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL
                ) && (node.descendants_with_tokens())
                    .filter_map(|element| element.into_token())
                    .find(|first| !first.kind().is_trivia())
                    .is_some_and(|first| first.text_range().start() < token.text_range().start())
            })
        })
        .filter_map(|token| parsed.span(&token))
        .map(diagnostics::misplaced_resetall)
        .collect();
    parsed.diagnostics.extend(misplaced);
    parsed
}

impl Parsed {
    /// The first and last token of each outermost `VERBATIM` node that no
    /// diagnostic explains.
    pub(crate) fn runs(&self) -> Vec<(Span, Span)> {
        let mut found = Vec::new();
        if !self.holds.verbatim {
            return found;
        }
        let mut walk = self.root.preorder();
        while let Some(event) = walk.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };
            if node.kind() != VERBATIM {
                continue;
            }
            walk.skip_subtree();

            let spans: Vec<Span> = (node.descendants_with_tokens())
                .filter_map(|element| element.into_token())
                .filter(|token| !token.kind().is_trivia())
                .filter_map(|token| self.span(&token))
                .collect();
            let explained = spans
                .iter()
                .any(|span| self.diagnostics.iter().any(|it| it.at == *span));
            if let (Some(&first), Some(&last)) = (spans.first(), spans.last())
                && !explained
            {
                found.push((first, last));
            }
        }
        found
    }
}
