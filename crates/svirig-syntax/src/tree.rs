//! The tree, and the one type that joins `rowan` to [`SyntaxKind`].
//!
//! `rowan` stores a kind as a bare `u16` and is generic over a [`rowan::Language`]
//! that says how to read one back. [`SystemVerilog`] is ours, and it is the
//! whole of the join: everything else in this module is a name for a `rowan`
//! type with that parameter already applied, so that no caller has to write
//! the parameter and no two callers can disagree about it.
//!
//! # Green and red
//!
//! `rowan`'s trees come in two layers, and the distinction matters as soon as
//! anything walks one. A *green* node is the immutable, position-free,
//! structurally shared data -- two `always_ff` blocks with the same text are
//! one allocation. A *red* node ([`SyntaxNode`]) is a cursor onto a green
//! node that knows its parent and its absolute offset, created on demand as
//! the tree is walked.
//!
//! A parse therefore produces a `GreenNode`, and [`SyntaxNode::new_root`] is
//! what turns it into something with positions in it.

use crate::SyntaxKind;

/// The language `rowan` is parameterised by.
///
/// Uninhabited: it is only ever a type parameter, and there is no reason to
/// be able to hold one.
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

/// A node in a parsed file: a kind, children, and a position.
pub type SyntaxNode = rowan::SyntaxNode<SystemVerilog>;
/// A leaf, carrying the text it covers.
pub type SyntaxToken = rowan::SyntaxToken<SystemVerilog>;
/// A node or a token, which is what iterating children gives when trivia is
/// wanted as well as structure.
pub type SyntaxElement = rowan::SyntaxElement<SystemVerilog>;
