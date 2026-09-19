//! What the grammar reports, and the much larger set of things it does not.
//!
//! The second half is the point, and is why there is only one of the first.
//! [D3](../../../docs/plan.md)'s `VERBATIM` fallback means a construct no rule
//! claims is a *successful* recovery, so a diagnostic for one would report how
//! much of Annex A is written rather than anything about the file. These tests
//! pin the silence at least as firmly as the noise.

use svirig_parse::SyntaxTree;

fn codes(source: &str) -> Vec<String> {
    let tree = SyntaxTree::parse("top.sv", source.to_string());
    tree.diagnostics()
        .iter()
        .map(|diag| diag.code.as_str().to_string())
        .collect()
}

/// Whatever was reported, the tree's text is still the file's.
fn round_trips(source: &str) -> bool {
    SyntaxTree::parse("top.sv", source.to_string())
        .root()
        .text()
        == source
}

#[test]
fn ordinary_source_reports_nothing() {
    let source = "\
module top #(parameter int W = 8) (input logic clk_i, output logic [W-1:0] q_o);
  always_ff @(posedge clk_i) q_o <= q_o + 1;
endmodule
";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
    assert!(round_trips(source));
}

#[test]
fn a_construct_no_rule_claims_is_a_recovery_and_not_a_complaint() {
    // Whatever of Annex A is unwritten on any given day lands in `VERBATIM`,
    // and that is the design. If this ever starts reporting, the parser has
    // begun complaining about itself.
    let source = "module top;\n  nettype real wire_t with resolver;\nendmodule\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
    assert!(round_trips(source));
}

#[test]
fn a_file_that_ends_with_something_open_is_reported() {
    // A fact about the file and not about the grammar: a construct nothing
    // recognises still *balances*, so its run leaves nothing on the stack.
    assert_eq!(
        codes("module top;\n  logic q;\n"),
        ["unclosed-at-end-of-file"]
    );
    assert_eq!(codes("class C;\n  int x;\n"), ["unclosed-at-end-of-file"]);
    assert!(round_trips("module top;\n  logic q;\n"));
}

#[test]
fn the_message_names_what_is_open_and_what_would_close_it() {
    let tree = SyntaxTree::parse("top.sv", "module top;\n  logic q;\n".to_string());
    let diag = &tree.diagnostics()[0];

    assert_eq!(diag.message, "the file ends with `module` still open");
    assert_eq!(diag.caret(), "expected `endmodule`");

    // It points at the thing that is open, not at the end of the file: the
    // `module` on line 1 is what the reader has to go and close.
    let at = tree.origins().reported_at(diag.at);
    assert_eq!(tree.origins().slice(at), "module");
    let place = tree.origins().line_col(at.file, at.start);
    assert_eq!((place.line, place.col), (1, 1));
}

#[test]
fn the_keywords_that_only_sometimes_open_a_body_do_not_report() {
    // `verbatim` guesses whether each of these opens something, and a wrong
    // guess that never closes would reach the end of the file and look
    // unbalanced. None of these is unbalanced, so none may report.
    for source in [
        "module top;\n extern function void f();\nendmodule\n",
        "class C;\n pure virtual function void f();\nendclass\n",
        "module top;\n virtual interface i_if vif;\nendmodule\n",
        "typedef class C;\n",
        "module top;\n assert property (@(posedge c) a |-> b);\nendmodule\n",
        "module top;\n import \"DPI-C\" function void f();\nendmodule\n",
        "interface class IC;\n pure virtual function void g();\nendclass\n",
        "module top;\n sequence s; a ##1 b; endsequence\nendmodule\n",
        "module top;\n property p; a |-> b; endproperty\nendmodule\n",
        "package p;\n typedef enum { A, B } e_t;\nendpackage\n",
    ] {
        assert!(
            codes(source).is_empty(),
            "{:?} for {source:?}",
            codes(source)
        );
        assert!(round_trips(source));
    }
}

#[test]
fn an_empty_file_reports_nothing() {
    assert!(codes("").is_empty());
    assert!(codes("\n\n").is_empty());
}
