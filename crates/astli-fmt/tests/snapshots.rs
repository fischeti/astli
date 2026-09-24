//! Every `tests/data/**/*.sv`, formatted and compared with the `.out` beside
//! it, then formatted again to check that nothing changes.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-fmt snapshots
//! ```
//!
//! which writes every `.out` that is missing or differs. The diff is the
//! review.

use std::panic;
use std::path::{Path, PathBuf};

use astli_fmt::format;
use astli_parse::SyntaxTree;
use expect_test::expect_file;

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut cases = Vec::new();
    let mut orphans = Vec::new();
    walk(&root, &mut cases, &mut orphans);
    assert!(!cases.is_empty(), "no cases under {}", root.display());

    let mut failed = Vec::new();
    for case in &cases {
        let text = std::fs::read_to_string(case).expect("a case is UTF-8");
        let name = case
            .strip_prefix(&root)
            .unwrap_or(case)
            .display()
            .to_string();
        let expected = case.with_extension("out");
        // One mismatch must not hide the rest, so each is caught and the
        // panic message -- the diff -- is left to the default hook to print.
        let outcome = panic::catch_unwind(|| {
            let once = formatted(&name, text);
            expect_file![expected].assert_eq(&once);
            assert_eq!(
                formatted(&name, once.clone()),
                once,
                "{name}: not idempotent"
            );
        });
        if outcome.is_err() {
            failed.push(name);
        }
    }

    assert!(
        orphans.is_empty(),
        "an `.out` with no `.sv` beside it:\n{}",
        orphans.join("\n")
    );
    assert!(
        failed.is_empty(),
        "{} of {} cases differ (UPDATE_EXPECT=1 to accept):\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
}

fn formatted(name: &str, text: String) -> String {
    let tree = SyntaxTree::parse(name, text);
    format(&tree)
        .unwrap_or_else(|refusal| panic!("{name}:{}: {refusal}", tree.line_col(refusal.offset)))
}

fn walk(dir: &Path, cases: &mut Vec<PathBuf>, orphans: &mut Vec<String>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .map(|entry| entry.expect("a readable entry").path())
        .collect();
    entries.sort();
    for path in entries {
        match path.extension().and_then(|ext| ext.to_str()) {
            _ if path.is_dir() => walk(&path, cases, orphans),
            Some("sv") => cases.push(path),
            Some("out") if !path.with_extension("sv").exists() => {
                orphans.push(path.display().to_string());
            }
            _ => {}
        }
    }
}
