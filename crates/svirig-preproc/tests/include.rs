//! Following an `` `include ``: where the file is found, what it brings with
//! it, and where the search stops.
//!
//! The files are held in memory rather than on disk. Resolution is a list of
//! candidates and a read, so a map of paths exercises every part of it, and
//! the tests say what the tree is instead of building one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use svirig_preproc::{ExpandedToken, Includes, Session, render};
use svirig_text::Reader;

/// A source tree, and the search path to look through it with.
#[derive(Default)]
struct Tree {
    files: HashMap<PathBuf, String>,
    quoted: Vec<PathBuf>,
    angle: Vec<PathBuf>,
}

impl Reader for Tree {
    fn read(&self, path: &Path) -> Option<String> {
        self.files.get(path).cloned()
    }
}

impl Tree {
    fn new() -> Tree {
        Tree::default()
    }

    fn file(mut self, path: &str, text: &str) -> Tree {
        self.files.insert(path.into(), text.to_string());
        self
    }

    fn quoted(mut self, dir: &str) -> Tree {
        self.quoted.push(dir.into());
        self
    }

    fn angle(mut self, dir: &str) -> Tree {
        self.angle.push(dir.into());
        self
    }

    /// Expands one of the files as the one named on the command line.
    fn expand(&self, path: &str) -> Expanded<'_> {
        let includes = Includes {
            quoted: self.quoted.clone(),
            angle: self.angle.clone(),
        };
        let mut session = Session::reading(self).searching(includes);
        let text = self.files[Path::new(path)].clone();
        let file = session.add(path, text);
        let tokens = session.expand(file);
        Expanded { session, tokens }
    }

    /// The expansion as one line, which is what most of these are about.
    fn text(&self, path: &str) -> String {
        self.expand(path).text()
    }
}

struct Expanded<'a> {
    session: Session<'a>,
    tokens: Vec<ExpandedToken>,
}

impl Expanded<'_> {
    fn text(&self) -> String {
        render(self.session.origins(), &self.tokens)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Every expanded token spelled exactly `text`.
    fn only(&self, text: &str) -> ExpandedToken {
        let found: Vec<_> = self
            .tokens
            .iter()
            .copied()
            .filter(|token| self.session.origins().slice(token.origin.spelled) == text)
            .collect();
        assert_eq!(found.len(), 1, "expected one `{text}`");
        found[0]
    }

    /// The files an `` `include `` chain passed through to reach a token,
    /// innermost first.
    fn through(&self, token: ExpandedToken) -> Vec<String> {
        self.session
            .origins()
            .include_trace(token.origin.spelled.file)
            .map(|site| {
                self.session
                    .origins()
                    .path(site.file)
                    .unwrap()
                    .display()
                    .to_string()
            })
            .collect()
    }
}

#[test]
fn an_included_file_is_spliced_in_where_the_directive_was() {
    let tree = Tree::new()
        .file("rtl/top.sv", "a;\n`include \"defs.svh\"\nb;\n")
        .file("rtl/defs.svh", "middle;\n");
    assert_eq!(tree.text("rtl/top.sv"), "a; middle; b;");
}

#[test]
fn a_quoted_name_is_found_next_to_the_file_that_used_it() {
    // 22.4's first rule, and the one nearly every header relies on.
    let tree = Tree::new()
        .file("rtl/top.sv", "`include \"defs.svh\"\n")
        .file("rtl/defs.svh", "near;\n")
        .file("defs.svh", "far;\n");
    assert_eq!(tree.text("rtl/top.sv"), "near;");
}

#[test]
fn a_quoted_name_falls_back_to_the_search_path() {
    let tree = Tree::new()
        .file("rtl/top.sv", "`include \"pkg/defs.svh\"\n")
        .file("include/pkg/defs.svh", "found;\n")
        .quoted("include");
    assert_eq!(tree.text("rtl/top.sv"), "found;");
}

#[test]
fn an_angle_name_searches_only_what_the_implementation_supplies() {
    // 22.4 reserves `<...>` for the tool's own files, so a header sitting next
    // to the source must not answer for one.
    let tree = Tree::new()
        .file("rtl/top.sv", "`include <uvm_macros.svh>\n")
        .file("rtl/uvm_macros.svh", "wrong;\n")
        .file("vendor/uvm_macros.svh", "right;\n")
        .angle("vendor");
    assert_eq!(tree.text("rtl/top.sv"), "right;");
}

#[test]
fn a_definition_crosses_the_include_in_both_directions() {
    // The header's macro is used below it, and the header uses one the file
    // above had already defined -- which a per-file table cannot answer.
    let tree = Tree::new()
        .file(
            "rtl/top.sv",
            "`define UNIT ns\n`include \"defs.svh\"\nlogic [`WIDTH-1:0] q;\n",
        )
        .file("rtl/defs.svh", "`timescale 1 `UNIT\n`define WIDTH 8\n");
    assert_eq!(tree.text("rtl/top.sv"), "logic [8-1:0] q;");
}

#[test]
fn a_macro_from_a_header_takes_its_arguments_from_the_file_below() {
    // The body is spelled in one file and the argument in another, which is
    // the case a token index alone cannot address.
    let tree = Tree::new()
        .file(
            "rtl/top.sv",
            "`include \"defs.svh\"\nassign y = `WRAP(p);\n",
        )
        .file("rtl/defs.svh", "`define WRAP(a) f(a)\n");
    let expanded = tree.expand("rtl/top.sv");
    assert_eq!(expanded.text(), "assign y = f(p);");

    // The body token is written in the header and the argument in the source,
    // and one expansion placed both.
    let body = expanded.only("f").origin;
    let argument = expanded.only("p").origin;
    assert_eq!(
        expanded.session.origins().path(body.spelled.file).unwrap(),
        Path::new("rtl/defs.svh")
    );
    assert_eq!(
        expanded
            .session
            .origins()
            .path(argument.spelled.file)
            .unwrap(),
        Path::new("rtl/top.sv")
    );
    assert_eq!(body.from, argument.from);
}

