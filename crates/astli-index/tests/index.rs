//! What an index answers about a set of files: which a top needs, in what
//! order, and what is declared twice or nowhere.

use astli_index::{Index, UnknownTop, summarize};
use astli_preproc::Session;

/// An index over files given as `(path, text)`, each its own unit.
fn index(files: &[(&str, &str)]) -> Index {
    let summaries = files.iter().map(|(path, text)| {
        let mut session = Session::new();
        let file = session.add(*path, text.to_string());
        summarize(&mut session, file).0
    });
    Index::new(summaries.collect())
}

fn paths(index: &Index, files: &[usize]) -> Vec<String> {
    let path = |&file: &usize| index.summaries()[file].path.display().to_string();
    files.iter().map(path).collect()
}

const DESIGN: &[(&str, &str)] = &[
    (
        "top.sv",
        "module top;\n  core u_core ();\n  periph u_periph ();\nendmodule\n",
    ),
    (
        "core.sv",
        "module core;\n  import cfg_pkg::*;\n  alu u_alu ();\nendmodule\n",
    ),
    (
        "alu.sv",
        "module alu;\n  assign x = cfg_pkg::W;\nendmodule\n",
    ),
    (
        "cfg_pkg.sv",
        "package cfg_pkg;\n  localparam int W = 8;\nendpackage\n",
    ),
    ("periph.sv", "module periph;\nendmodule\n"),
    ("tb.sv", "module tb;\n  top dut ();\nendmodule\n"),
    ("spare.sv", "module spare;\nendmodule\n"),
];

#[test]
fn a_top_needs_what_it_reaches_and_nothing_else() {
    let index = index(DESIGN);
    let needed = index.reachable(&["top"]).unwrap();
    assert_eq!(
        paths(&index, &needed),
        ["top.sv", "core.sv", "alu.sv", "cfg_pkg.sv", "periph.sv"]
    );
    assert_eq!(index.reachable(&["top", "spare"]).unwrap().len(), 6);
}

#[test]
fn a_top_no_file_declares_is_an_error() {
    let index = index(DESIGN);
    assert_eq!(
        index.reachable(&["tpo"]),
        Err(UnknownTop("tpo".to_string()))
    );
}

#[test]
fn a_file_comes_after_the_files_it_depends_on() {
    let index = index(DESIGN);
    let needed = index.reachable(&["top"]).unwrap();
    assert_eq!(
        paths(&index, &index.ordered(&needed)),
        ["cfg_pkg.sv", "alu.sv", "core.sv", "periph.sv", "top.sv"]
    );
}

#[test]
fn files_that_depend_on_each_other_keep_the_order_given() {
    let index = index(&[
        ("a.sv", "module a;\n  b u ();\nendmodule\n"),
        ("b.sv", "module b;\n  a u ();\nendmodule\n"),
    ]);
    assert_eq!(paths(&index, &index.ordered(&[0, 1])), ["b.sv", "a.sv"]);
    assert_eq!(paths(&index, &index.ordered(&[1, 0])), ["a.sv", "b.sv"]);
}

#[test]
fn a_candidate_top_is_a_module_nothing_references() {
    let index = index(DESIGN);
    let tops: Vec<&str> = index.tops().iter().map(|(_, d)| d.name.as_str()).collect();
    assert_eq!(tops, ["tb", "spare"]);
}

#[test]
fn why_a_file_is_needed_is_the_chain_from_a_top() {
    let index = index(DESIGN);
    let chain = index.why(&["tb"], 2).unwrap().unwrap();
    let steps: Vec<(String, Option<&str>)> = chain
        .iter()
        .map(|step| {
            let path = index.summaries()[step.file].path.display().to_string();
            (path, step.via.map(|reference| reference.name.as_str()))
        })
        .collect();
    assert_eq!(
        steps,
        [
            ("tb.sv".to_string(), None),
            ("top.sv".to_string(), Some("top")),
            ("core.sv".to_string(), Some("core")),
            ("alu.sv".to_string(), Some("alu")),
        ]
    );
    assert!(index.why(&["top"], 6).unwrap().is_none());
}

