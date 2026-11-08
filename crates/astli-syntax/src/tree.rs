//! Concrete syntax tree types using `rowan`.
//!
//! This module binds [`SyntaxKind`] to `rowan`'s CST data structures through the
//! [`SystemVerilog`] language marker type, providing type aliases for working with
//! syntax nodes, tokens, and elements.

use crate::SyntaxKind;

/// Language marker type parameterizing `rowan` CST structures for SystemVerilog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemVerilog {}

impl rowan::Language for SystemVerilog {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Syntax tree interior node containing child nodes, tokens, and span information.
pub type SyntaxNode = rowan::SyntaxNode<SystemVerilog>;

/// Syntax tree leaf node representing an individual token and its covered text.
pub type SyntaxToken = rowan::SyntaxToken<SystemVerilog>;

/// Element representing either an interior [`SyntaxNode`] or a leaf [`SyntaxToken`].
pub type SyntaxElement = rowan::SyntaxElement<SystemVerilog>;

/// The first token of `node` that is not trivia, or `None` if it holds only
/// trivia.
///
/// It is found from the node's start and stops there, so finding it costs
/// the trivia and the nodes in front of it rather than the size of `node`:
/// a caller asking for every node's ends, as lowering does, would otherwise
/// walk each token once for every node around it.
pub fn first_significant(node: &SyntaxNode) -> Option<SyntaxToken> {
    std::iter::successors(
        node.first_child_or_token(),
        SyntaxElement::next_sibling_or_token,
    )
    .find_map(|element| significant(element, first_significant))
}

/// The last token of `node` that is not trivia, found from its end, as
/// [`first_significant`] is from its start.
pub fn last_significant(node: &SyntaxNode) -> Option<SyntaxToken> {
    std::iter::successors(
        node.last_child_or_token(),
        SyntaxElement::prev_sibling_or_token,
    )
    .find_map(|element| significant(element, last_significant))
}

/// The range from `node`'s first significant token to its last: the node
/// without the whitespace and comments the tree puts ahead of it, or
/// behind it.
pub fn significant_range(node: &SyntaxNode) -> Option<rowan::TextRange> {
    let first = first_significant(node)?;
    let last = last_significant(node)?;
    Some(first.text_range().cover(last.text_range()))
}

/// `element` if it is a token that is not trivia, or the significant token
/// `inside` finds in it if it is a node.
fn significant(
    element: SyntaxElement,
    inside: fn(&SyntaxNode) -> Option<SyntaxToken>,
) -> Option<SyntaxToken> {
    match element {
        rowan::NodeOrToken::Token(token) => (!token.kind().is_trivia()).then_some(token),
        rowan::NodeOrToken::Node(node) => inside(&node),
    }
}
