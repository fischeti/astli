//! What a `` ` `` builds in the tree.

use rowan::NodeOrToken;
use svirig_syntax::parser::{Expanded, Tokens, parse};
use svirig_syntax::preproc::{ExpandedToken, Includes, Input, expand};
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode, Token, tokenize};
use svirig_text::{Disk, FileId, Origins};

struct Source {
    origins: Origins,
    file: FileId,
    tokens: Vec<Token>,
}

impl Source {
    fn new(text: &str) -> Source {
        let mut origins = Origins::new();
        let file = origins.add_file("top.sv", text.to_string());
        let tokens = tokenize(origins.text(file));
        Source {
            origins,
            file,
            tokens,
        }
    }

    fn input(&self) -> Input<'_> {
        Input::new(self.file, self.origins.text(self.file), &self.tokens)
    }
}

fn tree(text: &str) -> SyntaxNode {
    let source = Source::new(text);
    let tree = parse(source.input());
    // Losslessness is the invariant no rung may break, and a rule that
    // miscounts a shape breaks it here rather than three steps downstream.
    assert_eq!(tree.text().to_string(), text, "the tree is not the file");
    tree
}

/// The nodes one file parses to and how they nest, tokens left out.
///
/// These tests are about shape. What the bytes are is the round-trip's
/// business, and it is asserted on every call above.
fn shape(text: &str) -> String {
    fn walk(node: &SyntaxNode, out: &mut String) {
        out.push_str(&format!("{:?}", node.kind()));
        let children: Vec<SyntaxNode> = node.children().collect();
        if children.is_empty() {
            return;
        }
        out.push('(');
        for (at, child) in children.iter().enumerate() {
            if at > 0 {
                out.push(' ');
            }
            walk(child, out);
        }
        out.push(')');
    }

    let mut out = String::new();
    walk(&tree(text), &mut out);
    out
}

/// The text of every node of `kind`, in source order, trimmed of the trivia
/// the builder hung on its edges.
fn texts(text: &str, kind: SyntaxKind) -> Vec<String> {
    fn walk(node: &SyntaxNode, kind: SyntaxKind, out: &mut Vec<String>) {
        if node.kind() == kind {
            out.push(node.text().to_string().trim().to_string());
        }
        for child in node.children() {
            walk(&child, kind, out);
        }
    }

    let mut out = Vec::new();
    walk(&tree(text), kind, &mut out);
    out
}

// ---------------------------------------------------------------- directives

#[test]
fn a_directive_is_a_node_rather_than_a_run() {
    assert_eq!(
        shape("`timescale 1ns / 1ps\n`resetall\n"),
        "SOURCE_FILE(DIRECTIVE DIRECTIVE)"
    );
}

#[test]
fn a_definition_keeps_its_body_apart() {
    assert_eq!(
        shape("`define WIDTH 8\n"),
        "SOURCE_FILE(DIRECTIVE(MACRO_BODY))"
    );
    assert_eq!(texts("`define WIDTH 8\n", MACRO_BODY), ["8"]);
}

#[test]
fn a_definition_with_no_body_has_no_node_for_one() {
    assert_eq!(shape("`define GUARD\n"), "SOURCE_FILE(DIRECTIVE)");
}

#[test]
fn a_body_is_text_and_nothing_in_it_is_read() {
    // The directives in a body belong to wherever the macro is used, so the
    // region here is the *body's* and closes nothing at this level.
    assert_eq!(
        shape("`define GUARD(x) `ifdef E x `endif\n"),
        "SOURCE_FILE(DIRECTIVE(MACRO_BODY))"
    );
    assert_eq!(
        texts("`define GUARD(x) `ifdef E x `endif\n", MACRO_BODY),
        ["`ifdef E x `endif"]
    );
}

#[test]
fn a_continued_body_is_one_node() {
    let text = "`define A(x) \\\n  do_thing(x); \\\n  do_other(x)\n";
    assert_eq!(shape(text), "SOURCE_FILE(DIRECTIVE(MACRO_BODY))");
}

#[test]
fn a_conditional_directive_with_nothing_above_it_is_an_ordinary_one() {
    assert_eq!(shape("`endif\n`else\n"), "SOURCE_FILE(DIRECTIVE DIRECTIVE)");
}

// -------------------------------------------------------------- macro calls

#[test]
fn a_reference_with_no_arguments_is_a_call_on_its_own() {
    assert_eq!(shape("`MY_MACRO\n"), "SOURCE_FILE(MACRO_CALL)");
}

#[test]
fn a_call_holds_one_node_per_argument() {
    assert_eq!(
        shape("`define A(x, y) x\n`A(1, 2)\n"),
        "SOURCE_FILE(DIRECTIVE(MACRO_BODY) MACRO_CALL(MACRO_ARG_LIST(MACRO_ARG MACRO_ARG)))"
    );
}

#[test]
fn an_argument_splits_only_at_depth_zero() {
    let text = "`define A(x, y) x\n`A(f(1, 2), {a, b})\n";
    assert_eq!(texts(text, MACRO_ARG), ["f(1, 2)", "{a, b}"]);
}

#[test]
fn an_empty_argument_is_still_an_argument() {
    // `A() passes one and `A(,) passes two, because the list is split on
    // commas and nothing else -- so the children have to count them.
    assert_eq!(texts("`A()\n", MACRO_ARG), [""]);
    assert_eq!(texts("`A(,)\n", MACRO_ARG), ["", ""]);
}

