//! Accessors the generator does not write: where two children could be of one
//! type, only their position, or the token before them, says which is which.

use super::{Assignment, BinExpr, CastExpr, ClassDecl, ConcatExpr, Dimension, Expr, IfStmt};
use super::{AstChildren, AstNode, ImmediateAssertion, support};
use super::{Bins, CovergroupDecl, Coverpoint, Cross, DataType, DistItem, PortList, WithClause};
use super::{ConcurrentAssertion, CycleDelay, PropertyBinExpr, PropertyIf, PropertyParen};
use super::{Declarator, PropExpr, TernaryExpr, TypeOrExpr, TypeRef, WaitOrderStmt};
use super::{IndexExpr, Item, MinTypMaxExpr, ParenExpr, PatternItem, ReplicationExpr, StreamExpr};
use super::{PropertySpec, PropertyUnaryExpr, Repetition, SequenceDelay};
use crate::SyntaxKind::{COMMA, ELSE_KW, EQ, EXTENDS_KW, IFF_KW, IMPLEMENTS_KW, L_BRACK, R_BRACK};
use crate::SyntaxKind::{COVERPOINT_KW, FUNCTION_KW, MATCHES_KW, WITH_KW};
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

impl MinTypMaxExpr {
    pub fn min(&self) -> Option<Expr> {
        nth(self.syntax(), 0)
    }

    pub fn typ(&self) -> Option<Expr> {
        nth(self.syntax(), 1)
    }

