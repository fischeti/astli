//! Every `tests/data/**/*.sv`, parsed and compared with the `.tree` beside it.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-parse snapshots
//! ```
//!
//! which writes every `.tree` that is missing or differs. The diff is the
//! review: a change to the grammar shows up as a change to these files.
//!
//! The dump leaves out text ranges. The tree is checked against the file byte
//! for byte first, and every token is printed with its text, so the offsets
//! would say nothing new -- and they would shift in every case below an edited
//! comment.

use std::fmt::Write as _;
use std::panic;
use std::path::{Path, PathBuf};

use astli_parse::SyntaxTree;
use astli_syntax::SyntaxNode;
use expect_test::expect_file;
use rowan::NodeOrToken;

mod grammar;

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
        let actual = dump(&name, text);
        let expected = case.with_extension("tree");
        // One mismatch must not hide the rest, so each is caught and the
        // panic message -- the diff -- is left to the default hook to print.
        if panic::catch_unwind(|| expect_file![expected].assert_eq(&actual)).is_err() {
            failed.push(name);
        }
    }

    assert!(
        orphans.is_empty(),
        "a `.tree` with no `.sv` beside it:\n{}",
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

/// Every case, held to the node shapes `astli.ungram` names. The cases are
/// written to exercise the grammar, so none of them may fall outside it.
#[test]
fn cases_have_the_shapes_the_grammar_names() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (mut cases, mut orphans) = (Vec::new(), Vec::new());
    walk(&root, &mut cases, &mut orphans);

    let shapes = grammar::Shapes::load();
    let mut mismatches = grammar::Mismatches::default();
    for case in &cases {
        let text = std::fs::read_to_string(case).expect("a case is UTF-8");
        let tree = SyntaxTree::parse(case, text);
        let name = case
            .strip_prefix(&root)
            .unwrap_or(case)
            .display()
            .to_string();
        shapes.check(tree.root(), &name, &mut mismatches);
    }
    assert!(
        mismatches.found.is_empty(),
        "{} nodes do not have the shape their rule names:\n{}",
        mismatches.total(),
        mismatches.report(50)
    );
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
            Some("tree") if !path.with_extension("sv").exists() => {
                orphans.push(path.display().to_string());
            }
            _ => {}
        }
    }
}

/// The tree, one element per line, then what the parser reported.
fn dump(name: &str, text: String) -> String {
    let tree = SyntaxTree::parse(name, text);
    assert_eq!(
        tree.root().text().to_string(),
        tree.source(),
        "{name}: the tree is not the file"
    );

    let mut out = String::new();
    node(&mut out, tree.root(), 0);

    if !tree.diagnostics().is_empty() {
        out.push_str("---\n");
    }
    let origins = tree.origins();
    for diagnostic in tree.diagnostics() {
        let at = origins.reported_at(diagnostic.at);
        let place = origins.line_col(at.src_id, at.start);
        writeln!(
            out,
            "{}[{}] {}:{} {:?}: {} ({})",
            format!("{:?}", diagnostic.severity).to_lowercase(),
            diagnostic.code.as_str(),
            place.line,
            place.col,
            origins.slice(at),
            diagnostic.message,
            diagnostic.caret(),
        )
        .expect("writing to a string");
    }
    out
}

fn node(out: &mut String, node: &SyntaxNode, depth: usize) {
    let indent = "  ".repeat(depth);
    writeln!(out, "{indent}{:?}", node.kind()).expect("writing to a string");
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(child) => self::node(out, &child, depth + 1),
            NodeOrToken::Token(token) => {
                writeln!(out, "{indent}  {:?} {:?}", token.kind(), token.text())
                    .expect("writing to a string");
            }
        }
    }
}
