//! Turning what a stage produced into lines on a terminal.
//!
//! Presentation lives here rather than in the crates, for the same reason
//! `svirig-text` holds a diagnostic and not its rendering: the tree is a data
//! structure, and how wide a column is or where a comment is cut short is the
//! driver's opinion about a terminal.

use std::io::{self, BufWriter, Write};

use rowan::NodeOrToken;
use svirig_syntax::SyntaxNode;

use crate::error::Result;

/// Longer texts are cut short; one block comment is not worth a screen.
const MAX_TEXT: usize = 60;

/// Where everything a command prints goes.
///
/// One buffered handle rather than `println!`, which takes the lock and
/// flushes on every line: the largest file in the corpus is 300k tokens, and
/// a dump of it is 300k lines. The other half is the pipe. `println!` panics
/// when the reader goes away, so `svirig parse big.sv | head` ends in a
/// backtrace; a write returns the error instead, and [`Error::Output`] is
/// where `main` decides that a closed pipe is not a failure.
///
/// [`Error::Output`]: crate::error::Error::Output
pub struct Out(BufWriter<io::Stdout>);

impl Out {
    pub fn new() -> Out {
        Out(BufWriter::new(io::stdout()))
    }
}

impl Write for Out {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Debug-quoted, so that whitespace is visible rather than printed.
pub fn elide(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return format!("{text:?}");
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{:?}...", &text[..end])
}

/// On one line. A macro argument is read for its shape rather than its bytes,
/// and a wrapped argument list is the common case in macro-heavy code, so the
/// newlines go.
pub fn flat(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX_TEXT) {
        Some((at, _)) => format!("{}...", &flat[..at]),
        None => flat,
    }
}

/// The tree, indented, one node or token to a line.
pub fn tree(out: &mut Out, node: &SyntaxNode, depth: usize) -> Result {
    let indent = "  ".repeat(depth);
    writeln!(out, "{indent}{:?}@{:?}", node.kind(), node.text_range())?;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(node) => tree(out, &node, depth + 1)?,
            NodeOrToken::Token(token) => writeln!(
                out,
                "{indent}  {:?}@{:?} {}",
                token.kind(),
                token.text_range(),
                elide(token.text())
            )?,
        }
    }
    Ok(())
}

/// How many nodes and how many leaves the tree holds.
pub fn count(node: &SyntaxNode) -> (usize, usize) {
    let mut nodes = 1;
    let mut leaves = 0;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(child) => {
                let (n, l) = count(&child);
                nodes += n;
                leaves += l;
            }
            NodeOrToken::Token(_) => leaves += 1,
        }
    }
    (nodes, leaves)
}
