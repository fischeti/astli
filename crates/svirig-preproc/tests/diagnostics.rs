//! What expansion reports, and that it still recovers the way it did.
//!
//! Every case here is a recovery `docs/limitations.md` tabulated while there
//! was nowhere to report it to. So each test asserts **both** halves: the code
//! that comes out, and that the tokens are what they always were. A diagnostic
//! that changed the expansion would be a regression, not a feature.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use svirig_preproc::{Includes, Session, render};
use svirig_text::Reader;

/// A source tree, so that `` `include `` has somewhere to look.
#[derive(Default)]
struct Tree(HashMap<PathBuf, String>);

impl Reader for Tree {
    fn read(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }
}

impl Tree {
    fn with(mut self, path: &str, text: &str) -> Tree {
        self.0.insert(path.into(), text.to_string());
        self
    }
}

/// One file expanded alone, which is the common case.
fn expand(source: &str) -> (Vec<String>, String) {
    expand_in(Tree::default().with("top.sv", source))
}

/// The same with a tree behind it, for the `` `include `` cases.
fn expand_in(tree: Tree) -> (Vec<String>, String) {
    let text = tree.0[Path::new("top.sv")].clone();
    let mut session = Session::reading(&tree).searching(Includes::new());
    let file = session.add("top.sv", text);
    let expanded = session.expand(file);

    let text = render(session.origins(), &expanded.tokens)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let codes = expanded
        .diagnostics
        .iter()
        .map(|diag| diag.code.as_str().to_string())
        .collect();
    (codes, text)
}

/// The codes alone, for a case whose recovery another test already pins.
fn codes(source: &str) -> Vec<String> {
    expand(source).0
}

#[test]
fn a_clean_file_reports_nothing() {
    let (codes, text) = expand("`define W 8\nlogic [`W-1:0] q;\n");
    assert!(codes.is_empty(), "{codes:?}");
    assert_eq!(text, "logic [8-1:0] q;");
}

#[test]
fn an_undefined_reference_is_reported_and_still_stands() {
    let (codes, text) = expand("logic [`WIDTH-1:0] q;\n");
    assert_eq!(codes, ["undefined-macro"]);
    // The recovery: the reference's own tokens are what is left behind.
    assert_eq!(text, "logic [`WIDTH-1:0] q;");
}

#[test]
fn a_macro_that_reaches_itself_is_reported_and_still_stands() {
    let (codes, text) = expand("`define A `A\n`A\n");
    assert_eq!(codes, ["recursive-macro"]);
    assert_eq!(text, "`A");
}

#[test]
fn a_call_with_no_argument_list_is_reported_and_does_not_expand() {
    let (codes, text) = expand("`define M(x) f(x)\n`M\n");
    assert_eq!(codes, ["missing-argument-list"]);
    assert_eq!(text, "`M");
}

#[test]
fn arguments_past_the_formals_are_reported_and_still_dropped() {
    let (codes, text) = expand("`define M(x) f(x)\n`M(a, b, c)\n");
    assert_eq!(codes, ["too-many-arguments"]);
    assert_eq!(text, "f(a)");
}

#[test]
fn a_formal_with_neither_argument_nor_default_is_reported_and_expands_to_nothing() {
    let (codes, text) = expand("`define M(x, y) f(x, y)\n`M(a)\n");
    assert_eq!(codes, ["missing-argument"]);
    assert_eq!(text, "f(a, )");

    // A default is what makes it not a mistake, and nothing is reported.
    let (codes, text) = expand("`define M(x, y = 1) f(x, y)\n`M(a)\n");
    assert!(codes.is_empty(), "{codes:?}");
    assert_eq!(text, "f(a, 1)");
}

#[test]
fn an_include_that_reads_nowhere_is_reported_and_expands_to_nothing() {
    let tree = Tree::default().with("top.sv", "`include \"nowhere.svh\"\nlogic q;\n");
    let (codes, text) = expand_in(tree);
    assert_eq!(codes, ["include-not-found"]);
    assert_eq!(text, "logic q;");
}

