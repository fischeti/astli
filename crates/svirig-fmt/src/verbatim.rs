//! A node no rule lays out, written as it was read.
//!
//! Its lines move together: each shifts by as much as the first, so the
//! node's own indentation survives a change in where it stands. A line that
//! starts inside a token stays where it is, because its leading whitespace is
//! part of the token: the later lines of a block comment, of a string
//! continued with `\`, and of a `` `define `` body, whose lines are `\`
//! continuations.
//!
//! Whitespace at the end of a line is dropped where it is whitespace between
//! tokens, and kept where it is inside one.

use svirig_syntax::{SyntaxKind::*, SyntaxNode};

use crate::doc::{Verbatim, VerbatimLine};

/// Columns a tab advances to the next multiple of.
const TAB: u32 = 8;

/// The text of `node` from its first significant token to its last, or `None`
/// if it has none.
pub(crate) fn verbatim(node: &SyntaxNode, source: &str) -> Option<Verbatim> {
    let tokens: Vec<_> = (node.descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .collect();
    let first = tokens.iter().position(|token| !token.kind().is_trivia())?;
    let last = tokens.iter().rposition(|token| !token.kind().is_trivia())?;

    let start = usize::from(tokens[first].text_range().start());
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let mut lines = Lines {
        column: columns(&source[line_start..start]),
        first: None,
        rest: Vec::new(),
        line: String::new(),
        start: Start::First,
    };

    for token in &tokens[first..=last] {
        let text = token.text();
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

    Some(Verbatim {
        column: lines.column,
        first: lines.first.unwrap_or_default(),
        rest: lines.rest,
    })
}

struct Lines {
    column: u32,
    first: Option<String>,
    rest: Vec<VerbatimLine>,
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

/// The column `text` ends at, starting from the first.
fn columns(text: &str) -> u32 {
    text.chars().fold(0, |column, char| match char {
        '\t' => (column / TAB + 1) * TAB,
        _ => column + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use svirig_parse::SyntaxTree;

    /// The lines of the first node of `kind`, marked by how each starts.
    fn lines(source: &str, kind: svirig_syntax::SyntaxKind) -> Vec<String> {
        let tree = SyntaxTree::parse("test.sv", source.to_owned());
        let node = (tree.root().descendants())
            .find(|node| node.kind() == kind)
            .unwrap();
        let verbatim = verbatim(&node, tree.source()).unwrap();
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

    #[test]
    fn a_define_body_stays_where_it_is() {
        let source = "  `define A(x) \\\n    x; \\\n  x\n";
        assert_eq!(
            lines(source, DIRECTIVE),
            ["2|`define A(x) \\", "kept|    x; \\", "kept|  x"]
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
