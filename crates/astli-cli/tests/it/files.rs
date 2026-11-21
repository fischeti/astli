//! `files` and `pickle` over a filelist.

use std::path::PathBuf;

use crate::support::*;

/// A design whose top reaches a module through a macro in a header, a
/// package, and a module nothing needs.
fn design(fixture: &Fixture) -> PathBuf {
    fixture.file("top.sv", "module top;\n  core u_core ();\nendmodule\n");
    fixture.file(
        "core.sv",
        "`include \"defs.svh\"\nmodule core;\n  import cfg_pkg::*;\n  `INST(alu)\n  gone u_g ();\nendmodule\n",
    );
    fixture.file("inc/defs.svh", "`define INST(t) t u_``t ();\n");
    fixture.file("unused/other.svh", "");
    fixture.file("alu.sv", "module alu;\nendmodule\n");
    fixture.file("cfg_pkg.sv", "package cfg_pkg;\nendpackage\n");
    fixture.file("spare.sv", "module spare;\nendmodule\n");
    fixture.file(
        "design.f",
        "+incdir+inc\n+incdir+unused\n+define+X=1\ntop.sv\ncore.sv\nalu.sv\ncfg_pkg.sv\nspare.sv\n",
    )
}

#[test]
fn files_keeps_what_the_top_needs_in_dependency_order() {
    let fixture = Fixture::new("files-top");
    design(&fixture);

    let output = driver(&fixture).args([
        "files", "-f", "design.f", "--top", "top", "--order", "--diag", "short",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "+incdir+inc\n+define+X=1\ncfg_pkg.sv\nalu.sv\ncore.sv\ntop.sv\n"
    );
    // Declared in no file, which is worth saying but not failing over.
    assert!(
        stderr(&output).contains(
            "core.sv:5:3: warning[unknown-definition]: `gone` is instantiated and declared in \
             no file"
        ),
        "{}",
        stderr(&output)
    );

    // A string definition is written with its quotes escaped, as a
    // simulator reads them back.
    let output = driver(&fixture).args(["files", "-f", "design.f", "--top", "top", "-DS=\"s\""]);
    let text = stdout(&output);
    assert!(text.contains("+define+S=\\\"s\\\"\n"), "{text}");

    // A directory the filelist and a flag both give is searched, and so
    // written, once.
    let output = driver(&fixture).args([
        "files", "-f", "design.f", "-I", "inc", "-I", "inc", "--emit", "incdirs",
    ]);
    assert_eq!(stdout(&output), "+incdir+inc\n+incdir+unused\n");
}

#[test]
fn files_refuses_a_name_declared_twice_that_is_used() {
    let fixture = Fixture::new("files-redeclared-used");
    fixture.file("top.sv", "module top;\n  regfile u ();\nendmodule\n");
    fixture.file("ff.sv", "module regfile;\nendmodule\n");
    fixture.file("latch.sv", "module regfile;\nendmodule\n");

    let output = driver(&fixture).args(["files", "top.sv", "ff.sv", "latch.sv"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains(
            "[declared-twice] Error: `regfile` is declared 2 times and used, so which one it \
             means is unclear"
        ),
        "{said}"
    );
    assert!(said.contains("[ ff.sv:1:8 ]"), "{said}");
    assert!(said.contains("also declared at latch.sv:1:8"), "{said}");

    // A line for a tool holds no style, whatever colour stderr takes.
    let files = ["top.sv", "ff.sv", "latch.sv"];
    for (format, starts) in [("short", "ff.sv:1:8: "), ("github", "::error ")] {
        let output = driver(&fixture).args(
            [
                &["files"][..],
                &["--color", "always", "--diag", format],
                &files[..],
            ]
            .concat(),
        );
        let said = stderr(&output);
        let line = said.lines().find(|line| line.starts_with(starts));
        let line = line.unwrap_or_else(|| panic!("no {format} line: {said}"));
        assert!(line.contains("`regfile` is declared 2 times"), "{line}");
        assert!(!line.contains('\u{1b}'), "{line:?}");
    }
}

#[test]
fn files_warns_of_a_name_declared_twice_that_nothing_uses() {
    let fixture = Fixture::new("files-redeclared-unused");
    fixture.file("a/tb_top.sv", "module tb_top;\nendmodule\n");
    fixture.file("b/tb_top.sv", "module tb_top;\nendmodule\n");

    let unused = driver(&fixture).args(["files", "a/tb_top.sv", "b/tb_top.sv"]);
    let as_top = driver(&fixture).args(["files", "a/tb_top.sv", "b/tb_top.sv", "--top", "tb_top"]);

    assert!(unused.status.success(), "{}", stderr(&unused));
    assert!(stderr(&unused).contains("`tb_top` is declared 2 times and used nowhere"));
    // Named as a top, it is used.
    assert!(!as_top.status.success());
}

#[test]
fn files_without_a_top_keeps_every_file() {
    let fixture = Fixture::new("files-all");
    design(&fixture);

    let output = driver(&fixture).args(["files", "-f", "design.f", "--emit", "files"]);
    assert_eq!(
        stdout(&output),
        "top.sv\ncore.sv\nalu.sv\ncfg_pkg.sv\nspare.sv\n"
    );
}

#[test]
fn files_lists_what_could_be_a_top() {
    let fixture = Fixture::new("files-tops");
    design(&fixture);

    let output = driver(&fixture).args(["files", "-f", "design.f", "--emit", "tops"]);
    assert_eq!(stdout(&output), "top\nspare\n");
}

#[test]
fn files_says_why_a_top_needs_a_file() {
    let fixture = Fixture::new("files-why");
    design(&fixture);

    let output =
        driver(&fixture).args(["files", "-f", "design.f", "--top", "top", "--why", "alu.sv"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "top.sv\n\
         core.sv: declares `core`, used at top.sv:2:3\n\
         alu.sv: declares `alu`, used at core.sv:4:3\n"
    );

    let output = driver(&fixture).args([
        "files", "-f", "design.f", "--top", "top", "--why", "spare.sv",
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("the tops do not need it"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn files_fails_on_a_top_no_file_declares() {
    let fixture = Fixture::new("files-unknown");
    design(&fixture);

    let output = driver(&fixture).args(["files", "-f", "design.f", "--top", "tpo"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no file declares the top `tpo`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn files_fails_and_writes_no_list_when_a_file_it_keeps_does_not_parse() {
    let fixture = Fixture::new("files-parse-error");
    fixture.file("top.sv", "module top; sub u(); endmodule\n");
    fixture.file("sub.sv", "module sub; assign = ; endmodule\n");
    fixture.file("lone.sv", "module lone; endmodule\n");
    fixture.file("needy.sv", "module needy; gone u(); endmodule\n");

    let output = driver(&fixture).args(["files", "--top", "top", "top.sv", "sub.sv"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(stderr(&output).contains("sub.sv"), "{}", stderr(&output));
    assert!(
        stderr(&output).ends_with("error: 1 of 2 files has errors\n"),
        "{}",
        stderr(&output)
    );

    // A list of one file ends at what it says, as a run of one file does.
    let output = driver(&fixture).args(["files", "--top", "sub", "sub.sv", "--diag", "short"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "sub.sv:1:13: error[not-parsed]: not valid SystemVerilog\n"
    );

    // One the top does not reach cannot shorten the list, as `check` does
    // not report it either...
    let output = driver(&fixture).args(["files", "--top", "lone", "lone.sv", "sub.sv"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "lone.sv\n");
    assert!(!stderr(&output).contains("sub.sv"), "{}", stderr(&output));
    // A Code Quality report holds what the run tells, and only that.
    let output = driver(&fixture).args([
        "files",
        "--top",
        "lone",
        "lone.sv",
        "sub.sv",
        "--diag=gitlab",
    ]);
    assert_eq!(stderr(&output), "[]\n");

    // ...unless the top uses a name no file declares, which the region
    // that did not parse may.
    let output = driver(&fixture).args(["files", "--top", "needy", "needy.sv", "sub.sv"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(stderr(&output).contains("sub.sv"), "{}", stderr(&output));
}

#[test]
fn pickle_writes_what_the_top_needs_expanded_and_renamed() {
    let fixture = Fixture::new("pickle-top");
    design(&fixture);

    let output = driver(&fixture).args([
        "pickle",
        "-f",
        "design.f",
        "--top",
        "top",
        "--order",
        "--expand",
        "--prefix",
        "p_",
        "--exclude-rename",
        "top",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    // What a macro wrote is renamed, and `gone`, which no file declares, is
    // not.
    assert_eq!(
        stdout(&output),
        "package p_cfg_pkg;\nendpackage\n\
         module p_alu;\nendmodule\n\
         \n\nmodule p_core;\n  import p_cfg_pkg::*;\n  p_alu u_alu ();\n  gone u_g ();\nendmodule\n\
         module top;\n  p_core u_core ();\nendmodule\n"
    );
}

#[test]
fn pickle_keeps_the_directives_a_compiler_needs() {
    let fixture = Fixture::new("pickle-directives");
    let file = fixture.file(
        "top.sv",
        "`timescale 1ns/1ps\n`default_nettype none\nmodule top;\nendmodule\n`default_nettype wire\n",
    );

    let output = driver(&fixture).args(["pickle".as_ref(), "--expand".as_ref(), file.as_os_str()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "`timescale 1ns/1ps\n`default_nettype none\nmodule top;\nendmodule\n`default_nettype wire\n"
    );
}

#[test]
fn raw_pickle_inlines_headers_and_starts_each_file_afresh() {
    let fixture = Fixture::new("pickle-raw");
    design(&fixture);

    let output = driver(&fixture).args(["pickle", "-f", "design.f", "--top", "top", "--order"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "`define X 1\npackage cfg_pkg;\nendpackage\n`undefineall\n\
         `define X 1\nmodule alu;\nendmodule\n`undefineall\n\
         `define X 1\n`define INST(t) t u_``t ();\n\n\
         module core;\n  import cfg_pkg::*;\n  `INST(alu)\n  gone u_g ();\nendmodule\n`undefineall\n\
         `define X 1\nmodule top;\n  core u_core ();\nendmodule\n`undefineall\n"
    );
}

#[test]
fn raw_pickle_inlines_a_header_in_a_branch_the_build_does_not_take() {
    let fixture = Fixture::new("pickle-raw-branch");
    fixture.file("sim.svh", "`define WHERE sim\n");
    fixture.file("syn.svh", "`define WHERE syn\n");
    let file = fixture.file(
        "top.sv",
        "`ifdef SIM\n`include \"sim.svh\"\n`else\n`include \"syn.svh\"\n`endif\nmodule top;\nendmodule\n",
    );

    let output = driver(&fixture).args([
        "pickle".as_ref(),
        "-I".as_ref(),
        fixture.path().as_os_str(),
        file.as_os_str(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "`ifdef SIM\n`define WHERE sim\n\n`else\n`define WHERE syn\n\n`endif\nmodule top;\nendmodule\n`undefineall\n"
    );
}

#[test]
fn raw_pickle_warns_of_an_include_it_leaves_as_a_directive() {
    let fixture = Fixture::new("pickle-raw-left");
    // In a branch the build does not take, so only the pickle sees it.
    fixture.file("x.svh", "");
    fixture.file("loop.svh", "`ifdef NEVER\n`include \"loop.svh\"\n`endif\n");
    let file = fixture.file(
        "top.sv",
        "`define H \"x.svh\"\n`include `H\n`include \"loop.svh\"\nmodule top;\nendmodule\n",
    );
    let run = |diag: &str| {
        driver(&fixture).args([
            "--diag".as_ref(),
            diag.as_ref(),
            "pickle".as_ref(),
            "-I".as_ref(),
            fixture.path().as_os_str(),
            file.as_os_str(),
        ])
    };

    let output = run("pretty");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stderr(&output);
    assert!(
        text.contains("Warning: a macro names this header"),
        "{text}"
    );
    assert!(
        text.contains("Warning: `loop.svh` includes itself"),
        "{text}"
    );
    assert!(!text.contains("astli:"), "{text}");

    let output = run("json");
    let lines: Vec<String> = stderr(&output).lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].contains(r#""code":"left-macro-include""#),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].contains(r#""code":"left-self-include""#),
        "{}",
        lines[1]
    );
    assert!(lines.iter().all(|line| line.starts_with('{')), "{lines:?}");
}

#[test]
fn pickle_fails_on_a_file_with_errors_as_files_does() {
    let fixture = Fixture::new("pickle-errors");
    fixture.file("bad.sv", "module bad(;\nendmodule\n");
    fixture.file("ok.sv", "module ok;\nendmodule\n");
    for diag in ["pretty", "json"] {
        let run = |command: &str| {
            driver(&fixture).args([
                "--diag".as_ref(),
                diag.as_ref(),
                command.as_ref(),
                fixture.path().join("bad.sv").as_os_str(),
                fixture.path().join("ok.sv").as_os_str(),
            ])
        };
        let (pickle, files) = (run("pickle"), run("files"));
        assert_eq!(pickle.status.code(), Some(1), "{}", stderr(&pickle));
        assert_eq!(pickle.status.code(), files.status.code());
        assert_eq!(stderr(&pickle), stderr(&files));
    }
}

#[test]
fn raw_pickle_renames_nothing() {
    let fixture = Fixture::new("pickle-raw-rename");
    design(&fixture);

    let output = driver(&fixture).args(["pickle", "-f", "design.f", "--prefix", "p_"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--expand"), "{}", stderr(&output));
}

/// A design whose IP is encrypted but for the package it imports.
fn encrypted(fixture: &Fixture) {
    fixture.file("top.sv", "module top;\n  vendor_ip u_ip ();\nendmodule\n");
    fixture.file(
        "vendor_ip.sv",
        "import ip_pkg::*;\n`pragma protect begin_protected\n`pragma protect data_block\n\
         bW9kdWxlIHZlbmRvcl9pcCAoKTsgaXBfcGtnOjpXOyBlbmRtb2R1bGU= u (\n\
         `pragma protect end_protected\n",
    );
    fixture.file("ip_pkg.sv", "package ip_pkg;\nendpackage\n");
    fixture.file("spare.sv", "module spare;\nendmodule\n");
}

#[test]
fn files_keeps_an_encrypted_file_and_what_it_needs() {
    let fixture = Fixture::new("files-encrypted");
    encrypted(&fixture);

    let output = driver(&fixture).args([
        "files",
        "top.sv",
        "vendor_ip.sv",
        "ip_pkg.sv",
        "spare.sv",
        "--top",
        "top",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "top.sv\nvendor_ip.sv\nip_pkg.sv\n");
    assert!(
        stderr(&output).contains("vendor_ip.sv: encrypted, so it is kept"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn files_leaves_out_a_stub_for_cells_a_library_supplies() {
    let fixture = Fixture::new("files-stub");
    fixture.file(
        "top.sv",
        "module top;\n  logic a, b, y;\n  AND2_X1 u_and (.A(a), .B(b), .Y(y));\n  \
         OR2_X1 u_or (.A(a), .B(b), .Y(y));\nendmodule\n",
    );
    fixture.file(
        "cells.sv",
        "extern module AND2_X1 (input logic A, input logic B, output logic Y);\n",
    );
    // Not a stub: it declares a type too.
    fixture.file(
        "mixed.sv",
        "extern module OR2_X1 (input logic A, input logic B, output logic Y);\n\
         typedef logic bit_t;\n",
    );
    let files = ["top.sv", "cells.sv", "mixed.sv"];

    for top in [&["--top", "top"][..], &[]] {
        let output =
            driver(&fixture).args([&["files"][..], &files[..], top, &["--emit", "files"]].concat());
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(stdout(&output), "top.sv\nmixed.sv\n");
        assert!(
            stderr(&output)
                .contains("cells.sv: `extern` declarations alone, so it is left out of the list"),
            "{}",
            stderr(&output)
        );
    }
    let quiet =
        driver(&fixture).args([&["files"][..], &files[..], &["-q", "--top", "top"]].concat());
    assert_eq!(stdout(&quiet), "top.sv\nmixed.sv\n");
    assert!(stderr(&quiet).is_empty(), "{}", stderr(&quiet));

    // With the body listed too, the `extern` may be what its `.*` reads.
    fixture.file(
        "and2.sv",
        "module AND2_X1 (.*);\n  assign Y = A & B;\nendmodule\n",
    );
    let output = driver(&fixture)
        .args([&["files"][..], &files[..], &["and2.sv", "--emit", "files"]].concat());
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "top.sv\ncells.sv\nmixed.sv\nand2.sv\n");
}

#[test]
fn pickle_writes_an_envelope_back_as_it_is_and_renames_nothing_in_it() {
    let fixture = Fixture::new("pickle-encrypted");
    encrypted(&fixture);

    let output = driver(&fixture).args([
        "pickle",
        "top.sv",
        "vendor_ip.sv",
        "ip_pkg.sv",
        "--top",
        "top",
        "--expand",
        "--prefix",
        "p_",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let envelope = std::fs::read_to_string(fixture.path().join("vendor_ip.sv")).unwrap();
    let envelope = envelope.replace("import ip_pkg", "import p_ip_pkg");
    assert_eq!(
        stdout(&output),
        format!(
            "module p_top;\n  vendor_ip u_ip ();\nendmodule\n{envelope}package p_ip_pkg;\nendpackage\n"
        )
    );
    assert!(
        stderr(&output).contains("breaks if the name is renamed"),
        "{}",
        stderr(&output)
    );
}