#[test]
fn a_nullary_macro_keeps_the_parenthesis_that_is_not_its_own() {
    // `define WITH iff stands in for a keyword, so the `(` opens an ordinary
    // expression. The definition is what settles it.
    assert_eq!(
        shape("`define WITH iff\nassert (a) `WITH (!b);\n"),
        "SOURCE_FILE(DIRECTIVE(MACRO_BODY) VERBATIM(MACRO_CALL))"
    );
}

#[test]
fn a_call_is_an_atom_wherever_it_stands() {
    // Statement position, inside a run the grammar cannot yet shape. The
    // whole point of Level B: verification code is written out of these.
    assert_eq!(
        shape("module m;\n  `uvm_info(\"T\", \"m\", LOW)\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(MACRO_CALL(MACRO_ARG_LIST(MACRO_ARG MACRO_ARG MACRO_ARG))))"
    );
}

// ------------------------------------------------------- conditional regions

#[test]
fn every_branch_a_region_writes_is_present() {
    // Raw mode cannot evaluate the condition: the formatter does not know
    // what a build system will define, so it keeps all of them.
    assert_eq!(
        shape("`ifdef A\na <= 1;\n`elsif B\nb <= 2;\n`else\nc <= 3;\n`endif\n"),
        "SOURCE_FILE(CONDITIONAL_REGION(\
           CONDITIONAL_BRANCH(VERBATIM) \
           CONDITIONAL_BRANCH(VERBATIM) \
           CONDITIONAL_BRANCH(VERBATIM)))"
    );
}

#[test]
fn the_endif_closes_the_region_rather_than_the_last_branch() {
    let text = "`ifdef A\na <= 1;\n`endif\n";
    let region = tree(text)
        .children()
        .find(|node| node.kind() == CONDITIONAL_REGION)
        .expect("a region");
    let last = region
        .children_with_tokens()
        .filter_map(|child| match child {
            NodeOrToken::Token(token) if !token.kind().is_trivia() => Some(token),
            _ => None,
        })
        .last()
        .expect("a token of its own");
    assert_eq!(last.text(), "`endif");
}

#[test]
fn a_region_with_no_endif_runs_to_the_end_of_its_text() {
    assert_eq!(
        shape("`ifdef A\na <= 1;\n"),
        "SOURCE_FILE(CONDITIONAL_REGION(CONDITIONAL_BRANCH(VERBATIM)))"
    );
}

#[test]
fn a_region_inside_a_branch_is_a_region_of_its_own() {
    assert_eq!(
        shape("`ifdef A\n`ifdef B\nb <= 1;\n`endif\n`else\nc <= 2;\n`endif\n"),
        "SOURCE_FILE(CONDITIONAL_REGION(\
           CONDITIONAL_BRANCH(CONDITIONAL_REGION(CONDITIONAL_BRANCH(VERBATIM))) \
           CONDITIONAL_BRANCH(VERBATIM)))"
    );
}

#[test]
fn a_ragged_branch_does_not_swallow_the_rest_of_the_region() {
    // The delimiter is handed across the boundary: each branch opens a `begin`
    // that the text after the `endif` closes. Bounding the branch is what
    // stops the first one going looking for the second's `end`.
    let text =
        "`ifdef SYN\n  always_comb begin\n`else\n  always_ff begin\n`endif\n  x <= 1;\nend\n";
    assert_eq!(
        shape(text),
        "SOURCE_FILE(CONDITIONAL_REGION(\
           CONDITIONAL_BRANCH(VERBATIM) \
           CONDITIONAL_BRANCH(VERBATIM)) \
         VERBATIM VERBATIM)"
    );
}

#[test]
fn a_ragged_region_costs_one_construct_and_no_more() {
    // The `begin` is opened inside the region and closed outside it, so
    // nothing that encloses the region can balance -- no reading of this text
    // balances, which is what ragged *means*. What the atom buys is that the
    // damage stops: the region is one child of the module, the `end` it could
    // not account for is a run beside it, and the module after it is reached
    // intact.
    let text = "module m;\n`ifdef SYN\n  if (a) begin\n`else\n  if (b) begin\n`endif\n  end\nendmodule\nmodule n;\nendmodule\n";
    assert_eq!(
        shape(text),
        "SOURCE_FILE(\
           MODULE_DECL(\
             CONDITIONAL_REGION(CONDITIONAL_BRANCH(VERBATIM) CONDITIONAL_BRANCH(VERBATIM)) \
             VERBATIM) \
           MODULE_DECL)"
    );

    // And the module after the region is whole, which is the claim: what a
    // ragged region costs is the one construct enclosing it.
    let last = tree(text)
        .children()
        .filter(|node| node.kind() == MODULE_DECL)
        .last()
        .expect("a module");
    assert_eq!(last.text().to_string().trim(), "module n;\nendmodule");
}

// ------------------------------------------------------------ the other path

#[test]
fn the_expanded_stream_has_none_of_this_to_shape() {
    // A rule that shapes these correctly in raw mode does nothing at all here,
    // without asking why: the reference is gone, the directive has run, and
    // the branch that was taken is simply the text.
    let source = Source::new("`define W 8\n`ifdef W\nlogic [`W-1:0] x;\n`endif\n");
    let mut origins = Origins::new();
    let file = origins.add_file("top.sv", source.origins.text(source.file).to_string());
    let tokens: Vec<ExpandedToken> = expand(&mut origins, file, &Includes::new(), &Disk);

    let mut expanded = Expanded::new(&origins, &tokens);
    while !expanded.at_end() {
        assert_eq!(expanded.macro_call(), None);
        assert_eq!(expanded.directive(), None);
        assert_eq!(expanded.region(), None);
        expanded.bump();
    }
}
