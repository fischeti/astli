//! Where each comment is written: before a node, after one, or after a token.
//!
//! The tree builder parks trivia by a rule that suits a round trip, not a
//! layout, so the formatter places comments again. A comment may never cross
//! a token, or the transparency check refuses, so it is placed within the gap
//! it sits in, between the significant tokens `prev` and `next`:
//!
//! - on `prev`'s line, it trails the largest node that ends at `prev`, or
//!   follows `prev` itself if no node ends there;
//! - on a line of its own, it leads the largest node that starts at `next`;
//!   failing that it trails the node that ends at `prev`, as a comment before
//!   `end` belongs with the last statement; failing that it follows `prev`.
//!
//! "Largest" stops below the node that holds both tokens, and below the root.
//! Each comment is taken once by the rule that writes it, and [`Comments::
//! untaken`] finds one that no rule wrote.

use std::cell::Cell;
use std::ops::Range;

use rowan::TextRange;
use rustc_hash::FxHashMap;
use svirig_syntax::{SyntaxKind::*, SyntaxNode, SyntaxToken};

use crate::doc::Doc;

/// Every comment in a file, and where each is to be written.
pub(crate) struct Comments {
    comments: Vec<Comment>,
    leading: FxHashMap<SyntaxNode, Range<usize>>,
    trailing: FxHashMap<SyntaxNode, Range<usize>>,
    after: FxHashMap<SyntaxToken, Range<usize>>,
    /// Before the first token, where no node starts: a file of comments alone.
    head: Range<usize>,
}

struct Comment {
    token: SyntaxToken,
    /// Newlines between the token before and this comment: 0 on the same
    /// line, 2 or more with an empty line between.
    lines_before: usize,
    lines_after: usize,
    place: Place,
    taken: Cell<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Leading(SyntaxNode),
    Trailing(SyntaxNode),
    After(SyntaxToken),
    Head,
}

impl Comments {
    pub(crate) fn new(root: &SyntaxNode, source: &str) -> Comments {
        let mut comments = Vec::new();
        let mut prev: Option<SyntaxToken> = None;
        // The comments since `prev`, each with the newlines before it.
        let mut gap: Vec<(SyntaxToken, usize)> = Vec::new();
        let mut end = 0;

        for token in root
            .descendants_with_tokens()
            .filter_map(|it| it.into_token())
        {
            let start = offset(&token).start;
            match token.kind() {
                WHITESPACE => continue,
                LINE_COMMENT | BLOCK_COMMENT => {
                    gap.push((token.clone(), newlines(source, end, start)))
                }
                _ => {
                    let gap = std::mem::take(&mut gap);
                    place(&mut comments, source, gap, prev.as_ref(), Some(&token));
                    prev = Some(token.clone());
                }
            }
            end = offset(&token).end;
        }
        place(&mut comments, source, gap, prev.as_ref(), None);

        Comments::index(comments)
    }

    fn index(comments: Vec<Comment>) -> Comments {
        let mut leading = FxHashMap::default();
        let mut trailing = FxHashMap::default();
        let mut after = FxHashMap::default();
        let mut head = 0..0;

        // The comments of one place come from one gap, so they are adjacent.
        fn extend(range: &mut Range<usize>, at: usize) {
            if range.start == range.end {
                *range = at..at;
            }
            debug_assert_eq!(range.end, at, "a place's comments are not adjacent");
            range.end = at + 1;
        }
        for (at, comment) in comments.iter().enumerate() {
            match &comment.place {
                Place::Leading(node) => extend(leading.entry(node.clone()).or_default(), at),
                Place::Trailing(node) => extend(trailing.entry(node.clone()).or_default(), at),
                Place::After(token) => extend(after.entry(token.clone()).or_default(), at),
                Place::Head => extend(&mut head, at),
            }
        }

        Comments {
            comments,
            leading,
            trailing,
            after,
            head,
        }
    }

    /// The comments written before `node`, each on the line it was on.
    pub(crate) fn leading(&self, node: &SyntaxNode) -> Doc {
        self.take(self.leading.get(node))
    }

    /// The comments written after `node`.
    pub(crate) fn trailing(&self, node: &SyntaxNode) -> Doc {
        self.take(self.trailing.get(node))
    }

