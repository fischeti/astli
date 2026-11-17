//! What a file's summary holds: its top-level declarations, the names it uses
//! that could mean one, and the headers it read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use astli_index::{Declares, Summary, SweepValues, Uses, summarize};
use astli_parse::parse;
use astli_preproc::{Build, MacroTable, Session};

/// Files held in memory, so a case can include one.
type Files = HashMap<PathBuf, String>;

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
    // module through the module around it, where an instance names it rather
    // than one at the top level. An `extern` declaration declares a
    // definition's ports, which its body stands in for.
    let summary = expanded(
        "package p;\n  class c; endclass\nendpackage\n\
         extern module m (input a);\n\
         module m (.*);\n  module inner; endmodule\n  inner u_inner ();\nendmodule\n\
         interface i;\n  inner u_inner ();\nendinterface\n\
         program pr; endprogram\n\
         class k; endclass\n\
         checker ch; endchecker\n\
         extern primitive u (output y, input a);\n\
         primitive u (.*);\n  table 0 : 1; endtable\nendprimitive\n",
    );
    assert_eq!(
        declared(&summary),
        [
            ("p", Declares::Package),
            ("m", Declares::Module),
            ("m", Declares::Module),
            ("i", Declares::Interface),
            ("pr", Declares::Program),
            ("k", Declares::Class),
            ("ch", Declares::Checker),
            ("u", Declares::Primitive),
            ("u", Declares::Primitive),
        ]
    );
    let prototypes = summary.declarations.iter().map(|d| d.prototype);
    assert_eq!(
        prototypes.collect::<Vec<_>>(),
        [false, true, false, false, false, false, false, true, false]
    );
    assert_eq!(used(&summary), [("inner", Uses::Instance)]);
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
    files.insert("inc/pkg.svh".into(), "package p;\nendpackage\n".into());
    let summary = expanded_with(
        &files,
        Build::new().include_dir("inc"),
        "`include \"pkg.svh\"\nmodule top;\nendmodule\n",
    );

    assert_eq!(
        declared(&summary),
        [("p", Declares::Package), ("top", Declares::Module)]
    );
    let header = Path::new("inc").join("pkg.svh");
    assert_eq!(
        summary.declarations[0].location.to_string(),
        format!("{}:1:9", header.display())
    );
    let [include] = summary.includes.as_slice() else {
        panic!("one include, got {:?}", summary.includes);
    };
    assert_eq!(include.path, PathBuf::from("inc/pkg.svh"));
    // The name searched for, which a compiler given `inc` finds too.
    assert_eq!(include.name.as_deref(), Some("pkg.svh"));
    assert_eq!(include.at.to_string(), "top.sv:1:1");
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
    // A module without its `endmodule` is left to the fallback, so it is
    // `VERBATIM`, and the instantiation in it is read off the tokens.
    let summary = expanded("module m;\n  sub #(.N(2)) u_sub (.*);\n");
    assert_eq!(used(&summary), [("sub", Uses::Unparsed)]);
}

