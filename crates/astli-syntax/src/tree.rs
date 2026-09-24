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
