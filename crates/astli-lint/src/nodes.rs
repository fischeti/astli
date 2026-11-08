//! A file's nodes by kind, from one walk of its tree.

use astli_syntax::SyntaxKind::{
    self, CONSTRAINT_KW, DEFPARAM_KW, REG_KW, SYSTEM_IDENT, TICK_IDENT,
};
use astli_syntax::ast::AstNode;
use astli_syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
use rustc_hash::FxHashMap;

/// The kinds of token a rule looks for wherever they stand. Holding every
/// token would hold most of the tree's cursors at once.
const TOKENS: &[SyntaxKind] = &[CONSTRAINT_KW, DEFPARAM_KW, REG_KW, SYSTEM_IDENT, TICK_IDENT];

/// Each node of a file, by its kind, and the tokens of the kinds in
/// [`TOKENS`].
///
/// A rule asks for the nodes of its kinds rather than walking the file:
/// with every rule walking it, the walks were most of what linting a file
/// took.
pub(crate) struct Nodes {
    /// Each kind's nodes, in the order of the file, with their place in it.
    nodes: FxHashMap<SyntaxKind, Vec<(u32, SyntaxNode)>>,
    tokens: FxHashMap<SyntaxKind, Vec<SyntaxToken>>,
}

impl Nodes {
    pub fn new(root: &SyntaxNode) -> Nodes {
        let mut nodes: FxHashMap<SyntaxKind, Vec<(u32, SyntaxNode)>> = FxHashMap::default();
        let mut tokens: FxHashMap<SyntaxKind, Vec<SyntaxToken>> = FxHashMap::default();
        for (at, element) in (0..).zip(root.descendants_with_tokens()) {
            match element {
                SyntaxElement::Node(node) => nodes.entry(node.kind()).or_default().push((at, node)),
                SyntaxElement::Token(token) if TOKENS.contains(&token.kind()) => {
                    tokens.entry(token.kind()).or_default().push(token);
                }
                SyntaxElement::Token(_) => {}
            }
        }
        Nodes { nodes, tokens }
    }

    /// The nodes `N` views, in the order of the file.
    pub fn of<N: AstNode>(&self) -> impl Iterator<Item = N> + use<N> {
        self.matching(N::can_cast).into_iter().filter_map(N::cast)
    }

    /// The nodes of `kinds`, in the order of the file.
    pub fn kinds(&self, kinds: &[SyntaxKind]) -> std::vec::IntoIter<SyntaxNode> {
        self.matching(|kind| kinds.contains(&kind)).into_iter()
    }

    /// The tokens of `kind`, one of [`TOKENS`], in the order of the file.
    pub fn tokens(&self, kind: SyntaxKind) -> impl Iterator<Item = SyntaxToken> + '_ {
        debug_assert!(TOKENS.contains(&kind), "{kind:?} is not gathered");
        self.tokens.get(&kind).into_iter().flatten().cloned()
    }

    fn matching(&self, wanted: impl Fn(SyntaxKind) -> bool) -> Vec<SyntaxNode> {
        let mut lists = (self.nodes.iter()).filter(|(kind, _)| wanted(**kind));
        let (Some((_, first)), second) = (lists.next(), lists.next()) else {
            return Vec::new();
        };
        let Some((_, second)) = second else {
            return first.iter().map(|(_, node)| node.clone()).collect();
        };
        let mut all: Vec<&(u32, SyntaxNode)> = (first.iter().chain(second))
            .chain(lists.flat_map(|(_, list)| list))
            .collect();
        all.sort_unstable_by_key(|(at, _)| *at);
        all.into_iter().map(|(_, node)| node.clone()).collect()
    }
}
