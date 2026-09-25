//! What a file's summary holds: its top-level declarations, the names it uses
//! that could mean one, and the headers it read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use astli_index::{Declares, Summary, Uses, summarize};
use astli_parse::parse;
use astli_preproc::{Build, MacroTable, Session};
use astli_text::Reader;

/// Files held in memory, so a case can include one.
#[derive(Default)]
struct Files(HashMap<PathBuf, String>);

impl Reader for Files {
    fn read(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }
}

fn expanded(text: &str) -> Summary {
    expanded_with(&Files::default(), Build::new(), text)
}

fn expanded_with(files: &Files, build: Build, text: &str) -> Summary {
    let mut session = Session::reading(files).building(build);
    let file = session.add("top.sv", text.to_string());
    summarize(&mut session, file).0
}

fn declared(summary: &Summary) -> Vec<(&str, Declares)> {
    let declarations = summary.declarations.iter();
    declarations
        .map(|d| (d.name.as_str(), d.declares))
        .collect()
}

fn used(summary: &Summary) -> Vec<(&str, Uses)> {
    let references = summary.references.iter();
    references.map(|r| (r.name.as_str(), r.uses)).collect()
}

#[test]
fn only_what_is_declared_at_the_top_level_counts() {
    // A class in a package is reached through the package, and a nested
    // module through the module around it.
    let summary = expanded(
        "package p;\n  class c; endclass\nendpackage\n\
         module m;\n  module inner; endmodule\nendmodule\n\
         interface i; endinterface\n\
         program pr; endprogram\n\
         class k; endclass\n",
    );
    assert_eq!(
        declared(&summary),
        [
            ("p", Declares::Package),
            ("m", Declares::Module),
            ("i", Declares::Interface),
            ("pr", Declares::Program),
            ("k", Declares::Class),
        ]
    );
}

#[test]
fn each_place_a_name_could_mean_a_declaration_is_a_reference() {
    let summary = expanded(
        "module top (bus_if.slave s, input axi_pkg::req_t req);\n\
         \x20 import util_pkg::clog2;\n\
         \x20 virtual bus_if vif;\n\
         \x20 core #(.W(8)) u_core ();\n\
         \x20 assign x = math_pkg::f(y);\n\
         \x20 class foo extends base_c; endclass\n\
         endmodule\n",
    );
    assert_eq!(
        used(&summary),
        [
            ("bus_if", Uses::Type),
            ("axi_pkg", Uses::Scope),
            ("util_pkg", Uses::Import),
            ("bus_if", Uses::Type),
            ("core", Uses::Instance),
            ("math_pkg", Uses::Scope),
            ("base_c", Uses::Type),
        ]
    );
}

#[test]
fn a_scoped_name_references_only_what_opens_it() {
    let summary = expanded("module top;\n  assign x = a_pkg::b_cls::c;\nendmodule\n");
    assert_eq!(used(&summary), [("a_pkg", Uses::Scope)]);
}

#[test]
fn what_a_macro_writes_is_referenced_where_it_was_called() {
    let summary = expanded("`define INST(t) t u_i ();\nmodule top;\n  `INST(core)\nendmodule\n");
    assert_eq!(used(&summary), [("core", Uses::Instance)]);
    let location = &summary.references[0].location;
    assert_eq!(location.to_string(), "top.sv:3:3");
}

#[test]
fn a_header_declares_for_the_file_that_includes_it() {
    let mut files = Files::default();
    files
        .0
        .insert("inc/pkg.svh".into(), "package p;\nendpackage\n".into());
    let summary = expanded_with(
        &files,
        Build::new().include_dir("inc"),
        "`include \"pkg.svh\"\nmodule top;\nendmodule\n",
    );

    assert_eq!(
        declared(&summary),
        [("p", Declares::Package), ("top", Declares::Module)]
    );
    assert_eq!(
        summary.declarations[0].location.to_string(),
        "inc/pkg.svh:1:9"
    );
    assert_eq!(summary.includes, [PathBuf::from("inc/pkg.svh")]);
}

#[test]
fn only_the_branch_a_build_takes_is_read() {
    let text = "module top;\n`ifdef FAST\n  fast u ();\n`else\n  slow u ();\n`endif\nendmodule\n";
    let files = Files::default();
    let fast = expanded_with(&files, Build::new().define("FAST", ""), text);
    assert_eq!(used(&fast), [("fast", Uses::Instance)]);
    assert_eq!(used(&expanded(text)), [("slow", Uses::Instance)]);
}

#[test]
fn a_raw_tree_reads_every_branch_and_no_macro() {
    let mut session = Session::new();
    let text = "`ifdef FAST\nmodule fast; endmodule\n`else\nmodule slow; endmodule\n`endif\n";
    let file = session.add("top.sv", text.into());
    let parsed = parse(&session, file, MacroTable::new());
    let summary = Summary::new(&session, file, &parsed);
    assert_eq!(
        declared(&summary),
        [("fast", Declares::Module), ("slow", Declares::Module)]
    );
}

#[test]
fn an_escaped_name_is_the_name() {
    let summary = expanded("module \\top ;\n  \\core u ();\nendmodule\n");
    assert_eq!(declared(&summary), [("top", Declares::Module)]);
    assert_eq!(used(&summary), [("core", Uses::Instance)]);
}

#[test]
fn an_instantiation_in_unparsed_text_is_still_found() {
    // `bind` has no rule, so it is `VERBATIM`, and its instantiation is
    // read off the tokens.
    let summary = expanded("module top;\nendmodule\nbind top checker_m #(.N(2)) u_chk (.*);\n");
    assert_eq!(used(&summary), [("checker_m", Uses::Unparsed)]);
}

#[test]
fn a_declaration_in_unparsed_text_is_still_found() {
    // Two attribute instances in a row leave the module to the fallback,
    // and the declaration must not go with it.
    let summary = expanded(
        "(* a *)\n(* b *)\nmodule m;\n  virtual interface bus_if vif;\n  typedef class c;\nendmodule\n",
    );
    assert_eq!(declared(&summary), [("m", Declares::Module)]);
}
