//! The parser's fuzz inputs, formatted, and held to what must hold for all
//! input: nothing panics, and formatting the output again changes nothing.
//!
//! A refusal is the formatter catching itself changing what the file means.
//! Where the parser could not read the text, that is the check doing its
//! job: the file is not SystemVerilog, and is left as it was. Anywhere else
//! it is a bug, and the corpus holds real code to that.

use astli_parse::SyntaxTree;
use astli_syntax::SyntaxKind::VERBATIM;

mod corpus;
#[path = "../../astli-parse/tests/generate/mod.rs"]
mod generate;

/// Formats `text` and then its output, and panics, naming `what`, unless the
/// first is accepted or the parser could not read `text`, and the second
/// changes nothing.
fn check(text: &str, what: &str) {
    let tree = SyntaxTree::parse("fuzz.sv", text.to_string());
    let once = match astli_fmt::format(&tree) {
        Ok(once) => once,
        Err(_) if unreadable(&tree) => return,
        Err(refusal) => panic!("{refusal} at byte {}, from {what}", refusal.offset),
    };
    let twice = astli_fmt::format(&SyntaxTree::parse("fuzz.sv", once.clone()))
        .unwrap_or_else(|refusal| panic!("{refusal} in its own output, from {what}"));
    assert_eq!(once, twice, "formatting again changed it, from {what}");
}

/// Whether the parser left some of the tree as written, or reported an error.
fn unreadable(tree: &SyntaxTree) -> bool {
    tree.root()
        .descendants()
        .any(|node| node.kind() == VERBATIM)
        || tree.diagnostics().iter().any(|it| it.is_error())
}

#[test]
fn random_token_sequences_format() {
    for (what, text) in generate::token_sequences() {
        check(&text, &what);
    }
}

#[test]
fn long_random_token_sequences_format() {
    for (what, text) in generate::long_token_sequences() {
        check(&text, &what);
    }
}

#[test]
fn random_bytes_format() {
    for (what, text) in generate::random_bytes() {
        check(&text, &what);
    }
}

#[test]
fn the_empty_file_and_the_shortest_ones_format() {
    for (what, text) in generate::shortest() {
        check(&text, &what);
    }
}

/// Past the parser's limit of 256 open nodes, though not its limit of a tree
/// 2048 deep: a chain that long of anything but one operator is more than the
/// formatter's stack holds (docs/limitations.md).
#[test]
fn deep_nesting_formats() {
    for (what, text) in generate::deep_nestings(300) {
        check(&text, &what);
    }
}

/// The nearly-valid case: real text, cut where nobody would cut it.
#[test]
fn corpus_spliced_files_format() {
    if let Some(inputs) = generate::corpus_splices() {
        for (what, text) in inputs {
            check(&text, &what);
        }
    }
}
