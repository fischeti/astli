//! Conditional regions: the structure raw mode reads, and the choice the
//! expanded mode makes from it.
//!
//! Assertions are written against source text rather than token indices,
//! because an index tells you nothing when the test fails.

use std::rc::Rc;

use astli_preproc::{Input, Region, Session, Taken, regions, render};
use astli_syntax::Token;
use astli_text::SourceId;

/// One file, with everything the two readings need to be asked of it.
struct Source {
    session: Session<'static>,
    file: SourceId,
    /// Kept so that a span can be read back as the text it covers.
    tokens: Rc<[Token]>,
}

impl Source {
    fn new(text: &str) -> Source {
        let mut session = Session::new();
        let file = session.add("top.sv", text.to_string());
        let tokens = session.tokens(file);
        Source {
            session,
            file,
            tokens,
        }
    }

    fn input(&self) -> Input<'_> {
        self.session.input(self.file)
    }

    fn regions(&self) -> Vec<Region> {
        let input = self.input();
        regions(&input, input.span(0..input.len()))
    }

    fn only(&self) -> Region {
        let mut found = self.regions();
        assert_eq!(found.len(), 1, "expected one region");
        found.remove(0)
    }

    /// Each branch as `directive => body`, which is how a region reads.
    fn shape(&self, region: &Region) -> Vec<String> {
        region
            .branches
            .iter()
            .map(|branch| {
                format!(
                    "{} => {}",
                    self.text(branch.directive),
                    flat(self.text(branch.body))
                )
            })
            .collect()
    }

    fn text(&self, span: astli_preproc::TokenSpan) -> &str {
        if span.is_empty() {
            return "";
        }
        self.session.origins().slice(span.bytes(&self.tokens))
    }
}

fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The expansion as one line, which is what the evaluation tests are about.
fn expanded(text: &str) -> String {
    let mut session = Session::new();
    let file = session.add("top.sv", text.to_string());
    let tokens = session.expand(file).tokens;
    flat(&render(session.origins(), &tokens))
}

// -- the structure raw mode reads ------------------------------------------

#[test]
fn a_region_splits_into_the_branches_that_were_written() {
    let source =
        Source::new("`ifdef A\n  one;\n`elsif B\n  two;\n`else\n  three;\n`endif\nafter;\n");
    let region = source.only();
    assert_eq!(
        source.shape(&region),
        ["`ifdef A => one;", "`elsif B => two;", "`else => three;",]
    );
    // The region stops at its `` `endif ``, and what follows is not its.
    assert!(flat(source.text(region.tokens)).ends_with("`endif"));
    assert!(region.closed);
    assert!(region.has_else());
}

#[test]
fn a_region_without_an_else_names_no_alternative() {
    let source = Source::new("`ifndef GUARD\nbody;\n`endif\n");
    let region = source.only();
    assert_eq!(source.shape(&region), ["`ifndef GUARD => body;"]);
    assert!(!region.has_else());
    assert!(matches!(region.branches[0].taken, Taken::Undefined(_)));
}

#[test]
fn regions_report_only_the_outermost() {
    // A branch's own regions are found by asking again about that branch.
    let source =
        Source::new("`ifdef A\n`ifdef B\ninner;\n`endif\n`endif\n`ifdef C\nnext;\n`endif\n");
    let found = source.regions();
    assert_eq!(found.len(), 2);

    let input = source.input();
    let inner = regions(&input, found[0].branches[0].body);
    assert_eq!(source.shape(&inner[0]), ["`ifdef B => inner;"]);
}

#[test]
fn a_conditional_in_a_macro_body_is_not_a_region_of_the_file() {
    // The body is substitution text, so its `` `endif `` closes the region the
    // body opens wherever the macro is used -- not one here.
    let source = Source::new("`define GUARD(x) `ifdef E x `endif\n`ifdef A\nreal;\n`endif\n");
    let found = source.regions();
    assert_eq!(found.len(), 1);
    assert_eq!(source.shape(&found[0]), ["`ifdef A => real;"]);
}

#[test]
fn a_region_that_never_closes_runs_to_the_end_of_the_text() {
    let source = Source::new("`ifdef A\nbody;\n");
    let region = source.only();
    assert!(!region.closed);
    assert_eq!(source.shape(&region), ["`ifdef A => body;"]);
}

// -- the choice the expanded mode makes ------------------------------------

