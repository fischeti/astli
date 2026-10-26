//! Terminal output rendering and formatting helpers.

use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use anstyle::AnsiColor;
use astli_diag::{Sources, resolve_all, write as write_diagnostic, write_github, write_short};
use astli_syntax::SyntaxNode;
use astli_text::{Diagnostic, Origins, Severity};
use rowan::NodeOrToken;
use similar::udiff::UnifiedHunkHeader;
use similar::{ChangeTag, TextDiff};

use crate::cli::DiagnosticFormat;
use crate::error::Result;
use crate::{json, term};

/// Maximum character length before eliding token or macro text in debug dumps.
const MAX_TEXT: usize = 60;

/// Maximum number of diagnostics to display per file before eliding remaining errors.
const SHOWN: usize = 20;

/// Buffered standard output writer.
pub type Out = BufWriter<io::Stdout>;

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

/// Writes the unified diff that turns `old` into `new`, both named `path`.
pub fn diff(out: &mut dyn Write, path: &Path, old: &str, new: &str) -> Result {
    let colour = term::stdout_colour();
    let paint = |style| if colour { style } else { anstyle::Style::new() };
    let colored = |color: AnsiColor| paint(color.on_default());
    let (bold, cyan, red, green) = (
        paint(anstyle::Style::new().bold()),
        colored(AnsiColor::Cyan),
        colored(AnsiColor::Red),
        colored(AnsiColor::Green),
    );

    let diff = TextDiff::from_lines(old, new);
    let path = path.display();
    writeln!(out, "{bold}--- {path}{bold:#}")?;
    writeln!(out, "{bold}+++ {path}{bold:#}")?;
    for hunk in diff.grouped_ops(CONTEXT) {
        writeln!(out, "{cyan}{}{cyan:#}", UnifiedHunkHeader::new(&hunk))?;
        for change in hunk.iter().flat_map(|op| diff.iter_changes(op)) {
            let (sign, colour) = match change.tag() {
                ChangeTag::Equal => (' ', anstyle::Style::new()),
                ChangeTag::Delete => ('-', red),
                ChangeTag::Insert => ('+', green),
            };
            // The reset goes before the newline, so that no colour bleeds into
            // the next line if the output is cut short.
            let line = change.value().strip_suffix('\n').unwrap_or(change.value());
            writeln!(out, "{colour}{sign}{line}{colour:#}")?;
            if change.missing_newline() {
                writeln!(out, "\\ No newline at end of file")?;
            }
        }
    }
    Ok(())
}

/// The shape diagnostics are written in, set once from the command line.
///
/// Global rather than passed along, since every stage reports diagnostics and
/// the choice is the same for all of them.
pub static FORMAT: OnceLock<DiagnosticFormat> = OnceLock::new();

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
    let warnings = resolved
        .iter()
        .filter(|one| one.diagnostic.severity == Severity::Warning)
        .count();

    let mut sources = Sources::new(origins);
    // A tool reading lines wants every one, and nothing between them.
    match FORMAT.get() {
        Some(DiagnosticFormat::Short) => {
            for one in &resolved {
                write_short(to, &sources, one)?;
            }
            return Ok(errors);
        }
        Some(DiagnosticFormat::Github) => {
            for one in &resolved {
                write_github(to, &sources, one)?;
            }
            return Ok(errors);
        }
        Some(DiagnosticFormat::Json) => {
            for one in &resolved {
                writeln!(to, "{}", json::line(&sources, one))?;
            }
            return Ok(errors);
        }
        Some(DiagnosticFormat::Gitlab) => {
            for one in &resolved {
                json::find(&sources, one);
            }
            return Ok(errors);
        }
        Some(DiagnosticFormat::Pretty) | None => {}
    }

    for (index, one) in resolved.iter().take(SHOWN).enumerate() {
        // A blank line keeps one report's frame from running into the next.
        if index > 0 {
            writeln!(to)?;
        }
        write_diagnostic(to, &mut sources, one, term::style())?;
    }

    // One report counts itself; past that, a tally saves counting frames.
    if resolved.len() > 1 {
        let mut tally = [(errors, "error"), (warnings, "warning")]
            .into_iter()
            .filter(|(count, _)| *count > 0)
            .map(|(count, noun)| plural(count, noun))
            .collect::<Vec<_>>()
            .join(", ");
        if tally.is_empty() {
            tally = plural(resolved.len(), "note");
        }
        if resolved.len() > SHOWN {
            tally.push_str(&format!("; the first {SHOWN} shown"));
        }
        // Written to stderr, which strips the style where it is not wanted.
        let bold = anstyle::Style::new().bold();
        writeln!(to, "\n{bold}{tally}{bold:#}")?;
    }

    Ok(errors)
}

/// `count` of `noun`, as in "1 error" or "3 errors".
pub fn plural(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        _ => format!("{count} {noun}s"),
    }
}
