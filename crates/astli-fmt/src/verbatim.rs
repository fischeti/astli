//! A node no rule lays out, written as it was read.
//!
//! Its lines move together, so the node's own indentation survives a change
//! in where it stands: with the first line, or, for lines that hang left of
//! it, with the indentation of the line it starts on. A line that
//! starts inside a token stays where it is, because its leading whitespace is
//! part of the token: the later lines of a block comment, of a string
//! continued with `\`, and of a `` `define `` body, whose lines are `\`
//! continuations.
//!
//! Whitespace at the end of a line is dropped where it is whitespace between
//! tokens, and kept where it is inside one. So is the whitespace before a
//! `` `define ``'s `\`, if the printer can line them up: see [`movable`].

use std::ops::Range;

use astli_syntax::{SyntaxKind::*, SyntaxNode, SyntaxToken};

use crate::doc::{Verbatim, VerbatimLine};

/// Columns a tab advances to the next multiple of.
const TAB: u32 = 8;

/// The text of `node` from its first significant token to its last, and the
/// bytes of the input it covers, or `None` if it has no significant token.
///
/// A directive's text runs on to `until`, the end of the directive the node
/// ends in, if it ends in one: the comments on its line are its own.
pub(crate) fn verbatim(
    node: &SyntaxNode,
    source: &str,
    until: usize,
) -> Option<(Verbatim, Range<usize>)> {
    let tokens: Vec<_> = (node.descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .collect();
    let first = tokens.iter().position(|token| !token.kind().is_trivia())?;
    let last = tokens.iter().rposition(|token| !token.kind().is_trivia())?;
    let mut tokens = tokens[first..=last].to_vec();

    // Comments on a line a `\` continued, or before the end of a directive,
    // are a directive's text, wherever the tree put them.
    let mut next = tokens[tokens.len() - 1].next_token();
    let mut space = None;
    let inside = |token: &Option<SyntaxToken>| {
        token
            .as_ref()
            .is_some_and(|token| usize::from(token.text_range().end()) <= until)
    };
    while tokens[tokens.len() - 1].kind() == LINE_CONTINUATION || space.is_some() || inside(&next) {
        match next {
            Some(token) if token.kind() == WHITESPACE && !token.text().contains('\n') => {
                next = token.next_token();
                space = Some(token);
            }
            Some(token) if matches!(token.kind(), LINE_COMMENT | BLOCK_COMMENT) => {
                next = token.next_token();
                tokens.extend(space.take());
                tokens.push(token);
            }
            _ => break,
        }
    }

    let start = usize::from(tokens[0].text_range().start());
    let end = usize::from(tokens[tokens.len() - 1].text_range().end());
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let before = &source[line_start..start];
    let indent = columns(&before[..before.len() - before.trim_start().len()]);
    let mut lines = Lines {
        column: columns(before),
        first: None,
        rest: Vec::new(),
        continued: Vec::new(),
        line: String::new(),
        start: Start::First,
    };

    let movable = movable(&tokens);
    for (at, token) in tokens.iter().enumerate() {
        let text = token.text();
        if movable.binary_search(&(at + 1)).is_ok() && token.kind() == WHITESPACE {
            continue;
        }
        if movable.binary_search(&at).is_ok() {
            lines
                .continued
                .push(lines.first.as_ref().map_or(0, |_| 1 + lines.rest.len()));
        }
        if !text.contains('\n') {
            lines.line.push_str(text);
        } else if token.kind() == WHITESPACE {
            // What comes before the first newline ends a line, and is dropped.
            for indent in text.split('\n').skip(1) {
                lines.end(Start::Moved(columns(indent)));
            }
        } else {
            let mut parts = text.split('\n');
            lines.line.push_str(parts.next().unwrap_or_default());
            for part in parts {
                lines.end(Start::Kept);
                lines.line.push_str(part);
            }
        }
    }
    lines.end(Start::First);

    let verbatim = Verbatim {
        column: lines.column,
        indent,
        first: lines.first.unwrap_or_default(),
        rest: lines.rest,
        continued: lines.continued,
    };
    Some((verbatim, start..end))
}

struct Lines {
    column: u32,
    first: Option<String>,
    rest: Vec<VerbatimLine>,
    continued: Vec<usize>,
    /// The line being read, from after its leading whitespace if it is moved.
    line: String,
    start: Start,
}

/// Where a line starts.
enum Start {
    First,
    /// Between tokens, `indent` columns in.
    Moved(u32),
    /// Inside a token.
    Kept,
}

impl Lines {
    /// Ends the line being read. The next starts as `next` says.
    fn end(&mut self, next: Start) {
        let text = std::mem::take(&mut self.line);
        match std::mem::replace(&mut self.start, next) {
            Start::First => self.first = Some(text),
            Start::Moved(indent) => self.rest.push(VerbatimLine::Moved { indent, text }),
            Start::Kept => self.rest.push(VerbatimLine::Kept(text)),
        }
    }
}

/// The indices in `tokens` of the `\`s the printer may line up: every `\`
/// of each `` `define `` whose `\`s can all move. Whitespace between tokens
/// in a body is only observable inside `` `"…`" ``, or after a ``` `` ```,
/// where it decides whether tokens paste. A `\` right after a line comment
/// cannot move either, since the comment would take in the space before it.
/// A `` `define `` any of whose `\`s cannot move is left as it was, so that
/// its `\`s stay lined up however they were. In order.
fn movable(tokens: &[SyntaxToken]) -> Vec<usize> {
    let mut movable = Vec::new();
    // The `\`s of the `` `define `` being read, whether they can all move,
    // and whether a `` `" `` is open.
    let mut define: Option<(Vec<usize>, bool, bool)> = None;
    for (at, token) in tokens.iter().enumerate() {
        match token.kind() {
            TICK_IDENT if token.text() == "`define" => define = Some((Vec::new(), true, false)),
            WHITESPACE if token.text().contains('\n') => {
                if let Some((continuations, true, _)) = define.take() {
                    movable.extend(continuations);
                }
            }
            MACRO_QUOTE => {
                if let Some((_, _, quoted)) = &mut define {
                    *quoted = !*quoted;
                }
            }
            LINE_CONTINUATION => {
                if let Some((continuations, all, quoted)) = &mut define {
                    let before = at.checked_sub(1).map(|at| tokens[at].kind());
                    *all &= !*quoted && !matches!(before, Some(LINE_COMMENT | MACRO_PASTE));
                    continuations.push(at);
                }
            }
            _ => {}
        }
    }
    if let Some((continuations, true, _)) = define {
        movable.extend(continuations);
    }
    movable
}

/// The column `text` ends at, starting from the first.
pub(crate) fn columns(text: &str) -> u32 {
    text.chars().fold(0, |column, char| match char {
        '\t' => (column / TAB + 1) * TAB,
        _ => column + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use astli_parse::SyntaxTree;

    /// The lines of the first node of `kind`, marked by how each starts.
    fn lines(source: &str, kind: astli_syntax::SyntaxKind) -> Vec<String> {
        let tree = SyntaxTree::parse("test.sv", source.to_owned());
        let node = (tree.root().descendants())
            .find(|node| node.kind() == kind)
            .unwrap();
        let (verbatim, _) = verbatim(&node, tree.source(), 0).unwrap();
        let rest = verbatim.rest.iter().map(|line| match line {
            VerbatimLine::Moved { indent, text } => format!("{indent}|{text}"),
            VerbatimLine::Kept(text) => format!("kept|{text}"),
        });
        [format!("{}|{}", verbatim.column, verbatim.first)]
            .into_iter()
            .chain(rest)
            .collect()
    }

    #[test]
    fn lines_between_tokens_move_and_lines_inside_one_stay() {
        let source = concat!(
            "module m;\n",
            "    initial begin  \n",
            "      /* one\n",
            "   two */ a = 1;\n",
            "\n",
            "\t  b = \"x\\\n",
            " y\";\n",
            "    end\n",
            "endmodule\n",
        );
        assert_eq!(
            lines(source, PROCEDURAL_BLOCK),
            [
                "4|initial begin",
                "6|/* one",
                "kept|   two */ a = 1;",
                "0|",
                "10|b = \"x\\",
                "kept| y\";",
                "4|end",
            ]
        );
    }

    /// The printer puts back the space before each `\`.
    #[test]
    fn a_define_body_stays_where_it_is() {
        let source = "  `define A(x) \\\n    x; \\\n  x\n";
        assert_eq!(
            lines(source, DIRECTIVE),
            ["2|`define A(x)\\", "kept|    x;\\", "kept|  x"]
        );
    }

    #[test]
    fn a_comment_on_a_continued_line_is_written_with_the_directive() {
        let source = "`define A \\\n   // tail\n// next\n";
        assert_eq!(
            lines(source, DIRECTIVE),
            ["0|`define A\\", "kept|   // tail"]
        );
    }

    #[test]
    fn a_node_of_one_line_has_no_rest() {
        assert_eq!(
            lines("assign a = b;  \n", CONTINUOUS_ASSIGN),
            ["0|assign a = b;"]
        );
    }
}
