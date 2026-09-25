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
    let (earlier, later) = index.redeclared()[0];
    assert_eq!(earlier.location.to_string(), "old.sv:1:8");
    assert_eq!(later.location.to_string(), "new.sv:1:8");
    assert_eq!(
        paths(&index, &index.reachable(&["top"]).unwrap()),
        ["new.sv", "top.sv"]
    );
}