    /// The comments written after `token`, where no node ends.
    pub(crate) fn after(&self, token: &SyntaxToken) -> Doc {
        self.take(self.after.get(token))
    }

    /// The comments of a file with no token for them to go before.
    pub(crate) fn head(&self) -> Doc {
        self.take(Some(&self.head))
    }

    /// Marks the comments between the first and last significant tokens of
    /// `node` as written, for a node whose text is written as it was.
    pub(crate) fn within(&self, node: &SyntaxNode) {
        let mut significant = (node.descendants_with_tokens())
            .filter_map(|it| it.into_token())
            .filter(|token| !token.kind().is_trivia());
        let Some(first) = significant.next() else {
            return;
        };
        let last = significant.last().unwrap_or_else(|| first.clone());
        let range = offset(&first).start..offset(&last).end;
        let first = self
            .comments
            .partition_point(|comment| offset(&comment.token).start < range.start);
        let inside = self.comments[first..].iter();
        for comment in inside.take_while(|comment| offset(&comment.token).end <= range.end) {
            comment.take();
        }
    }

    /// A comment no rule wrote, if there is one.
    pub(crate) fn untaken(&self) -> Option<&SyntaxToken> {
        let comment = self.comments.iter().find(|comment| !comment.taken.get());
        comment.map(|comment| &comment.token)
    }

    fn take(&self, range: Option<&Range<usize>>) -> Doc {
        let comments = range.map_or(&[][..], |range| &self.comments[range.clone()]);
        Doc::concat(comments.iter().map(|comment| {
            comment.take();
            comment.doc()
        }))
    }
}

impl Comment {
    fn take(&self) {
        let twice = self.taken.replace(true);
        debug_assert!(!twice, "comment {:?} written twice", self.token);
    }

    /// The comment with the separation it had on either side: a space on the
    /// same line, a line break, or an empty line.
    fn doc(&self) -> Doc {
        let lines_after = match self.token.kind() {
            LINE_COMMENT => self.lines_after.max(1),
            _ => self.lines_after,
        };
        Doc::concat([
            separation(self.lines_before),
            Doc::token(self.token.text()),
            separation(lines_after),
        ])
    }
}

fn separation(lines: usize) -> Doc {
    match lines {
        0 => Doc::Space,
        1 => Doc::HardLine,
        _ => Doc::BlankLine,
    }
}

/// Places the comments of the gap between `prev` and `next`.
fn place(
    comments: &mut Vec<Comment>,
    source: &str,
    gap: Vec<(SyntaxToken, usize)>,
    prev: Option<&SyntaxToken>,
    next: Option<&SyntaxToken>,
) {
    let trailing = prev.and_then(|prev| largest(prev, next));
    let leading = next.and_then(|next| largest(next, prev));
    let upto = next.map_or(source.len(), |next| offset(next).start);
    let ends: Vec<usize> = (gap.iter().skip(1).map(|(token, _)| offset(token).start))
        .chain([upto])
        .collect();

    let mut on_prev_line = prev.is_some();
    for ((token, lines_before), next_start) in gap.into_iter().zip(ends) {
        on_prev_line &= lines_before == 0;
        let place = match (&trailing, &leading, prev) {
            (Some(node), _, _) if on_prev_line => Place::Trailing(node.clone()),
            (_, _, Some(prev)) if on_prev_line => Place::After(prev.clone()),
            (_, Some(node), _) => Place::Leading(node.clone()),
            (Some(node), None, _) => Place::Trailing(node.clone()),
            (None, None, Some(prev)) => Place::After(prev.clone()),
            (None, None, None) => Place::Head,
        };
        comments.push(Comment {
            lines_after: newlines(source, offset(&token).end, next_start),
            token,
            lines_before,
            place,
            taken: Cell::new(false),
        });
    }
}

/// The largest node below the root that holds `token` and not `other`, the
/// token across the gap: the largest that `token` starts or ends.
fn largest(token: &SyntaxToken, other: Option<&SyntaxToken>) -> Option<SyntaxNode> {
    let holds = |node: &SyntaxNode| {
        other.is_some_and(|other| node.text_range().contains_range(other.text_range()))
    };
    token
        .parent_ancestors()
        .take_while(|node| node.parent().is_some() && !holds(node))
        .last()
}

fn offset(token: &SyntaxToken) -> Range<usize> {
    offset_of(token.text_range())
}