    pub fn max(&self) -> Option<Expr> {
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

impl ImmediateAssertion {
    /// What runs when the condition holds, which may be left out before
    /// `else`.
    pub fn pass(&self) -> Option<Item> {
        let pass = self.syntax().children_with_tokens();
        pass.take_while(|element| element.kind() != ELSE_KW)
            .filter_map(SyntaxElement::into_node)
            .find_map(Item::cast)
    }

    pub fn fail(&self) -> Option<Item> {
        after(self.syntax(), ELSE_KW).find_map(Item::cast)
    }
}

impl ConcurrentAssertion {
    /// What runs when the property holds, which may be left out before
    /// `else`.
    pub fn pass(&self) -> Option<Item> {
        let pass = self.syntax().children_with_tokens();
        pass.take_while(|element| element.kind() != ELSE_KW)
            .filter_map(SyntaxElement::into_node)
            .find_map(Item::cast)
    }

    pub fn fail(&self) -> Option<Item> {
        after(self.syntax(), ELSE_KW).find_map(Item::cast)
    }
}

impl PropertySpec {
    /// The condition after `disable iff`.
    pub fn disable(&self) -> Option<ParenExpr> {
        after(self.syntax(), IFF_KW)
            .next()
            .and_then(ParenExpr::cast)
    }

    /// The property, which is always the last child: one in parentheses may
    /// follow `disable iff (…)`.
    pub fn property(&self) -> Option<PropExpr> {
        self.syntax().children().last().and_then(PropExpr::cast)
    }
}

impl PropertyBinExpr {
    pub fn lhs(&self) -> Option<PropExpr> {
        nth_node(self.syntax(), 0)
    }

    pub fn rhs(&self) -> Option<PropExpr> {
        nth_node(self.syntax(), 1)
    }
}

impl PropertyUnaryExpr {
    /// The range in `[…]`: its one bound, or its low and its high.
    pub fn bounds(&self) -> impl Iterator<Item = Expr> + use<> {
        bracketed(self.syntax())
    }

    /// The condition of `accept_on (…)` and the like, before the property.
    pub fn condition(&self) -> Option<ParenExpr> {
        let mut nodes = self.syntax().children();
        let first = nodes.next()?;
        let bracketed = self.l_brack_token().is_some();
        (!bracketed && nodes.next().is_some())
            .then(|| ParenExpr::cast(first))
            .flatten()
    }

    /// The property the operator applies to, which is always the last child.
    pub fn operand(&self) -> Option<PropExpr> {
        self.syntax().children().last().and_then(PropExpr::cast)
    }
}

impl PropertyParen {
    /// The sequence that matches.
    pub fn sequence(&self) -> Option<PropExpr> {
        nth_node(self.syntax(), 0)
    }

    /// What a match assigns, after the sequence: an assignment, or an
    /// increment or a decrement.
    pub fn items(&self) -> impl Iterator<Item = SyntaxNode> + use<> {
        after(self.syntax(), COMMA)
    }
}

impl PropertyIf {
    pub fn condition(&self) -> Option<ParenExpr> {
        nth_node(self.syntax(), 0)
    }

    pub fn then_branch(&self) -> Option<PropExpr> {
        nth_node(self.syntax(), 1)
    }

    pub fn else_branch(&self) -> Option<PropExpr> {
        after(self.syntax(), ELSE_KW).find_map(PropExpr::cast)
    }
}

impl SequenceDelay {
    /// The sequence before the delay; `None` when the delay starts it.
    pub fn lhs(&self) -> Option<PropExpr> {
        let first = self.syntax().children().next()?;
        PropExpr::cast(first)
    }

    pub fn rhs(&self) -> Option<PropExpr> {
        let after = self
            .syntax()
            .children()
            .skip_while(|node| !CycleDelay::can_cast(node.kind()));
        after.skip(1).find_map(PropExpr::cast)
    }
}

impl CycleDelay {
    /// The cycles it waits: one count, or the low and the high of a range;
    /// none for `[*]` and `[+]`.
    pub fn bounds(&self) -> AstChildren<Expr> {
        support::children(self.syntax())
    }
}

impl Repetition {
    pub fn operand(&self) -> Option<PropExpr> {
        nth_node(self.syntax(), 0)
    }

    /// How often it repeats: one count, or the low and the high of a range;
    /// none for `[*]` and `[+]`.
    pub fn bounds(&self) -> impl Iterator<Item = Expr> + use<> {
        bracketed(self.syntax())
    }
}

impl Declarator {
    /// The value after `=`, unless it is a sequence or a property, which only
    /// a formal defaults to.
    pub fn init(&self) -> Option<TypeOrExpr> {
        support::child(self.syntax())
    }

    /// The sequence or property after `=`, or the expression it may be.
    pub fn actual(&self) -> Option<PropExpr> {
        support::child(self.syntax())
    }
}

impl WaitOrderStmt {
    /// What runs when the events come in turn, which may be left out
    /// before `else`.
    pub fn pass(&self) -> Option<Item> {
        let pass = self.syntax().children_with_tokens();
        pass.take_while(|element| element.kind() != ELSE_KW)
            .filter_map(SyntaxElement::into_node)
            .find_map(Item::cast)
    }

    pub fn fail(&self) -> Option<Item> {
        after(self.syntax(), ELSE_KW).find_map(Item::cast)
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

impl CovergroupDecl {
    /// What `new` passes it, before any `with function sample`.
    pub fn formals(&self) -> Option<PortList> {
        before(self.syntax(), WITH_KW).find_map(PortList::cast)
    }

    /// The arguments of `with function sample (…)`.
    pub fn sample(&self) -> Option<PortList> {
        after(self.syntax(), FUNCTION_KW).find_map(PortList::cast)
    }
}

impl Coverpoint {
    /// The type of a typed cover point.
    pub fn data_type(&self) -> Option<DataType> {
        before(self.syntax(), COVERPOINT_KW).find_map(DataType::cast)
    }

    /// What it samples.
    pub fn expr(&self) -> Option<Expr> {
        after(self.syntax(), COVERPOINT_KW).find_map(Expr::cast)
    }

    /// The condition after `iff`.
    pub fn condition(&self) -> Option<ParenExpr> {
        after(self.syntax(), IFF_KW).find_map(ParenExpr::cast)
    }
}

impl Cross {
    /// The cover points it crosses.
    pub fn points(&self) -> impl Iterator<Item = Expr> + use<> {
        before(self.syntax(), IFF_KW).filter_map(Expr::cast)
    }

    /// The condition after `iff`.
    pub fn condition(&self) -> Option<ParenExpr> {
        after(self.syntax(), IFF_KW).find_map(ParenExpr::cast)
    }
}

impl Bins {
    /// The number of bins in `[n]`.
    pub fn count(&self) -> Option<Expr> {
        before(self.syntax(), EQ).find_map(Expr::cast)
    }

    /// The value after `=`, when it is an expression: a cover point, or a
    /// cross's selection of bins.
    pub fn value(&self) -> Option<Expr> {
        after(self.syntax(), EQ).next().and_then(Expr::cast)
    }

    /// How many of the selected bins must match, after `matches`.
    pub fn matches(&self) -> Option<Expr> {
        after(self.syntax(), MATCHES_KW).find_map(Expr::cast)
    }

    /// The condition after `iff`.
    pub fn condition(&self) -> Option<ParenExpr> {
        after(self.syntax(), IFF_KW).find_map(ParenExpr::cast)
    }
}

impl WithClause {
    /// An array method's expression, or the names of a `randomize`'s
    /// object its constraints may restrict.
    pub fn condition(&self) -> Option<ParenExpr> {
        before(self.syntax(), L_BRACK).find_map(ParenExpr::cast)
    }
}

impl DistItem {
    /// Its value, or its range's two bounds, then its weight.
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(self.syntax())
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

/// The child nodes before the first token of `kind`, or all of them if
/// there is no such token.
fn before(node: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxNode> + use<> {
    node.children_with_tokens()
        .take_while(move |element| element.kind() != kind)
        .filter_map(SyntaxElement::into_node)
}

/// The expressions between `[` and `]`.
fn bracketed(node: &SyntaxNode) -> impl Iterator<Item = Expr> + use<> {
    node.children_with_tokens()
        .skip_while(|element| element.kind() != L_BRACK)
        .take_while(|element| element.kind() != R_BRACK)
        .filter_map(SyntaxElement::into_node)
        .filter_map(Expr::cast)
}