#[test]
fn a_token_from_a_header_traces_back_through_the_includes() {
    let tree = Tree::new()
        .file("rtl/top.sv", "`include \"a.svh\"\n")
        .file("rtl/a.svh", "`include \"b.svh\"\n")
        .file("rtl/b.svh", "deepest;\n");
    let expanded = tree.expand("rtl/top.sv");
    assert_eq!(expanded.text(), "deepest;");
    assert_eq!(
        expanded.through(expanded.only("deepest")),
        ["rtl/a.svh", "rtl/top.sv"]
    );
}

#[test]
fn a_name_carried_by_a_macro_resolves_where_the_macro_is_used() {
    // The `` `include `` is written in the header and executed in the source,
    // and the formal is a file name only once the body is substituted.
    let tree = Tree::new()
        .file(
            "rtl/top.sv",
            "`include \"pull.svh\"\n`PULL(\"parts/one.svh\")\n",
        )
        .file("rtl/pull.svh", "`define PULL(f) `include f\n")
        .file("rtl/parts/one.svh", "one;\n");
    assert_eq!(tree.text("rtl/top.sv"), "one;");
}

#[test]
fn a_stringified_formal_names_the_file() {
    // The shape the corpus actually uses: the argument is a bare name, and
    // `` `" `` turns it into the string literal the directive wants.
    let tree = Tree::new()
        .file(
            "rtl/top.sv",
            "`define include_file(f) `include `\"f`\"\n`include_file(defs.svh)\n",
        )
        .file("rtl/defs.svh", "pulled;\n");
    assert_eq!(tree.text("rtl/top.sv"), "pulled;");
}

#[test]
fn a_name_that_is_itself_a_macro_resolves() {
    let tree = Tree::new()
        .file(
            "rtl/top.sv",
            "`define DEFS \"defs.svh\"\n`include `DEFS\nq;\n",
        )
        .file("rtl/defs.svh", "p;\n");
    assert_eq!(tree.text("rtl/top.sv"), "p; q;");
}

#[test]
fn an_include_in_a_conditional_body_stops_at_the_name() {
    // Reading to the end of the line would take the `` `endif `` for part of
    // the file name.
    let body = "`define MAYBE(f) \\\n  `ifdef E \\\n    `include f \\\n  `endif\n";
    let call = "`MAYBE(\"defs.svh\")\n";

    let tree = Tree::new()
        .file("rtl/top.sv", &format!("`define E 1\n{body}{call}"))
        .file("rtl/defs.svh", "pulled;\n");
    assert_eq!(tree.text("rtl/top.sv"), "pulled;");

    // And the guard is a guard: with `E` undefined the file is never read.
    let tree = Tree::new()
        .file("rtl/top.sv", &format!("{body}{call}"))
        .file("rtl/defs.svh", "pulled;\n");
    assert_eq!(tree.text("rtl/top.sv"), "");
}

#[test]
fn a_name_that_resolves_to_nothing_expands_to_nothing() {
    // An error by 22.4, and silent until there is somewhere to report it.
    let tree = Tree::new().file("rtl/top.sv", "a;\n`include \"missing.svh\"\nb;\n");
    assert_eq!(tree.text("rtl/top.sv"), "a; b;");
}

#[test]
fn a_cycle_stops_instead_of_recursing() {
    let tree = Tree::new()
        .file("rtl/top.sv", "top;\n`include \"a.svh\"\n")
        .file("rtl/a.svh", "a;\n`include \"b.svh\"\n")
        .file("rtl/b.svh", "b;\n`include \"a.svh\"\n");
    assert_eq!(tree.text("rtl/top.sv"), "top; a; b;");
}

#[test]
fn a_file_that_includes_itself_stops() {
    let tree = Tree::new().file("rtl/top.sv", "once;\n`include \"top.sv\"\n");
    assert_eq!(tree.text("rtl/top.sv"), "once;");
}

#[test]
fn a_name_reaching_one_file_two_ways_is_still_one_file() {
    // `` `include "./a.svh" `` and `` `include "a.svh" `` are the same file, so
    // the second one is a cycle and not a second read.
    let tree = Tree::new()
        .file("rtl/top.sv", "`include \"./a.svh\"\n")
        .file("rtl/a.svh", "a;\n`include \"a.svh\"\n");
    assert_eq!(tree.text("rtl/top.sv"), "a;");
}

#[test]
fn a_name_climbing_out_of_its_directory_is_cleaned_not_collapsed() {
    // `..` cancels one segment. A path that is itself relative -- which is how
    // a corpus or a build tree is usually named -- must keep its own leading
    // `..`s rather than have them cancel each other.
    let tree = Tree::new()
        .file("../../rtl/top.sv", "`include \"../pkg/defs.svh\"\n")
        .file("../../pkg/defs.svh", "found;\n");
    assert_eq!(tree.text("../../rtl/top.sv"), "found;");
}

#[test]
fn nesting_reaches_the_depth_the_standard_requires() {
    // 22.4 asks for at least 15 levels.
    let mut tree = Tree::new().file("rtl/top.sv", "`include \"h0.svh\"\n");
    for level in 0..20 {
        tree = tree.file(
            &format!("rtl/h{level}.svh"),
            &format!("l{level};\n`include \"h{}.svh\"\n", level + 1),
        );
    }
    let text = tree.text("rtl/top.sv");
    assert!(text.starts_with("l0; l1;"), "{text}");
    assert!(text.ends_with("l19;"), "{text}");
}
