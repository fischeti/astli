//! Typed views of the syntax tree: a type per node kind, and an enum per union
//! of them, generated from `astli.ungram`.
//!
//! A view is a [`SyntaxNode`] whose kind has been checked, and nothing more.
//! It holds no data of its own, and every accessor walks the node's children
//! when it is called. An accessor returns `None` where the tree has no such
//! child: because the source leaves it out, or because the parser handed that
//! stretch to the fallback.
//!
//! An accessor that returns [`AstChildren`] yields every child of its type, in
//! order, wherever in the node it stands. A module's `import_decls` are the
//! ones in its header and the ones in its body alike.

mod ext;
// Formatted by the generator, which checks it byte for byte.
#[rustfmt::skip]
mod generated;

pub use generated::*;

use std::marker::PhantomData;

use crate::{SyntaxKind, SyntaxNode, SyntaxToken, SystemVerilog};

/// A typed view of one [`SyntaxNode`].
pub trait AstNode: Sized {
    /// Whether a node of `kind` can be viewed as `Self`.
    fn can_cast(kind: SyntaxKind) -> bool;

    /// Views `node` as `Self`, if its kind allows it.
    fn cast(node: SyntaxNode) -> Option<Self>;

    /// The node this view is of.
    fn syntax(&self) -> &SyntaxNode;
}

/// The children of a node that can be viewed as `N`, in order.
#[derive(Debug, Clone)]
pub struct AstChildren<N> {
    inner: rowan::SyntaxNodeChildren<SystemVerilog>,
    kind: PhantomData<N>,
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;

    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}

/// What the generated accessors are written in terms of.
mod support {
    use super::*;

    /// The first child that can be viewed as `N`.
    pub fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
        parent.children().find_map(N::cast)
    }

    pub fn children<N: AstNode>(parent: &SyntaxNode) -> AstChildren<N> {
        AstChildren {
            inner: parent.children(),
            kind: PhantomData,
        }
    }

    /// The first token of any of `kinds` among the node's own tokens.
    pub fn token(parent: &SyntaxNode, kinds: &[SyntaxKind]) -> Option<SyntaxToken> {
        parent
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| kinds.contains(&token.kind()))
    }
}
