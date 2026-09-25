//! Where each comment is written: before a node, after one, or after a token.
//!
//! The tree builder parks trivia by a rule that suits a round trip, not a
//! layout, so the formatter places comments again. A comment may never cross
//! a token, or the transparency check refuses, so it is placed within the gap
//! it sits in, between the significant tokens `prev` and `next`:
//!
//! - on `prev`'s line with `next` on it too, a block comment leads the
//!   largest node that starts at `next`, since it labels what follows it:
//!   `f(a, /* width */ 8)`;
//! - otherwise on `prev`'s line, it trails the largest node that ends at
//!   `prev`, or follows `prev` itself if no node ends there;
//! - on a line of its own, it leads the largest node that starts at `next`;
//!   failing that it trails the node that ends at `prev`, as a comment before
//!   `end` belongs with the last statement; failing that it follows `prev`.
//!
//! "Largest" stops below the node that holds both tokens, and below the root.
//! A line comment right below one at the end of a line, starting in the same
//! column, continues it: it goes where that one goes, and keeps its column.
//! A comment at the end of a line lines up with the others of its table.
//! A comment on a line continued from a directive's is the directive's text,
//! so it is written with the directive and placed nowhere.
//! Each comment is taken once by the rule that writes it, and [`Comments::
//! untaken`] finds one that no rule wrote.

use std::cell::Cell;
use std::ops::Range;

use astli_syntax::{SyntaxKind::*, SyntaxNode, SyntaxToken};
use rowan::TextRange;
use rustc_hash::FxHashMap;

use crate::align::COMMENT;
use crate::doc::{Doc, Verbatim, VerbatimLine};

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
    /// Whether a closer follows it on its line, against it.
    closes: bool,
    /// The column it starts at in the input.
    column: u32,
    /// Whether it continues the comment before it.
    continues: bool,
    place: Place,
    taken: Cell<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Leading(SyntaxNode),
    Trailing(SyntaxNode),
    After(SyntaxToken),
    Head,
    /// On a line a `\\` continued, inside a directive.
    Inside,
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
                Place::Inside => {}
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

    /// Marks the comments in the bytes `range` of the input as written, for
    /// text written as it was.
    pub(crate) fn within(&self, range: Range<usize>) {
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
        let mut docs = Vec::new();
        for run in comments.chunk_by(|_, next| next.continues) {
            for comment in run {
                comment.take();
            }
            docs.push(doc(run));
        }
        Doc::concat(docs)
    }
}

impl Comment {
    fn take(&self) {
        let twice = self.taken.replace(true);
        debug_assert!(!twice, "comment {:?} written twice", self.token);
    }
}