#[test]
fn an_include_that_re_enters_is_told_apart_from_one_that_is_missing() {
    // `f.svh` includes itself: it reads, so this is a cycle rather than a
    // name that resolves to nothing. The two used to be one silent `None`.
    let tree = Tree::default()
        .with("top.sv", "`include \"f.svh\"\n")
        .with("f.svh", "`include \"f.svh\"\nlogic q;\n");
    let (codes, text) = expand_in(tree);
    assert_eq!(codes, ["include-cycle"]);
    // The outer include still happened; only the re-entry did not.
    assert_eq!(text, "logic q;");
}

#[test]
fn an_include_with_no_name_is_reported() {
    let tree = Tree::default()
        .with("top.sv", "`define EMPTY\n`include `EMPTY\nlogic q;\n")
        .with("f.svh", "");
    let (codes, text) = expand_in(tree);
    assert_eq!(codes, ["include-without-name"]);
    assert_eq!(text, "logic q;");
}

#[test]
fn a_conditional_with_no_name_is_reported_and_is_never_taken() {
    let (codes, text) = expand("`ifdef\nlogic taken;\n`else\nlogic otherwise;\n`endif\n");
    assert_eq!(codes, ["conditional-without-name"]);
    // The recovery: with nothing to test the branch cannot be true, and the
    // `` `else `` below it is the one that is well formed.
    assert_eq!(text, "logic otherwise;");
}

#[test]
fn a_region_that_is_never_closed_is_reported_and_runs_to_the_end() {
    let (codes, text) = expand("`define E 1\n`ifdef E\nlogic q;\n");
    assert_eq!(codes, ["unclosed-conditional"]);
    assert_eq!(text, "logic q;");
}

#[test]
fn a_closer_with_no_region_above_it_is_reported_and_consumed() {
    let (codes, text) = expand("logic a;\n`endif\nlogic b;\n");
    assert_eq!(codes, ["stray-conditional"]);
    // The recovery: consumed, like any other directive.
    assert_eq!(text, "logic a; logic b;");
}

#[test]
fn a_stringification_that_is_never_closed_is_reported() {
    let reported = codes("`define S(x) `\"x\n`S(a)\n");
    assert_eq!(reported, ["unclosed-stringification"]);
}

#[test]
fn every_diagnostic_points_somewhere_the_store_can_resolve() {
    let mut session = Session::new();
    let file = session.add(
        "top.sv",
        "`define M(x) f(x)\n`M\n`UNDEFINED\n`endif\n".to_string(),
    );
    let expanded = session.expand(file);

    assert_eq!(expanded.diagnostics.len(), 3);
    for diag in &expanded.diagnostics {
        // Resolving a diagnostic is the store's job, and every one of them has
        // to survive it: this is what a renderer will do to each.
        let at = session.origins().reported_at(diag.at);
        let line = session.origins().line_col(at.file, at.start);
        assert!(line.line >= 1 && line.col >= 1);
        assert!(!diag.message.is_empty());
        assert!(diag.is_error());
    }
}

#[test]
fn each_pass_answers_for_itself() {
    let mut session = Session::new();
    let a = session.add("a.sv", "`A\n".to_string());
    let b = session.add("b.sv", "`B\n".to_string());

    // A pass's diagnostics come back with its tokens rather than piling up on
    // the session, so two files sharing a session do not share complaints and
    // nothing has to be cleared between them.
    assert_eq!(session.expand(a).diagnostics.len(), 1);
    assert_eq!(session.expand(b).diagnostics.len(), 1);
}

#[test]
fn raw_mode_reports_nothing_at_all() {
    // Not merely that it happens not to: `scan` takes `&self`, so there is no
    // sink for it to reach. An undefined macro is the ordinary case when a
    // file is read alone, which is why the modes must disagree.
    let mut session = Session::new();
    let file = session.add(
        "top.sv",
        "`UNDEFINED\n`include \"nowhere.svh\"\n".to_string(),
    );

    // `Scan` has no diagnostics to hold, which is the mode distinction stated
    // in the types rather than left as a rule to remember.
    let scan = session.scan(file);
    assert!(scan.references().count() > 0);
}
