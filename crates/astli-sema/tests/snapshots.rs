//! Every `tests/data/*.sv`, expanded, parsed and lowered, and its HIR
//! written out compared with the `.hir` beside it.
//!
//! A case must parse whole, so that the lowering is tested on the tree it is
//! for, except one named `unparsed*.sv`, for what the parser keeps as
//! written.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-sema snapshots
//! ```
//!
//! which writes every `.hir` that is missing or differs. The diff is the
//! review.

use std::panic;
use std::path::{Path, PathBuf};

use astli_parse::parse_expanded;
use astli_preproc::Session;
use astli_sema::lower;
use expect_test::expect_file;

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut cases = Vec::new();
    let mut strays = Vec::new();
    for path in entries(&root) {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("sv") => cases.push(path),
            Some("hir") if path.with_extension("sv").exists() => {}
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
            expect_file![case.with_extension("hir")].assert_eq(&lowered(&name, text));
        });
        if outcome.is_err() {
            failed.push(name);
        }
    }

    assert!(
        strays.is_empty(),
        "not a case or a case's `.hir`:\n{}",
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

/// The HIR of `text`, which must parse whole unless its name says it may
/// not.
fn lowered(name: &str, text: String) -> String {
    let mut session = Session::new();
    let file = session.add(name, text);
    let expanded = session.expand(file);
    let parsed = parse_expanded(&session, &expanded.tokens);
    let verbatim = parsed
        .root
        .descendants()
        .any(|node| node.kind() == astli_syntax::SyntaxKind::VERBATIM);
    assert!(
        expanded.diagnostics.is_empty() && parsed.diagnostics.is_empty(),
        "{name}: {:?}",
        (expanded.diagnostics.iter().chain(&parsed.diagnostics)).collect::<Vec<_>>()
    );
    assert!(
        !verbatim || name.starts_with("unparsed"),
        "{name}: not parsed whole"
    );
    lower(&parsed).to_string()
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .map(|entry| entry.expect("a readable entry").path())
        .collect();
    entries.sort();
    entries
}
