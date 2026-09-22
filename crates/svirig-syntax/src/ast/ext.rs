//! Accessors the generator cannot write, because only a child's position or
//! the token before it says which it is.

use super::{AstChildren, AstNode, ClassDecl, ConcatExpr, Dimension, Expr, PatternItem};
use super::{StreamExpr, TypeOrExpr, TypeRef, support};
use crate::SyntaxKind::{EXTENDS_KW, IMPLEMENTS_KW};
use crate::{SyntaxElement, SyntaxKind};

impl PatternItem {
    /// What `key: value` assigns to; `None` for a positional item.
    pub fn key(&self) -> Option<Expr> {
        let mut exprs = support::children::<Expr>(self.syntax());
        let first = exprs.next();
        exprs.next().and(first)
    }

    pub fn value(&self) -> Option<Expr> {
        support::children::<Expr>(self.syntax()).last()
    }
}

impl StreamExpr {
    /// The slice size or type between the operator and the concatenation.
    pub fn slice(&self) -> Option<TypeOrExpr> {
        let mut nodes = self.syntax().children();
        let first = nodes.next()?;
        nodes.next().and_then(|_| TypeOrExpr::cast(first))
    }

    /// The concatenation being streamed, which is always the last child.
    pub fn concat(&self) -> Option<ConcatExpr> {
        self.syntax().children().last().and_then(ConcatExpr::cast)
    }
}

impl ClassDecl {
    pub fn extends(&self) -> Option<TypeRef> {
        after(self.syntax(), EXTENDS_KW).find_map(TypeRef::cast)
    }

    pub fn implements(&self) -> impl Iterator<Item = TypeRef> {
        after(self.syntax(), IMPLEMENTS_KW).filter_map(TypeRef::cast)
    }
}

impl Dimension {
    /// The one or two things between the brackets: a size, the bounds of a
    /// range, or an associative array's index type.
    pub fn bounds(&self) -> AstChildren<TypeOrExpr> {
        support::children(self.syntax())
    }
}

/// The child nodes after the first token of `kind`, or none if there is no
/// such token.
fn after(
    node: &crate::SyntaxNode,
    kind: SyntaxKind,
) -> impl Iterator<Item = crate::SyntaxNode> + use<> {
    node.children_with_tokens()
        .skip_while(move |element| element.kind() != kind)
        .filter_map(SyntaxElement::into_node)
}