/// A comment and those that continue it, with the separation they had on
/// either side: a space on the same line, a line break, or an empty line. The
/// later ones keep their columns relative to the first, as a verbatim run.
fn doc(run: &[Comment]) -> Doc {
    let (first, last) = (&run[0], &run[run.len() - 1]);
    let lines_after = match last.token.kind() {
        LINE_COMMENT => last.lines_after.max(1),
        _ => last.lines_after,
    };
    let text = match run {
        [comment] => Doc::token(comment.token.text()),
        _ => Doc::Verbatim(Verbatim {
            column: first.column,
            // Every line starts in the first's column, so none hangs and
            // this is not read.
            indent: first.column,
            first: first.token.text().to_owned(),
            rest: (run[1..].iter())
                .map(|comment| VerbatimLine::Moved {
                    indent: comment.column,
                    text: comment.token.text().to_owned(),
                })
                .collect(),
        }),
    };
    let ends_line = first.lines_before == 0 && lines_after > 0 && first.place != Place::Head;
    // One that leads a node from the line before it is separated from what
    // comes before as the node would be, by the rule that writes both.
    let before = match (first.lines_before, &first.place) {
        (0, Place::Leading(_)) => Doc::nil(),
        (lines, _) => separation(lines),
    };
    Doc::concat([
        before,
        if ends_line {
            Doc::Cell(COMMENT)
        } else {
            Doc::nil()
        },
        text,
        match last.closes {
            true => Doc::nil(),
            false => separation(lines_after),
        },
    ])
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
    if gap.is_empty() {
        return;
    }
    let trailing = prev.and_then(|prev| largest(prev, next));
    let leading = next.and_then(|next| largest(next, prev));
    let upto = next.map_or(source.len(), |next| offset(next).start);
    let ends: Vec<usize> = (gap.iter().skip(1).map(|(token, _)| offset(token).start))
        .chain([upto])
        .collect();

    // A block comment right before a closer keeps it on its line, wherever
    // the input had it: a closer put on a line of its own, as a broken list
    // does, would otherwise hold it there when formatted again.
    let closes = next.is_some_and(|next| matches!(next.kind(), R_PAREN | R_BRACK | R_BRACE));
    let mut on_prev_line = prev.is_some();
    // The column and place of the comment before, if it ends a line of code
    // or continues one that does.
    let mut above: Option<(u32, Place)> = None;
    for ((token, lines_before), next_start) in gap.into_iter().zip(ends) {
        on_prev_line &= lines_before == 0;
        let column = column(source, offset(&token).start);
        let continues = above.as_ref().filter(|(above, _)| {
            lines_before == 1 && token.kind() == LINE_COMMENT && *above == column
        });
        if let Some((_, place)) = continues {
            let place = place.clone();
            comments.push(Comment {
                lines_after: newlines(source, offset(&token).end, next_start),
                closes: false,
                token,
                lines_before,
                column,
                continues: true,
                place,
                taken: Cell::new(false),
            });
            continue;
        }
        let labels =
            token.kind() == BLOCK_COMMENT && newlines(source, offset(&token).end, upto) == 0;
        let place = match (&trailing, &leading, prev) {
            (_, _, Some(prev)) if on_prev_line && prev.kind() == LINE_CONTINUATION => Place::Inside,
            (_, Some(node), _) if on_prev_line && labels => Place::Leading(node.clone()),
            (Some(node), _, _) if on_prev_line => Place::Trailing(node.clone()),
            (_, _, Some(prev)) if on_prev_line => Place::After(prev.clone()),
            (_, Some(node), _) => Place::Leading(node.clone()),
            (Some(node), None, _) => Place::Trailing(node.clone()),
            (None, None, Some(prev)) => Place::After(prev.clone()),
            (None, None, None) => Place::Head,
        };
        above = (on_prev_line && token.kind() == LINE_COMMENT && place != Place::Inside)
            .then(|| (column, place.clone()));
        let closes = closes && next_start == upto && token.kind() == BLOCK_COMMENT;
        let lines_after = match closes {
            true => 0,
            false => newlines(source, offset(&token).end, next_start),
        };
        comments.push(Comment {
            lines_after,
            closes,
            token,
            lines_before,
            column,
            continues: false,
            place,
            taken: Cell::new(false),
        });
    }
}

/// The column `at` is in, counting a character as one.
fn column(source: &str, at: usize) -> u32 {
    let start = source[..at].rfind('\n').map_or(0, |newline| newline + 1);
    source[start..at].chars().count() as u32
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
    use astli_parse::SyntaxTree;

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
            Place::Inside => "inside".to_owned(),
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
    fn a_comment_below_one_at_the_end_of_a_line_continues_it() {
        let source = "\
module m;
  logic a; // one
           // two
  // three
  logic b;
endmodule
";
        assert_eq!(
            places(source),
            [
                "// one: trails VAR_DECL",
                "// two: trails VAR_DECL",
                "// three: leads VAR_DECL",
            ]
        );
    }

    #[test]
    fn a_comment_on_a_continued_line_is_inside_the_directive() {
        let source = "`define A \\\n  // tail\n// next\nmodule m; endmodule\n";
        assert_eq!(
            places(source),
            ["// tail: inside", "// next: leads MODULE_DECL"]
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
        let source = tree.source();
        let end = source.rfind("endmodule").unwrap() + "endmodule".len();
        comments.within(source.find("module").unwrap()..end);
        assert_eq!(untaken().as_deref(), Some("// trail"));
        assert_eq!(printed(comments.trailing(&module)), "// trail\n");
        assert_eq!(untaken(), None);
    }
}
