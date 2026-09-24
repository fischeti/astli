//! Terminal output rendering and formatting helpers.

use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use astli_diag::{Sources, Style, resolve_all, write as write_diagnostic};
use astli_syntax::SyntaxNode;
use astli_text::{Diagnostic, Origins};
use rowan::NodeOrToken;
use similar::udiff::UnifiedHunkHeader;
use similar::{ChangeTag, TextDiff};

use crate::error::Result;

/// Maximum character length before eliding token or macro text in debug dumps.
const MAX_TEXT: usize = 60;

/// Maximum number of diagnostics to display per file before eliding remaining errors.
const SHOWN: usize = 20;

/// Buffered standard output writer.
pub struct Out(BufWriter<io::Stdout>);

impl Out {
    /// Creates a new buffered stdout writer.
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

/// Formats an elapsed duration into a human-readable string (`s`, `ms`, or `µs`).
pub fn duration(of: Duration) -> String {
    match of.as_secs_f64() {
        secs if secs >= 1.0 => format!("{secs:.2}s"),
        secs if secs >= 1e-3 => format!("{:.1}ms", secs * 1e3),
        secs => format!("{:.0}\u{b5}s", secs * 1e6),
    }
}

/// Debug-quotes text, truncating to [`MAX_TEXT`] characters if longer.
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

/// Normalizes whitespace into single spaces and truncates to [`MAX_TEXT`] characters.
pub fn flat(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX_TEXT) {
        Some((at, _)) => format!("{}...", &flat[..at]),
        None => flat,
    }
}

/// Prints an indented representation of `node` and its descendants to `out`.
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

/// Counts the total number of syntax nodes and leaf tokens in `node`.
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

/// Lines of unchanged context around each hunk of a diff.
const CONTEXT: usize = 3;

/// Whether standard output takes colour. Piped, a diff stays a patch that
/// `git apply` or a pager like `delta` can read.
static COLOUR: LazyLock<bool> = LazyLock::new(|| io::stdout().is_terminal());

/// Writes the unified diff that turns `old` into `new`, both named `path`.
pub fn diff(out: &mut dyn Write, path: &Path, old: &str, new: &str) -> Result {
    let paint = |code| if *COLOUR { code } else { "" };
    let (bold, cyan, red, green, reset) = (
        paint("\x1b[1m"),
        paint("\x1b[36m"),
        paint("\x1b[31m"),
        paint("\x1b[32m"),
        paint("\x1b[0m"),
    );

    let diff = TextDiff::from_lines(old, new);
    let path = path.display();
    writeln!(out, "{bold}--- {path}{reset}")?;
    writeln!(out, "{bold}+++ {path}{reset}")?;
    for hunk in diff.grouped_ops(CONTEXT) {
        writeln!(out, "{cyan}{}{reset}", UnifiedHunkHeader::new(&hunk))?;
        for change in hunk.iter().flat_map(|op| diff.iter_changes(op)) {
            let (sign, colour) = match change.tag() {
                ChangeTag::Equal => (' ', ""),
                ChangeTag::Delete => ('-', red),
                ChangeTag::Insert => ('+', green),
            };
            // The reset goes before the newline, so that no colour bleeds into
            // the next line if the output is cut short.
            let line = change.value().strip_suffix('\n').unwrap_or(change.value());
            writeln!(out, "{colour}{sign}{line}{reset}")?;
            if change.missing_newline() {
                writeln!(out, "\\ No newline at end of file")?;
            }
        }
    }
    Ok(())
}

/// Cached terminal styling for standard error.
static STYLE: LazyLock<Style> = LazyLock::new(|| match io::stderr().is_terminal() {
    true => Style::default(),
    false => Style::plain(),
});

/// Formats and prints diagnostics to `to`, returning the count of errors encountered.
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
