//! Random input, held to the two properties that must hold for *all* input.
//!
//! **It comes back out byte for byte, and nothing panics.** Everything else
//! the parser does is a judgement about what the text means, and a judgement
//! can be wrong without the tool being broken -- that is what
//! the fallback (`src/verbatim.rs`) is for. These two are not
//! judgements. A formatter that loses a byte is a formatter nobody may run,
//! and a parser that panics on the fourth file of a repository is one nobody
//! will run twice.
//!
//! # Why a seeded generator and not `cargo-fuzz`
//!
//! libFuzzer finds more, given a week and a corpus of its own. What is wanted
//! here is a property that runs on every `cargo nextest run`, on the machine
//! of whoever broke it, and reports the *same* failure twice -- so the input
//! is generated from a counter and the seed is in the panic message. A real
//! fuzz target is worth adding when there is CI to run it in; it would use
//! these same generators, in `generate/`.

use astli_parse::SyntaxTree;
use astli_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};
use rowan::NodeOrToken;

mod corpus;
mod generate;

/// The two properties, plus the one structural claim that holds for any input
/// at all.
fn check(text: &str, what: &str) {
    let parsed = SyntaxTree::parse("fuzz.sv", text.to_string());
    let tree = parsed.root();

    assert_eq!(
        tree.text().to_string(),
        text,
        "the tree is not the input, from {what}"
    );
    assert_eq!(tree.kind(), SOURCE_FILE, "no root, from {what}");

    // A rule that closed a node over text it never read would show here and
    // nowhere else: the bytes would still round-trip.
    if let Some(bad) = malformed(tree) {
        panic!("{bad}, from {what}");
    }
}

/// The first shell whose own tokens are not the keyword it claims and the
/// `end…` that matches, if there is one.
fn malformed(root: &SyntaxNode) -> Option<String> {
    fn bounds(kind: SyntaxKind) -> Option<(&'static [SyntaxKind], SyntaxKind)> {
        Some(match kind {
            MODULE_DECL => (&[MODULE_KW, MACROMODULE_KW][..], ENDMODULE_KW),
            INTERFACE_DECL => (&[INTERFACE_KW], ENDINTERFACE_KW),
            PROGRAM_DECL => (&[PROGRAM_KW], ENDPROGRAM_KW),
            PACKAGE_DECL => (&[PACKAGE_KW], ENDPACKAGE_KW),
            CLASS_DECL => (&[CLASS_KW, VIRTUAL_KW, INTERFACE_KW], ENDCLASS_KW),
            GENERATE_REGION => (&[GENERATE_KW], ENDGENERATE_KW),
            CASE_STMT => (
                &[
                    CASE_KW,
                    CASEX_KW,
                    CASEZ_KW,
                    UNIQUE_KW,
                    UNIQUE0_KW,
                    PRIORITY_KW,
                ],
                ENDCASE_KW,
            ),
            _ => return None,
        })
    }

    // Not recursive: a tree may be thousands of levels deep.
    root.descendants().find_map(|node| {
        let (open, close) = bounds(node.kind())?;
        let mut own: Vec<SyntaxKind> = node
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .map(|token| token.kind())
            .filter(|kind| !kind.is_trivia())
            .collect();
        // `endmodule : m` -- the label is the node's too.
        if own.len() >= 3 && own[own.len() - 2] == COLON {
            own.truncate(own.len() - 2);
        }
        let well_formed =
            own.first().is_some_and(|kind| open.contains(kind)) && own.last() == Some(&close);
        (!well_formed).then(|| format!("{:?} is {own:?}", node.kind()))
    })
}

#[test]
fn random_token_sequences_round_trip() {
    for (what, text) in generate::token_sequences() {
        check(&text, &what);
    }
}

#[test]
fn long_random_token_sequences_round_trip() {
    for (what, text) in generate::long_token_sequences() {
        check(&text, &what);
    }
}

#[test]
fn random_bytes_round_trip() {
    for (what, text) in generate::random_bytes() {
        check(&text, &what);
    }
}

#[test]
fn the_empty_file_and_the_shortest_ones() {
    for (what, text) in generate::shortest() {
        check(&text, &what);
    }
}

#[test]
fn deep_nesting_round_trips() {
    // Past both of the parser's limits, 256 open nodes and a tree 2048 deep,
    // so that what is tested is what happens beyond them.
    for (what, text) in generate::deep_nestings(3_000) {
        check(&text, &what);
    }
}

/// The nearly-valid case: real text, cut where nobody would cut it.
#[test]
fn corpus_spliced_files_round_trip() {
    if let Some(inputs) = generate::corpus_splices() {
        for (what, text) in inputs {
            check(&text, &what);
        }
    }
}
