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

use astli_fmt::{Options, format};
use astli_parse::SyntaxTree;
use expect_test::expect_file;

#[test]
fn getter_setter_macro_formats_and_is_idempotent() {
    fn expansion(source: &str) -> (Vec<(astli_syntax::SyntaxKind, String)>, Vec<String>) {
        use astli_syntax::SyntaxKind::*;
        let mut session = astli_preproc::Session::new();
        let file = session.add("accessors.sv", source.to_string());
        let expanded = session.expand(file);
        assert!(
            expanded
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != astli_text::Severity::Error)
        );
        // Redundant paste operators in the source can warn. Formatting must
        // preserve those diagnostics as well as the generated tokens.
        let diagnostics = expanded
            .diagnostics
            .iter()
            .map(|diagnostic| {
                format!(
                    "{:?}:{}:{}",
                    diagnostic.severity,
                    diagnostic.code.as_str(),
                    diagnostic.message
                )
            })
            .collect();
        let parsed = astli_parse::parse_expanded(&session, &expanded.tokens);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(
            parsed
                .root
                .descendants()
                .filter(|node| node.kind() == FUNCTION_DECL)
                .count(),
            2
        );
        let tokens: Vec<_> = parsed
            .root
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .map(|token| (token.kind(), token.text().to_string()))
            .collect();
        for name in ["get_values", "set_values", "values_"] {
            assert!(
                tokens
                    .iter()
                    .any(|(kind, text)| *kind == IDENT && text == name)
            );
        }
        (tokens, diagnostics)
    }
    let name = "preproc/define-accessors.svh";
    let source = include_str!("../data/preproc/define-accessors.svh");
    let tree = SyntaxTree::parse(name, source.to_string());
    assert_eq!(tree.root().text().to_string(), source);
    assert!(tree.diagnostics().is_empty());
    assert!(tree.unparsed().is_empty());
    let once = format(&tree, &Options::default()).expect("macro formatting refused");
    assert_eq!(expansion(source), expansion(&once));
    assert_eq!(formatted(name, once.clone()), once, "not idempotent");
}

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
    format(&tree, &Options::default())
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
