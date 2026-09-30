//! Every case under `tests/data/`, expanded and parsed, and compared with
//! the snapshot beside it: `lower/*.sv` lowered, its HIR written out in a
//! `.hir`; `resolve/*.sv` resolved, each name it uses and what it refers to
//! in a `.names`; `check/*.sv` checked, each error in a `.check`.
//!
//! A case must parse whole, so that sema is tested on the tree it is for,
//! except one named `unparsed*.sv`, for what the parser keeps as written;
//! and must expand without a problem, except one named `missing*.sv`, for an
//! `` `include `` not found.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-sema snapshots
//! ```
//!
//! which writes every snapshot that is missing or differs. The diff is the
//! review.

use std::fmt::Write;
use std::panic;
use std::path::{Path, PathBuf};

use astli_parse::{Parsed, parse_expanded};
use astli_preproc::Session;
use astli_sema::{Design, Resolution, SymbolKind, check, lower};
use astli_syntax::SyntaxKind::VERBATIM;
use astli_text::Span;
use expect_test::expect_file;

#[test]
fn snapshots_lower() {
    snapshots("lower", "hir", |_, parsed| lower(parsed).to_string());
}

#[test]
fn snapshots_resolve() {
    snapshots("resolve", "names", resolved);
}

#[test]
fn snapshots_check() {
    snapshots("check", "check", checked);
}

/// Checks each `.sv` in `tests/data/<dir>` against its `.<extension>`, as
/// `write` writes it from the case's session and tree.
fn snapshots(dir: &str, extension: &str, write: fn(&Session, &Parsed) -> String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir);
    let mut cases = Vec::new();
    let mut strays = Vec::new();
    for path in entries(&root) {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("sv") => cases.push(path),
            Some(ext) if ext == extension && path.with_extension("sv").exists() => {}
            _ => strays.push(path.display().to_string()),
        }
    }
    assert!(!cases.is_empty(), "no cases under {}", root.display());

    let mut failed = Vec::new();
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(case).expect("a case is UTF-8");
        // One mismatch must not hide the rest, so each is caught and the
        // panic message -- the diff -- is left to the default hook to print.
        let outcome = panic::catch_unwind(|| {
            let (session, parsed) = parsed(&name, text);
            expect_file![case.with_extension(extension)].assert_eq(&write(&session, &parsed));
        });
        if outcome.is_err() {
            failed.push(name);
        }
    }

    assert!(
        strays.is_empty(),
        "not a case or a case's `.{extension}`:\n{}",
        strays.join("\n")
    );
    assert!(
        failed.is_empty(),
        "{} of {} cases differ (UPDATE_EXPECT=1 to accept):\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
}

/// The expanded tree of `text`, which must parse whole unless its name says
/// it may not.
fn parsed(name: &str, text: String) -> (Session<'static>, Parsed) {
    let mut session = Session::new();
    let file = session.add(name, text);
    let expanded = session.expand(file);
    let parsed = parse_expanded(&session, &expanded.tokens);
    let verbatim = parsed
        .root
        .descendants()
        .any(|node| node.kind() == VERBATIM);
    let missing = name.starts_with("missing");
    let expanded_ok =
        (expanded.diagnostics.iter()).all(|it| missing && it.code.as_str() == "include-not-found");
    assert!(
        expanded_ok && parsed.diagnostics.is_empty(),
        "{name}: {:?}",
        (expanded.diagnostics.iter().chain(&parsed.diagnostics)).collect::<Vec<_>>()
    );
    assert!(
        !verbatim || name.starts_with("unparsed"),
        "{name}: not parsed whole"
    );
    (session, parsed)
}

/// Each name the case uses, in the order written, and what it refers to.
fn resolved(session: &Session, parsed: &Parsed) -> String {
    let design = Design::new(vec![lower(parsed)]);
    let (file, hir) = design.files().next().unwrap();
    let names = design.resolve(file);
    let origins = session.origins();
    let at = |span: Span| origins.line_col(span.src_id, span.start);

    let mut found = Vec::new();
    for (id, expr) in hir.exprs() {
        if let Some(resolution) = names.expr(id) {
            found.push((expr.span, "", resolution));
        }
    }
    for (name, resolution) in names.loose() {
        found.push((name.span, "loose ", *resolution));
    }
    found.sort_by_key(|(span, _, _)| (span.start, span.end));
    found.dedup();

    let mut out = String::new();
    for (span, loose, resolution) in found {
        let what = match resolution {
            Resolution::Declared(symbol) => {
                let symbol = design.symbol(symbol);
                let kind = match &symbol.kind {
                    SymbolKind::Definition { .. } => "definition",
                    SymbolKind::Class(_) => "class",
                    SymbolKind::Port(_) => "port",
                    SymbolKind::Parameter(_) => "parameter",
                    SymbolKind::Net(_) => "net",
                    SymbolKind::Variable(_) => "variable",
                    SymbolKind::Genvar(_) => "genvar",
                    SymbolKind::Typedef(_) => "typedef",
                    SymbolKind::EnumMember { .. } => "enum member",
                    SymbolKind::Subroutine { .. } => "subroutine",
                    SymbolKind::Instance(_) => "instance",
                    SymbolKind::Block(_) => "block",
                    SymbolKind::Label(_) => "label",
                    SymbolKind::Other(_) => "other",
                };
                format!("{kind} {} at {}", symbol.name.text, at(symbol.name.span))
            }
            Resolution::Implicit(first) => format!("implicit net at {}", at(hir[first].span)),
            Resolution::Unknown => "unknown".to_string(),
            Resolution::Undeclared => "undeclared".to_string(),
        };
        writeln!(
            out,
            "{}: {loose}{} -> {what}",
            at(span),
            origins.slice(span)
        )
        .unwrap();
    }
    out
}

/// Each error `check` finds in the case, with its code.
fn checked(session: &Session, parsed: &Parsed) -> String {
    let design = Design::new(vec![lower(parsed)]);
    let (file, _) = design.files().next().unwrap();
    let origins = session.origins();
    let mut out = String::new();
    for diagnostic in check(&design, file) {
        let at = origins.line_col(diagnostic.at.src_id, diagnostic.at.start);
        let (severity, code) = (diagnostic.severity, diagnostic.code);
        writeln!(out, "{at}: {severity}[{code}] {}", diagnostic.message).unwrap();
    }
    out
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .map(|entry| entry.expect("a readable entry").path())
        .collect();
    entries.sort();
    entries
}