#[test]
fn an_instance_or_import_declared_nowhere_is_reported_once() {
    let index = index(&[(
        "a.sv",
        "module a;\n  import gone_pkg::*;\n  import std::*;\n  gone u1 ();\n  gone u2 ();\n  local_t x;\nendmodule\n",
    )]);
    let missing: Vec<String> = index
        .undeclared(&[0])
        .iter()
        .map(|(_, reference)| format!("{} {}", reference.name, reference.location))
        .collect();
    // A type's head may be a local typedef, and `std` is the language's.
    assert_eq!(missing, ["gone_pkg a.sv:2:10", "gone a.sv:4:3"]);
}

#[test]
fn a_name_declared_twice_resolves_to_the_later() {
    let index = index(&[
        ("old.sv", "module fifo;\nendmodule\n"),
        ("new.sv", "module fifo;\nendmodule\n"),
        ("top.sv", "module top;\n  fifo u ();\nendmodule\n"),
    ]);
    let redeclared = &index.redeclared(&[0, 1, 2])[0];
    let locations: Vec<String> = (redeclared.declarations.iter())
        .map(|declaration| declaration.location.to_string())
        .collect();
    assert_eq!(locations, ["old.sv:1:8", "new.sv:1:8"]);
    assert!(redeclared.referenced);
    assert_eq!(
        paths(&index, &index.reachable(&["top"]).unwrap()),
        ["new.sv", "top.sv"]
    );
}

#[test]
fn an_encrypted_file_is_reached_with_what_it_needs_whatever_the_tops() {
    // Its module is inside the envelope, so no reference reaches it, but the
    // package its plaintext imports is still needed.
    let index = index(&[
        ("top.sv", "module top;\n  vendor_ip u_ip ();\nendmodule\n"),
        (
            "vendor_ip.sv",
            "import ip_pkg::*;\n`pragma protect begin_protected\n`pragma protect data_block\n\
             bW9kdWxlIHZlbmRvcl9pcA==\n`pragma protect end_protected\n",
        ),
        ("ip_pkg.sv", "package ip_pkg;\nendpackage\n"),
        ("spare.sv", "module spare;\nendmodule\n"),
    ]);
    let needed = index.reachable(&["top"]).unwrap();
    assert_eq!(
        paths(&index, &needed),
        ["top.sv", "vendor_ip.sv", "ip_pkg.sv"]
    );
}

#[test]
fn every_declaration_of_a_name_is_one_redeclaration() {
    let index = index(&[
        ("ff.sv", "module regfile;\nendmodule\n"),
        ("latch.sv", "module regfile;\nendmodule\n"),
        ("fpga.sv", "module regfile;\nendmodule\n"),
    ]);
    let redeclared = index.redeclared(&[0, 1, 2]);
    assert_eq!(redeclared.len(), 1);
    assert_eq!(redeclared[0].name, "regfile");
    assert_eq!(redeclared[0].declarations.len(), 3);
    assert!(!redeclared[0].referenced);
}

#[test]
fn a_redeclaration_matters_only_to_the_files_that_declare_or_use_it() {
    let index = index(&[
        ("a/tb_top.sv", "module tb_top;\nendmodule\n"),
        ("b/tb_top.sv", "module tb_top;\nendmodule\n"),
        ("core.sv", "module core;\nendmodule\n"),
        ("top.sv", "module top;\n  core u ();\nendmodule\n"),
    ]);
    // What `top` needs neither declares nor uses `tb_top`.
    assert!(index.redeclared(&[2, 3]).is_empty());
    // One of the copies, kept, still declares it.
    let redeclared = index.redeclared(&[0, 2, 3]);
    assert_eq!(redeclared.len(), 1);
    assert!(!redeclared[0].referenced);
}
