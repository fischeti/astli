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
use std::path::Path;

use astli_parse::SyntaxTree;
use astli_syntax::SyntaxNode;
use astli_testing::snapshots;
use rowan::NodeOrToken;

use crate::grammar;

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    snapshots::check(&root, "tree", dump);
}

/// Every case, held to the node shapes `astli.ungram` names. The cases are
/// written to exercise the grammar, so none of them may fall outside it.
#[test]
fn cases_have_the_shapes_the_grammar_names() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let cases = snapshots::cases(&root, &["tree"]).cases;

    let shapes = grammar::Shapes::load();
    let mut mismatches = grammar::Mismatches::default();
    for case in &cases {
        let tree = SyntaxTree::parse(&case.path, case.text());
        shapes.check(tree.root(), &case.name, &mut mismatches);
    }
    assert!(
        mismatches.found.is_empty(),
        "{} nodes do not have the shape their rule names:\n{}",
        mismatches.total(),
        mismatches.report(50)
    );
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