#[test]
fn a_bind_instantiates_what_it_binds() {
    let summary = expanded(
        "module top;\nendmodule\nbind top checker_m #(.N(2)) u_chk (.*);\n\
         module top_bind;\n  bind top checker_m u_held (.*);\nendmodule\n",
    );
    assert_eq!(
        used(&summary),
        [
            ("top", Uses::Bind { held: false }),
            ("checker_m", Uses::Instance),
            ("top", Uses::Bind { held: true }),
            ("checker_m", Uses::Instance),
        ]
    );
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

#[test]
fn an_encrypted_envelope_is_flagged_and_hides_what_it_declares() {
    let summary = expanded(
        "module wrapper;\n  core u_core ();\nendmodule\n\
         `pragma protect begin_protected\n\
         `pragma protect data_block\n\
         bW9kdWxlIGNvcmUgKCk7IGVuZG1vZHVsZQ== module hidden (); endmodule\n\
         `pragma protect end_protected\n",
    );
    assert!(summary.encrypted);
    assert_eq!(declared(&summary), [("wrapper", Declares::Module)]);
    assert_eq!(used(&summary), [("core", Uses::Instance)]);

    // An envelope only waiting to be encrypted is plain code.
    let summary = expanded("`pragma protect begin\nmodule m;\nendmodule\n`pragma protect end\n");
    assert!(!summary.encrypted);
    assert_eq!(declared(&summary), [("m", Declares::Module)]);
}

#[test]
fn astli_test_marks_a_declaration_in_either_mode() {
    // Read as written, a value or a mark on what is no module included,
    // which is for the reader to refuse.
    let text = "(* astli_test *)\nmodule tb_a;\nendmodule\n\
                (* other, astli_test = 1 *) module tb_b; endmodule\n\
                module plain;\nendmodule\n\
                `ifdef FAST\n(* astli_test *) interface tb_i; endinterface\n`endif\n";
    let marks = |summary: &Summary| -> Vec<(String, Option<(String, bool)>)> {
        (summary.declarations.iter())
            .map(|d| {
                let test = (d.test.as_ref()).map(|t| (t.location.to_string(), t.valued));
                (d.name.clone(), test)
            })
            .collect()
    };
    let mark = |name: &str, at: Option<(&str, bool)>| {
        (
            name.to_string(),
            at.map(|(at, valued)| (at.to_string(), valued)),
        )
    };

    assert_eq!(
        marks(&expanded(text)),
        [
            mark("tb_a", Some(("top.sv:1:4", false))),
            mark("tb_b", Some(("top.sv:4:11", true))),
            mark("plain", None),
        ]
    );

    let mut session = Session::new();
    let file = session.add("top.sv", text.into());
    let parsed = parse(&session, file, MacroTable::new());
    assert_eq!(
        marks(&Summary::new(&session, file, &parsed)),
        [
            mark("tb_a", Some(("top.sv:1:4", false))),
            mark("tb_b", Some(("top.sv:4:11", true))),
            mark("plain", None),
            mark("tb_i", Some(("top.sv:8:4", false))),
        ]
    );
}

#[test]
fn astli_should_fail_reads_its_text_as_written() {
    // Without a string, or on what is no test, it is for the reader to
    // refuse; on a prototype, which has no body to run, it is not read.
    let text = "(* astli_test, astli_should_fail = \"[OVF]\" *)\nmodule tb_a;\nendmodule\n\
                (* astli_should_fail *) module tb_b; endmodule\n\
                (* astli_should_fail = 1 *) module tb_c; endmodule\n\
                (* astli_should_fail = \"x\" *) extern module tb_d;\n";
    let fails: Vec<_> = (expanded(text).declarations.iter())
        .map(|d| {
            let fail = (d.should_fail.as_ref()).map(|f| (f.location.to_string(), f.text.clone()));
            (d.name.clone(), fail)
        })
        .collect();
    let fail = |name: &str, at: Option<(&str, Option<&str>)>| {
        let at = at.map(|(at, text)| (at.to_string(), text.map(str::to_string)));
        (name.to_string(), at)
    };

    assert_eq!(
        fails,
        [
            fail("tb_a", Some(("top.sv:1:16", Some("[OVF]")))),
            fail("tb_b", Some(("top.sv:4:4", None))),
            fail("tb_c", Some(("top.sv:5:4", None))),
            fail("tb_d", None),
        ]
    );
}

#[test]
fn astli_sweep_marks_a_parameter_of_a_body_in_either_mode() {
    // Read as written, on a `localparam` or a declaration of two included;
    // one in a parameter port list, which the parser refuses, is not.
    let text = "module tb #((* astli_sweep *) parameter int P = 0);\n\
                \x20 (* astli_sweep = \"1, 2\" *) parameter int Depth = 4;\n\
                \x20 (* other *) (* astli_sweep *) localparam bit B = 0, C = 1;\n\
                \x20 (* astli_sweep = 3 *) parameter int N = 1;\n\
                `ifdef FAST\n  (* astli_sweep *) parameter bit F = 0;\n`endif\n\
                \x20 parameter int Plain = 0;\n\
                endmodule\n";
    let sweeps = |summary: &Summary| -> Vec<(String, Vec<String>, SweepValues)> {
        (summary.declarations.iter())
            .flat_map(|d| &d.sweeps)
            .map(|s| {
                (
                    s.location.to_string(),
                    s.parameters.clone(),
                    s.values.clone(),
                )
            })
            .collect()
    };
    let sweep = |at: &str, names: &[&str], values: SweepValues| {
        let names = names.iter().map(|it| it.to_string()).collect();
        (at.to_string(), names, values)
    };
    let written = [
        sweep("top.sv:2:6", &["Depth"], SweepValues::Listed("1, 2".into())),
        sweep("top.sv:3:18", &["B", "C"], SweepValues::Every),
        sweep("top.sv:4:6", &["N"], SweepValues::NotAString),
    ];

    assert_eq!(sweeps(&expanded(text)), written);

    let mut session = Session::new();
    let file = session.add("top.sv", text.into());
    let parsed = parse(&session, file, MacroTable::new());
    let mut every = written.to_vec();
    every.push(sweep("top.sv:6:6", &["F"], SweepValues::Every));
    assert_eq!(sweeps(&Summary::new(&session, file, &parsed)), every);
}
