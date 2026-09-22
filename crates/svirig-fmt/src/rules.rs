//! The rules, one per construct, and the fallback that writes a node no rule
//! lays out as it was read.
//!
//! A rule writes every token of its node, and takes every comment the comment
//! map gives to the node, its descendants or its tokens.

use rowan::NodeOrToken;
use svirig_syntax::{SyntaxKind::*, SyntaxNode, SyntaxToken};

use crate::comments::Comments;
use crate::doc::Doc;
use crate::verbatim::verbatim;

/// The document for the file `root` holds, and the nodes no rule laid out.
pub(crate) fn write(root: &SyntaxNode, source: &str) -> (Doc, Vec<SyntaxNode>) {
    let mut writer = Writer {
        source,
        comments: Comments::new(root, source),
        unformatted: Vec::new(),
    };
    let doc = writer.source_file(root);
    debug_assert!(
        writer.comments.untaken().is_none(),
        "no rule wrote {:?}",
        writer.comments.untaken()
    );
    (doc, writer.unformatted)
}

struct Writer<'a> {
    source: &'a str,
    comments: Comments,
    /// The outermost of the nodes written as they were read.
    unformatted: Vec<SyntaxNode>,
}

impl Writer<'_> {
    /// Items one after another, each on lines of its own.
    fn source_file(&mut self, file: &SyntaxNode) -> Doc {
        let mut docs = vec![self.comments.head()];
        for child in file.children_with_tokens() {
            match child {
                NodeOrToken::Node(item) => docs.push(self.item(&item)),
                NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                    docs.extend([Doc::HardLine, self.token(&token)]);
                }
                NodeOrToken::Token(_) => {}
            }
        }
        Doc::concat(docs)
    }

    /// An item on lines of its own, after an empty line if it had one.
    fn item(&mut self, item: &SyntaxNode) -> Doc {
        Doc::concat([
            Doc::HardLine,
            self.comments.leading(item),
            blank_line_before(item),
            self.layout(item),
            self.comments.trailing(item),
        ])
    }

    /// `node` without the comments around it.
    fn layout(&mut self, node: &SyntaxNode) -> Doc {
        self.verbatim(node)
    }

    fn token(&mut self, token: &SyntaxToken) -> Doc {
        Doc::concat([Doc::token(token.text()), self.comments.after(token)])
    }

    fn verbatim(&mut self, node: &SyntaxNode) -> Doc {
        let Some(verbatim) = verbatim(node, self.source) else {
            return Doc::nil();
        };
        self.comments.within(node);
        self.unformatted.push(node.clone());
        Doc::Verbatim(verbatim)
    }
}

/// An empty line if the input has one right before `node`'s first token.
///
/// Comments before it carry their own separation, so only the whitespace
/// between the last of them and the token is read.
fn blank_line_before(node: &SyntaxNode) -> Doc {
    let first = (node.descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .find(|token| !token.kind().is_trivia());
    let before = first.and_then(|token| token.prev_token());
    match before {
        Some(space) if space.kind() == WHITESPACE && space.text().matches('\n').count() > 1 => {
            Doc::BlankLine
        }
        _ => Doc::nil(),
    }
}
