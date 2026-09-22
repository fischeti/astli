//! Accessors the generator does not write: where two children could be of one
//! type, only their position, or the token before them, says which is which.

use super::{Assignment, BinExpr, CastExpr, ClassDecl, ConcatExpr, Dimension, Expr, IfStmt};
use super::{AstChildren, AstNode, support};
use super::{IndexExpr, Item, ParenExpr, PatternItem, ReplicationExpr, StreamExpr};
use super::{TernaryExpr, TypeOrExpr, TypeRef};
use crate::SyntaxKind::{EXTENDS_KW, IMPLEMENTS_KW};
use crate::{SyntaxElement, SyntaxKind, SyntaxNode};

/// The `n`th child that can be viewed as `N`.
fn nth<N: AstNode>(node: &SyntaxNode, n: usize) -> Option<N> {
    support::children(node).nth(n)
}

/// The `n`th child node, if it can be viewed as `N`: for a node whose children
/// are of different types that overlap.
fn nth_node<N: AstNode>(node: &SyntaxNode, n: usize) -> Option<N> {
    node.children().nth(n).and_then(N::cast)
}

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        nth(self.syntax(), 0)
    }

    pub fn rhs(&self) -> Option<Expr> {
        nth(self.syntax(), 1)
    }
}

impl Assignment {
    pub fn lhs(&self) -> Option<Expr> {
        nth(self.syntax(), 0)
    }

    pub fn rhs(&self) -> Option<Expr> {
        nth(self.syntax(), 1)
    }
}

impl TernaryExpr {
    pub fn condition(&self) -> Option<Expr> {
        nth(self.syntax(), 0)
    }

    pub fn then_value(&self) -> Option<Expr> {
        nth(self.syntax(), 1)
    }

    pub fn else_value(&self) -> Option<Expr> {
        nth(self.syntax(), 2)
    }
}

impl IndexExpr {
    pub fn base(&self) -> Option<Expr> {
        nth(self.syntax(), 0)
    }

    pub fn index(&self) -> Option<Expr> {
        nth(self.syntax(), 1)
    }

    /// The second bound of a range select, `lo` in `a[hi:lo]`.
    pub fn end(&self) -> Option<Expr> {
        nth(self.syntax(), 2)
    }
}

impl CastExpr {
    /// The type cast to, which the tree holds as an expression: `int`, `8`,
    /// `T::U`, `(W)`.
    pub fn ty(&self) -> Option<Expr> {
        nth_node(self.syntax(), 0)
    }

    pub fn operand(&self) -> Option<ParenExpr> {
        nth_node(self.syntax(), 1)
    }
}

impl ReplicationExpr {
    pub fn count(&self) -> Option<Expr> {
        nth_node(self.syntax(), 0)
    }

    pub fn concat(&self) -> Option<ConcatExpr> {
        nth_node(self.syntax(), 1)
    }
}

impl IfStmt {
    pub fn then_branch(&self) -> Option<Item> {
        nth(self.syntax(), 0)
    }

    pub fn else_branch(&self) -> Option<Item> {
        nth(self.syntax(), 1)
    }
}

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
fn after(node: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxNode> + use<> {
    node.children_with_tokens()
        .skip_while(move |element| element.kind() != kind)
        .filter_map(SyntaxElement::into_node)
}
