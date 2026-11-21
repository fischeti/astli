//! Terminal output rendering and formatting helpers.

use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use anstyle::AnsiColor;
use astli_diag::{Resolved, Sources, resolve_all, write as write_diagnostic};
use astli_syntax::SyntaxNode;
use astli_text::{Diagnostic, Origins, Severity};
use rayon::prelude::*;
use rowan::NodeOrToken;
use similar::udiff::UnifiedHunkHeader;
use similar::{ChangeTag, TextDiff};

use crate::cli::DiagnosticFormat;
use crate::error::Result;
use crate::{formats, term};

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

/// What was rendered of a file, for stderr, and how many errors it holds.
///
/// Under `--diag gitlab` it holds the findings instead, kept for the report
/// only once told, so that the report holds what the run tells, in the
/// order it tells it, whichever thread rendered what first.
#[derive(Default)]
pub struct Report {
    pub said: Vec<u8>,
    pub errors: usize,
    found: Vec<formats::Finding>,
}

impl Report {
    /// `found`, whose spans `origins` resolves, rendered.
    pub fn of(origins: &Origins, found: &[Diagnostic]) -> Result<Report> {
        let mut report = Report::default();
        report.render(origins, found)?;
        Ok(report)
    }

    /// Renders `found`, whose spans `origins` resolves, after what the
    /// report holds.
    pub fn render(&mut self, origins: &Origins, found: &[Diagnostic]) -> Result {
        let resolved = (resolve_all(origins, found).into_iter())
            .map(|one| (0, one))
            .collect();
        self.written(&mut [Sources::new(origins)], resolved)
    }

    /// Adds what `other` holds after what the report does.
    pub fn add(&mut self, other: Report) {
        self.said.extend(other.said);
        self.errors += other.errors;
        self.found.extend(other.found);
    }

    /// Tells what the report holds: writes it to stderr, or keeps its
    /// findings for the report written at the end.
    pub fn tell(&self) -> io::Result<()> {
        formats::keep(&self.found);
        match self.said.is_empty() {
            true => Ok(()),
            false => term::write(&self.said),
        }
    }

    /// What was found in `file`, rendered as one list, each set of
    /// diagnostics resolved against its own origins.
    ///
    /// Those reported in `file` come first, in the order of its lines, as
    /// written, whichever set they are in; then the rest, such as those in a
    /// header it includes, set by set.
    pub fn findings(file: &Path, sets: &[(&Origins, &[Diagnostic])]) -> Result<Report> {
        let mut resolved: Vec<(usize, Resolved)> = (sets.iter().enumerate())
            .flat_map(|(at, (origins, found))| {
                (resolve_all(origins, found).into_iter()).map(move |one| (at, one))
            })
            .collect();
        resolved.sort_by_key(
            |(at, one)| match sets[*at].0.path(one.at.src_id) == Some(file) {
                true => (false, one.at.start),
                false => (true, 0),
            },
        );
        let mut sources: Vec<Sources> = sets
            .iter()
            .map(|(origins, _)| Sources::new(origins))
            .collect();
        let mut report = Report::default();
        report.written(&mut sources, resolved)?;
        Ok(report)
    }

    /// Renders each of `resolved`, which the set of `sources` it is in
    /// resolves, after what the report holds, and counts its errors.
    fn written(&mut self, sources: &mut [Sources], resolved: Vec<(usize, Resolved)>) -> Result {
        if resolved.is_empty() {
            return Ok(());
        }

        let errors = resolved
            .iter()
            .filter(|(_, one)| one.diagnostic.is_error())
            .count();
        let warnings = resolved
            .iter()
            .filter(|(_, one)| one.diagnostic.severity == Severity::Warning)
            .count();
        self.errors += errors;
        let to = &mut self.said;

        // A tool reading lines wants every one, and nothing between them.
        match FORMAT.get() {
            Some(DiagnosticFormat::Short) => {
                for (_, one) in &resolved {
                    formats::short(to, one)?;
                }
                return Ok(());
            }
            Some(DiagnosticFormat::Github) => {
                for (_, one) in &resolved {
                    formats::github(to, one)?;
                }
                return Ok(());
            }
            Some(DiagnosticFormat::Json) => {
                for (at, one) in &resolved {
                    writeln!(to, "{}", formats::line(sources[*at].origins(), one))?;
                }
                return Ok(());
            }
            Some(DiagnosticFormat::Gitlab) => {
                (self.found).extend(resolved.iter().map(|(_, one)| formats::finding(one)));
                return Ok(());
            }
            Some(DiagnosticFormat::Pretty) | None => {}
        }

        for (index, (at, one)) in resolved.iter().take(SHOWN).enumerate() {
            // A blank line keeps one report's frame from running into the next.
            if index > 0 {
                writeln!(to)?;
            }
            write_diagnostic(to, &mut sources[*at], one, term::style())?;
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

        Ok(())
    }
}

/// Makes a report of each of `files` with `report`, in parallel, in order.
pub fn reports<T: Send>(
    files: Vec<T>,
    report: impl Fn(T) -> Result<Report> + Sync + Send,
) -> Result<Vec<Report>> {
    files.into_par_iter().map(report).collect()
}

/// `count` of `noun`, as in "1 error" or "3 errors".
pub fn plural(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        _ => format!("{count} {noun}s"),
    }
}

/// "has" or "have", agreeing with `count` of a noun.
pub fn have(count: usize) -> &'static str {
    if count == 1 { "has" } else { "have" }
}