#[test]
fn a_branch_is_taken_or_it_is_not() {
    assert_eq!(expanded("`define A 1\n`ifdef A\nyes;\n`endif\n"), "yes;");
    assert_eq!(expanded("`ifdef A\nyes;\n`endif\n"), "");
    assert_eq!(expanded("`ifndef A\nyes;\n`endif\n"), "yes;");
    assert_eq!(expanded("`define A 1\n`ifndef A\nyes;\n`endif\n"), "");
}

#[test]
fn the_first_branch_that_matches_wins() {
    let chain = "`ifdef A\na;\n`elsif B\nb;\n`elsif C\nc;\n`else\nd;\n`endif\n";
    assert_eq!(expanded(&format!("`define A 1\n{chain}")), "a;");
    assert_eq!(
        expanded(&format!("`define B 1\n`define C 1\n{chain}")),
        "b;"
    );
    assert_eq!(expanded(&format!("`define C 1\n{chain}")), "c;");
    assert_eq!(expanded(chain), "d;");
}

#[test]
fn a_branch_that_is_not_taken_is_not_text() {
    // Its `` `define `` never reaches the table, which is the whole point:
    // evaluating a conditional decides what the rest of the file even says.
    assert_eq!(
        expanded("`ifdef NOPE\n`define W 64\n`else\n`define W 32\n`endif\nlogic [`W-1:0] q;\n"),
        "logic [32-1:0] q;"
    );
    // And its `` `undef `` never happens.
    assert_eq!(
        expanded("`define W 8\n`ifdef NOPE\n`undef W\n`endif\nx = `W;\n"),
        "x = 8;"
    );
}

#[test]
fn a_reference_in_a_branch_that_is_not_taken_is_never_expanded() {
    // Undefined, and it does not matter: nothing reads it.
    assert_eq!(
        expanded("`ifdef NOPE\n`MISSING(a, b)\n`endif\nafter;\n"),
        "after;"
    );
}

#[test]
fn a_definition_made_in_a_taken_branch_outlives_the_region() {
    assert_eq!(
        expanded("`ifndef NOPE\n`define W 16\n`endif\nlogic [`W-1:0] q;\n"),
        "logic [16-1:0] q;"
    );
}

#[test]
fn regions_nest() {
    let nested = concat!(
        "`ifdef OUTER\n",
        "  `ifdef INNER\n    both;\n  `else\n    outer;\n  `endif\n",
        "`else\n",
        "  neither;\n",
        "`endif\n",
    );
    assert_eq!(
        expanded(&format!("`define OUTER 1\n`define INNER 1\n{nested}")),
        "both;"
    );
    assert_eq!(expanded(&format!("`define OUTER 1\n{nested}")), "outer;");
    assert_eq!(expanded(&format!("`define INNER 1\n{nested}")), "neither;");
}

#[test]
fn a_conditional_in_a_macro_body_is_evaluated_where_the_macro_is_used() {
    let macro_def = "`define PICK(x) `ifdef E x `else nothing `endif\n";
    assert_eq!(
        expanded(&format!("`define E 1\n{macro_def}`PICK(chosen);\n")),
        "chosen ;"
    );
    assert_eq!(
        expanded(&format!("{macro_def}`PICK(chosen);\n")),
        "nothing ;"
    );
}

#[test]
fn a_conditional_tests_a_name_however_it_is_spelled() {
    // The table keys a name with its backtick and its escape stripped, and a
    // condition asks the table.
    assert_eq!(
        expanded("`define \\A.B x\n`ifdef \\A.B\nyes;\n`endif\n"),
        "yes;"
    );
}

#[test]
fn a_stray_endif_is_consumed_like_any_other_directive() {
    assert_eq!(expanded("a;\n`endif\nb;\n"), "a; b;");
    assert_eq!(expanded("a;\n`else\nb;\n"), "a; b;");
}

#[test]
fn a_conditional_with_no_name_takes_the_branch_that_has_one() {
    // An error by 22.6. The name is the whole condition, so with none there is
    // nothing to be true, and the `` `else `` is the branch that is written
    // correctly.
    assert_eq!(expanded("`ifdef\nbad;\n`else\ngood;\n`endif\n"), "good;");
    assert_eq!(expanded("`ifdef\nbad;\n`endif\nafter;\n"), "after;");
}

#[test]
fn a_region_that_never_closes_still_ends_at_the_file() {
    assert_eq!(expanded("`define A 1\n`ifdef A\nbody;\n"), "body;");
    assert_eq!(expanded("`ifdef A\nbody;\n"), "");
}
