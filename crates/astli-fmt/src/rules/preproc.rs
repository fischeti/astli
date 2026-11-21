//! Conditional regions and their directives.

use astli_syntax::{SyntaxElement, SyntaxKind::*, SyntaxNode};
use rowan::NodeOrToken;

use super::items::blank_line_before;
use super::{Writer, significant_children};
use crate::doc::Doc;

impl Writer<'_> {
    /// A conditional region: each directive on a line of its own at the
    /// margin, and each branch's items where they would be without them.
    pub(super) fn conditional_region(&mut self, region: &SyntaxNode) -> Doc {
        let children = significant_children(region);
        let last = children.len().saturating_sub(1);
        let plain = children.iter().enumerate().all(|(at, child)| match child {
            NodeOrToken::Node(branch) => branch.kind() == CONDITIONAL_BRANCH && is_plain(branch),
            NodeOrToken::Token(_) => at == last,
        });
        if !plain {
            return self.verbatim(region);
        }

        let mut docs = Vec::new();
        for child in &children {
            match child {
                NodeOrToken::Node(branch) => {
                    let children = significant_children(branch);
                    let items = children.iter().position(|it| it.as_node().is_some());
                    let (directive, items) = children.split_at(items.unwrap_or(children.len()));
                    docs.extend([
                        self.comments.leading(branch),
                        self.directive(directive),
                        self.items(items),
                        self.comments.trailing(branch),
                    ]);
                }
                NodeOrToken::Token(_) => docs.push(self.directive(std::slice::from_ref(child))),
            }
        }
        return Doc::concat(docs);

        /// A directive and its condition, then the branch's items, or the
        /// entries of a list and their commas.
        fn is_plain(branch: &SyntaxNode) -> bool {
            let children = significant_children(branch);
            let tokens = children.iter().take_while(|it| it.as_token().is_some());
            let tokens = tokens.count();
            let expression = || {
                tokens > 2
                    && children[1].kind() == L_PAREN
                    && children[tokens - 1].kind() == R_PAREN
            };
            ((1..=2).contains(&tokens) || expression())
                && children[tokens..]
                    .iter()
                    .all(|it| it.as_node().is_some() || it.kind() == COMMA)
        }
    }

    /// The tokens of a directive on a line of their own at the margin.
    fn directive(&mut self, tokens: &[SyntaxElement]) -> Doc {
        let mut docs = vec![Doc::HardLine];
        if let Some(NodeOrToken::Token(first)) = tokens.first() {
            docs.push(blank_line_before(first));
        }
        // A condition's tokens keep the spacing they were written with, so
        // `` `ifdef (A && !B) `` reads as it did.
        for (at, token) in tokens.iter().enumerate() {
            let apart = (token.prev_sibling_or_token()).is_none_or(|it| it.kind().is_trivia());
            if at == 0 || apart {
                docs.push(Doc::Space);
            }
            docs.push(self.element(token));
        }
        Doc::margin(Doc::concat(docs))
    }
}

/// Whether `node` stands where the preprocessor, not the grammar, decides
/// what goes.
pub(super) fn is_preproc(node: &SyntaxNode) -> bool {
    matches!(node.kind(), MACRO_CALL | DIRECTIVE | CONDITIONAL_REGION)
}
