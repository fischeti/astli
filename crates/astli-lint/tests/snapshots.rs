//! Every `tests/data/<rule>/*.sv`, linted with that rule alone, and what it
//! found compared with the `.lint` beside it: a line per diagnostic, empty
//! when the case is one the rule must accept.
//!
//! A case must parse whole, so that a rule is tested on the tree it reads,
//! except one named `unparsed*.sv`, for a rule that finds what the parser
//! keeps as written.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lint snapshots
//! ```
//!
//! which writes every `.lint` that is missing or differs. The diff is the
//! review.

use std::fmt::Write;
use std::panic;
use std::path::{Path, PathBuf};

use astli_lint::{Config, Group, Level, RULES, lint};
use astli_parse::SyntaxTree;
use expect_test::expect_file;

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut cases = Vec::new();
    let mut strays = Vec::new();
    for dir in entries(&root) {
        let rule = dir.file_name().unwrap().to_string_lossy().into_owned();
        if !dir.is_dir() || !RULES.iter().any(|it| it.name == rule) {
            strays.push(dir.display().to_string());
            continue;
        }
        for path in entries(&dir) {
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("sv") => cases.push((rule.clone(), path)),
                Some("lint") if path.with_extension("sv").exists() => {}
                _ => strays.push(path.display().to_string()),
            }
        }
    }
    assert!(!cases.is_empty(), "no cases under {}", root.display());

    let mut failed = Vec::new();
    for (rule, case) in &cases {
        let name = case
            .strip_prefix(&root)
            .unwrap_or(case)
            .display()
            .to_string();
        let text = std::fs::read_to_string(case).expect("a case is UTF-8");
        // One mismatch must not hide the rest, so each is caught and the
        // panic message -- the diff -- is left to the default hook to print.
        let outcome = panic::catch_unwind(|| {
            let unparsed = case
                .file_name()
                .is_some_and(|it| it.to_string_lossy().starts_with("unparsed"));
            expect_file![case.with_extension("lint")]
                .assert_eq(&found(rule, &name, text, unparsed));
        });
        if outcome.is_err() {
            failed.push(name);
        }
    }

    assert!(
        strays.is_empty(),
        "not a rule's directory, a case, or a case's `.lint`:\n{}",
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

/// What `rule`, run alone, finds in `text`, which may keep constructs as
/// written when `unparsed` allows it.
fn found(rule: &str, name: &str, text: String, unparsed: bool) -> String {
    let tree = SyntaxTree::parse(name, text);
    let unparsed = match unparsed {
        true => Vec::new(),
        false => tree.unparsed(),
    };
    assert!(
        tree.diagnostics().is_empty() && unparsed.is_empty(),
        "{name}: not parsed from line {}",
        (tree.diagnostics().iter().chain(&unparsed))
            .map(|it| tree.line_col(it.at.start).line)
            .min()
            .unwrap_or(0)
    );

    let mut config = Config::default();
    for group in Group::ALL {
        config.set(group.name(), Level::Allow).unwrap();
    }
    config.set(rule, Level::Warn).unwrap();

    let mut out = String::new();
    for diagnostic in lint(&tree, &config) {
        let at = tree.line_col(diagnostic.at.start);
        write!(out, "{at}: [{}] {}", diagnostic.code, diagnostic.message).unwrap();
        match &diagnostic.label {
            Some(caret) => writeln!(out, " ({caret})").unwrap(),
            None => writeln!(out).unwrap(),
        }
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
