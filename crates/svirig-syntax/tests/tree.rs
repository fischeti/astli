//! The join between the kind enum and `rowan`.
//!
//! Nothing here parses anything -- there is no parser yet. What it checks is
//! that a kind survives the trip into a green tree and back out, which is the
//! assumption every later rung is built on.

use rowan::{GreenNodeBuilder, Language};
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode, SystemVerilog, tokenize};

/// Builds `SOURCE_FILE` over one `VERBATIM` holding every token of `src`.
///
/// The shape steps 4 and 5 will produce for a file the parser makes nothing
/// of, and the smallest tree that exercises both a node and its leaves.
fn verbatim_tree(src: &str) -> SyntaxNode {
    let mut builder = GreenNodeBuilder::new();
    builder.start_node(SystemVerilog::kind_to_raw(SOURCE_FILE));
    builder.start_node(SystemVerilog::kind_to_raw(VERBATIM));
    for token in tokenize(src) {
        if token.kind == EOF {
            continue;
        }
        builder.token(SystemVerilog::kind_to_raw(token.kind), token.text(src));
    }
    builder.finish_node();
    builder.finish_node();
    SyntaxNode::new_root(builder.finish())
}

#[test]
fn every_raw_discriminant_names_its_own_kind() {
    // `from_raw` transmutes under a bounds check, which is sound only while
    // the discriminants are 0..LAST with no holes. Pinning one by hand is the
    // way that stops being true, and it fails here.
    for raw in 0..LAST as u16 {
        assert_eq!(SyntaxKind::from_raw(raw) as u16, raw);
    }
}

#[test]
#[should_panic(expected = "names no SyntaxKind")]
fn a_number_past_the_end_is_not_a_kind() {
    SyntaxKind::from_raw(LAST as u16);
}

#[test]
fn tokens_and_nodes_are_two_contiguous_runs() {
    for kind in [
        WHITESPACE, IDENT, MODULE_KW, L_PAREN, TICK_IDENT, LEX_ERROR, EOF,
    ] {
        assert!(kind.is_token(), "{kind:?}");
        assert!(!kind.is_node(), "{kind:?}");
    }
    for kind in [SOURCE_FILE, VERBATIM] {
        assert!(kind.is_node(), "{kind:?}");
        assert!(!kind.is_token(), "{kind:?}");
    }
    // Not a kind, so neither -- and saying so here is what keeps a future
    // `is_token` from quietly counting it.
    assert!(!LAST.is_token() && !LAST.is_node());
}

#[test]
fn a_kind_round_trips_through_the_language() {
    for raw in 0..LAST as u16 {
        let kind = SyntaxKind::from_raw(raw);
        let there = SystemVerilog::kind_to_raw(kind);
        assert_eq!(SystemVerilog::kind_from_raw(there), kind);
    }
}

#[test]
fn a_tree_gives_back_the_bytes_it_was_built_from() {
    // The round-trip invariant, at the size of one line. Step 4 asserts the
    // same thing over the corpus, once events are what build the tree.
    let src = "module foo; // hi\n  assign x = 1'b0;\nendmodule\n";
    let tree = verbatim_tree(src);
    assert_eq!(tree.text().to_string(), src);
    assert_eq!(tree.kind(), SOURCE_FILE);
}

#[test]
fn kinds_survive_the_trip_into_the_tree() {
    let src = "assign \\odd.name[0] = `WIDTH'h1F;";
    let tree = verbatim_tree(src);
    let verbatim = tree.first_child().unwrap();
    assert_eq!(verbatim.kind(), VERBATIM);

    let kinds: Vec<SyntaxKind> = verbatim
        .children_with_tokens()
        .map(|element| element.kind())
        .collect();
    assert_eq!(
        kinds,
        [
            ASSIGN_KW,
            WHITESPACE,
            ESCAPED_IDENT,
            EQ,
            WHITESPACE,
            TICK_IDENT,
            BASED_LITERAL,
            SEMICOLON,
        ]
    );
}