fn offset_of(range: TextRange) -> Range<usize> {
    usize::from(range.start())..usize::from(range.end())
}

fn newlines(source: &str, from: usize, to: usize) -> usize {
    source[from..to]
        .bytes()
        .filter(|&byte| byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Layout, print};
    use svirig_parse::SyntaxTree;

    fn parse(source: &str) -> SyntaxTree {
        SyntaxTree::parse("test.sv", source.to_owned())
    }

    fn places(source: &str) -> Vec<String> {
        let tree = parse(source);
        let comments = Comments::new(tree.root(), tree.source());
        let place = |place: &Place| match place {
            Place::Leading(node) => format!("leads {:?}", node.kind()),
            Place::Trailing(node) => format!("trails {:?}", node.kind()),
            Place::After(token) => format!("after {}", token.text()),
            Place::Head => "head".to_owned(),
        };
        (comments.comments.iter())
            .map(|comment| format!("{}: {}", comment.token.text(), place(&comment.place)))
            .collect()
    }

    fn printed(doc: Doc) -> String {
        print(
            &doc,
            Layout {
                width: 100,
                indent: 2,
            },
        )
    }

    #[test]
    fn each_comment_goes_with_what_it_sits_beside() {
        let source = "\
// head

module m ( // after paren
  input a, // after comma
  // own line
  input b
); // after semi
  // before item
  assign x = a /* mid */ + b; // trailing
  // before end
endmodule
// tail
";
        assert_eq!(
            places(source),
            [
                "// head: leads MODULE_DECL",
                "// after paren: after (",
                "// after comma: after ,",
                "// own line: leads PORT",
                "// after semi: after ;",
                "// before item: leads CONTINUOUS_ASSIGN",
                "/* mid */: trails NAME_REF",
                "// trailing: trails CONTINUOUS_ASSIGN",
                "// before end: trails CONTINUOUS_ASSIGN",
                "// tail: trails MODULE_DECL",
            ]
        );
    }

    #[test]
    fn a_comment_after_one_on_its_own_line_is_on_its_own_line_too() {
        let source = "\
module m;
  assign a = b; /* same */ /* same */
  /* own */ /* after own */
  assign c = d;
endmodule
";
        assert_eq!(
            places(source),
            [
                "/* same */: trails CONTINUOUS_ASSIGN",
                "/* same */: trails CONTINUOUS_ASSIGN",
                "/* own */: leads CONTINUOUS_ASSIGN",
                "/* after own */: leads CONTINUOUS_ASSIGN",
            ]
        );
    }

    #[test]
    fn a_file_of_comments_alone_has_them_at_its_head() {
        let source = "// one\n/* two */\n";
        assert_eq!(places(source), ["// one: head", "/* two */: head"]);

        let tree = parse(source);
        let comments = Comments::new(tree.root(), tree.source());
        assert_eq!(printed(comments.head()), source);
    }

    #[test]
    fn comments_keep_their_lines_and_one_empty_line() {
        let tree = parse("// one\n\n\n// two\n\nmodule m; endmodule\n");
        let comments = Comments::new(tree.root(), tree.source());
        let module = tree.root().first_child().unwrap();
        assert_eq!(printed(comments.leading(&module)), "// one\n\n// two\n");
    }

    #[test]
    fn a_block_comment_keeps_its_later_lines_as_they_were() {
        let tree = parse("module m;\n  /* one\n       two */\nendmodule\n");
        let comments = Comments::new(tree.root(), tree.source());
        let semicolon = (tree.root().descendants_with_tokens())
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SEMICOLON)
            .unwrap();
        assert_eq!(
            printed(comments.after(&semicolon)),
            "/* one\n       two */\n"
        );
    }

    #[test]
    fn a_comment_no_rule_wrote_is_found() {
        let tree = parse("// lead\nmodule m; // inside\nendmodule // trail\n");
        let comments = Comments::new(tree.root(), tree.source());
        let module = tree.root().first_child().unwrap();
        let untaken = || comments.untaken().map(|token| token.text().to_owned());

        comments.leading(&module);
        assert_eq!(untaken().as_deref(), Some("// inside"));
        comments.within(&module);
        assert_eq!(untaken().as_deref(), Some("// trail"));
        assert_eq!(printed(comments.trailing(&module)), "// trail\n");
        assert_eq!(untaken(), None);
    }
}
