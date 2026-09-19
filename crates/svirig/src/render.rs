//! Turning what a stage produced into lines on a terminal.
//!
//! Presentation lives here rather than in the crates, for the same reason
//! `svirig-text` holds a diagnostic and not its rendering: the tree is a data
//! structure, and how wide a column is or where a comment is cut short is the
//! driver's opinion about a terminal.

use std::io::{self, BufWriter, IsTerminal, Write};
use std::sync::LazyLock;
use std::time::Duration;

use rowan::NodeOrToken;
use svirig_diag::{Sources, Style, resolve_all, write as write_diagnostic};
use svirig_syntax::SyntaxNode;
use svirig_text::{Diagnostic, Origins};

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

/// A duration at a glance. `Debug` prints every digit it has, which over a
/// run of any size is nine figures of noise in a line meant to be compared
/// with the last one.
pub fn duration(of: Duration) -> String {
    match of.as_secs_f64() {
        secs if secs >= 1.0 => format!("{secs:.2}s"),
        secs if secs >= 1e-3 => format!("{:.1}ms", secs * 1e3),
        secs => format!("{:.0}\u{b5}s", secs * 1e6),
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
pub fn tree(out: &mut dyn Write, node: &SyntaxNode, depth: usize) -> Result {
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

/// How many of one file's diagnostics are worth printing.
///
/// A file missing its include path can be wrong on every other line, and three
/// hundred copies of the same complaint is not more informative than twenty --
/// it only buries whatever else the run found. The count of what was left out
/// is printed, so nothing is silently dropped.
const SHOWN: usize = 20;

/// Whether stderr is a terminal, asked once.
///
/// Colour is for a person; a pipe gets the escapes as text and a log file keeps
/// them forever. The check is a syscall, and a run over a filelist would
/// otherwise make one per file.
static STYLE: LazyLock<Style> = LazyLock::new(|| match io::stderr().is_terminal() {
    true => Style::default(),
    false => Style::plain(),
});

/// Writes what a file's diagnostics say, and returns how many were errors.
///
/// Ordered and capped here rather than in `svirig-diag`, because how much is
/// worth printing is an opinion about a terminal and the crates do not hold
/// those. `resolve_all` has already put them in the order they should be read,
/// so the cap takes the front of the file rather than whatever came out first.
pub fn diagnostics(
    to: &mut dyn Write,
    origins: &Origins,
    diagnostics: &[Diagnostic],
) -> Result<usize> {
    if diagnostics.is_empty() {
        return Ok(0);
    }

    let resolved = resolve_all(origins, diagnostics);
    let errors = resolved
        .iter()
        .filter(|one| one.diagnostic.is_error())
        .count();

    let mut sources = Sources::new(origins);
    for one in resolved.iter().take(SHOWN) {
        write_diagnostic(to, &mut sources, one, *STYLE)?;
    }
    if let Some(hidden) = resolved.len().checked_sub(SHOWN).filter(|left| *left > 0) {
        writeln!(to, "... and {hidden} more")?;
    }

    Ok(errors)
}
