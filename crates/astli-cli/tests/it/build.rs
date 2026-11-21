//! `build`, `run` and `clean`.

use std::process::Command;

use crate::support::*;

#[test]
fn verilator_build_and_test_warn_once_of_each_list() {
    let fixture = Fixture::new("verilator-include-differs");
    fixture.file(
        "root/astli.toml",
        "[package]\ninclude-dirs = ['include']\n\n[dependencies]\nip = { path = '../ip' }\n",
    );
    // Two points, which read one list.
    fixture.file(
        "root/tb_top.sv",
        "`include \"defs.svh\"\n(* astli_test *)\nmodule tb_top;\n\
         \x20 (* astli_sweep = \"1, 2\" *) parameter int N = 1;\n  leaf u_leaf ();\n\
         \x20 initial $finish;\nendmodule\n",
    );
    fixture.file("root/include/defs.svh", "`define W 8\n");
    fixture.file(
        "ip/src/leaf.sv",
        "`include \"defs.svh\"\nmodule leaf;\nendmodule\n",
    );
    fixture.file("ip/include/defs.svh", "`define W 4\n");
    let root = fixture.path().join("root");

    for args in [&["build", "--top", "tb_top[N=1]"][..], &["test"]] {
        let output = driver(&fixture).dir(&root).cache().args(args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
        let said = stderr(&output).replace('\\', "/");
        assert!(
            said.contains("leaf.sv:1:1") && said.contains("root/include/defs.svh` for `defs.svh`"),
            "{args:?}: {said}"
        );
        assert_eq!(
            said.matches("[include-differs]").count(),
            1,
            "{args:?}: {said}"
        );
    }
}

/// A package whose testbench `tb` adds two numbers through a header's
/// macro at the width its `[top.tb]` defines, one of them the parameter
/// the table sets, and fails if the sum is wrong.
fn adder(fixture: &Fixture) {
    adder_in(fixture, "");
}

#[test]
fn build_needs_a_top() {
    let fixture = Fixture::new("build-no-top");
    fixture.file("astli.toml", "");
    fixture.file("rtl/core.sv", "module core;\nendmodule\n");

    let output = driver(&fixture).args(["build"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no top to build"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn verilator_build_writes_the_file_list_and_the_executable() {
    let fixture = Fixture::new("verilator-build");
    adder(&fixture);

    let output = driver(&fixture).args(["build"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let dir = fixture.path().join(".astli/build/verilator/dev/adder/tb");
    let list = std::fs::read_to_string(dir.join("files.f")).expect("a file list");
    let root = fixture.path().canonicalize().expect("a canonical root");
    let list = list
        .replace(&root.to_string_lossy().into_owned(), "<dir>")
        .replace(&fixture.path().to_string_lossy().into_owned(), "<dir>")
        .replace('\\', "/");
    assert_eq!(
        list,
        "+incdir+<dir>/include\n+define+WIDTH=8\n<dir>/rtl/adder.sv\n<dir>/tb/tb.sv\n"
    );
    let ignored = std::fs::read_to_string(fixture.path().join(".astli/.gitignore"));
    assert_eq!(ignored.expect("a .gitignore"), "*\n");
    let ran = Command::new(dir.join("Vtb"))
        .output()
        .expect("the build runs");
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert!(stdout(&ran).contains("sum 300"), "{}", stdout(&ran));
}

#[test]
fn verilator_build_stops_at_what_check_finds() {
    let fixture = Fixture::new("verilator-build-check");
    adder(&fixture);
    let tb = std::fs::read_to_string(fixture.path().join("tb/tb.sv")).expect("the testbench");
    fixture.file("tb/tb.sv", &tb.replace("adder #", "addr #"));

    let output = driver(&fixture).args(["build"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("`addr`"), "{}", stderr(&output));
    assert!(!fixture.path().join(".astli/build").exists());
}

#[test]
fn verilator_build_fails_on_an_elaboration_error() {
    let fixture = Fixture::new("verilator-build-elab-error");
    adder(&fixture);
    let adder = std::fs::read_to_string(fixture.path().join("rtl/adder.sv")).expect("the adder");
    fixture.file(
        "rtl/adder.sv",
        &adder.replace(
            "endmodule",
            "  if (W > 4) begin : gen_wide\n    $error(\"W must be at most 4\");\n  end\nendmodule",
        ),
    );

    let output = driver(&fixture).args(["build"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(said.contains("W must be at most 4"), "{said}");
    let dir = fixture.path().join(".astli/build/verilator/dev/adder/tb");
    assert!(!dir.join("Vtb").exists());
}

#[test]
fn verilator_build_is_fresh_until_what_it_reads_changes() {
    let fixture = Fixture::new("verilator-build-fresh");
    adder(&fixture);

    let first = driver(&fixture).args(["build"]);
    let again = driver(&fixture).args(["build", "-j", "2"]);
    fixture.file("include/sum.svh", "`define SUM(a, b) (b + a)\n");
    let header = driver(&fixture).args(["build"]);
    let define = driver(&fixture).args(["build", "-D", "EXTRA"]);
    let param = driver(&fixture).args(["build", "-G", "B=50"]);
    let trace = driver(&fixture).args(["build", "-G", "B=50", "--trace"]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert!(
        stderr(&first).contains("Building `tb`"),
        "{}",
        stderr(&first)
    );
    // How many jobs build it changes nothing it builds.
    assert!(!stderr(&again).contains("Building"), "{}", stderr(&again));
    assert!(
        stderr(&header).contains("Building `tb`"),
        "{}",
        stderr(&header)
    );
    assert!(
        stderr(&define).contains("Building `tb`"),
        "{}",
        stderr(&define)
    );
    assert!(
        stderr(&param).contains("Building `tb`"),
        "{}",
        stderr(&param)
    );
    assert!(
        stderr(&trace).contains("Building `tb`"),
        "{}",
        stderr(&trace)
    );
}

#[test]
fn verilator_run_finds_a_header_beside_the_file_that_includes_it() {
    let fixture = Fixture::new("verilator-include-beside");
    include_beside(&fixture);

    let output = driver(&fixture).args(["run"]);
    // As though the last build ended just now: what Verilator rewrites is
    // then no newer than the objects to a Make that compares whole seconds.
    let ahead = std::time::SystemTime::now() + std::time::Duration::from_millis(900);
    let dir = fixture.path().join(".astli/build/verilator/dev/beside/tb");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let file = std::fs::File::options()
            .write(true)
            .open(entry.unwrap().path());
        file.unwrap().set_modified(ahead).unwrap();
    }
    fixture.file("tb/defs.svh", "`define VAL 8\n");
    let changed = driver(&fixture).args(["run"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("val 7"), "{}", stdout(&output));
    assert!(changed.status.success(), "{}", stderr(&changed));
    assert!(stdout(&changed).contains("val 8"), "{}", stdout(&changed));
}

#[test]
fn verilator_reads_a_file_a_profile_names_from_the_manifest_and_rebuilds_when_it_changes() {
    let fixture = Fixture::new("verilator-profile-file");
    fixture.file(
        "astli.toml",
        "[package]\nname = \"dpi\"\n\n[top.tb]\n\n\
         [profile.dev.verilator]\nargs = [\"model/model.cc\"]\n",
    );
    fixture.file("model/model.cc", "extern \"C\" int f() { return 1; }\n");
    fixture.file(
        "tb/tb.sv",
        "module tb;\n\
         \x20 import \"DPI-C\" function int f();\n\
         \x20 initial begin\n\
         \x20   $display(\"f %0d\", f());\n\
         \x20   $finish;\n\
         \x20 end\n\
         endmodule\n",
    );

    // From a directory below the manifest, which the path is not relative
    // to.
    let first = driver(&fixture)
        .dir(fixture.path().join("tb"))
        .args(["run"]);
    fixture.file("model/model.cc", "extern \"C\" int f() { return 2; }\n");
    let changed = driver(&fixture)
        .dir(fixture.path().join("tb"))
        .args(["run"]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("f 1"), "{}", stdout(&first));
    assert!(changed.status.success(), "{}", stderr(&changed));
    assert!(
        stderr(&changed).contains("Building `tb`"),
        "{}",
        stderr(&changed)
    );
    assert!(stdout(&changed).contains("f 2"), "{}", stdout(&changed));
}

#[test]
fn verilator_run_passes_what_follows_and_exits_as_the_simulation_does() {
    let fixture = Fixture::new("verilator-run");
    // Each flop's initial value is drawn only when the run asks for it.
    fixture.file(
        "astli.toml",
        "[package]\nname = \"bench\"\n\n[top.tb]\n\n\
         [profile.dev.verilator]\n\
         args = [\"--x-initial\", \"unique\"]\n\
         run-args = [\"+verilator+rand+reset+2\"]\n",
    );
    fixture.file(
        "tb/tb.sv",
        "module tb;\n\
         \x20 int n;\n\
         \x20 logic [31:0] r;\n\
         \x20 initial begin\n\
         \x20   void'($value$plusargs(\"N=%d\", n));\n\
         \x20   $display(\"n %0d\", n);\n\
         \x20   $display(\"random %0d\", r != 0);\n\
         \x20   if ($test$plusargs(\"define+X\")) $display(\"plusarg\");\n\
         \x20   if (n > 5) $fatal(1, \"too big\");\n\
         \x20   $finish;\n\
         \x20 end\n\
         endmodule\n",
    );

    let passed = driver(&fixture).args(["run", "--", "+N=3", "+define+X"]);
    let failed = driver(&fixture).args(["run", "--", "+N=9", "+verilator+rand+reset+0"]);

    assert!(passed.status.success(), "{}", stderr(&passed));
    // A filelist's `+define+` after `--` is the simulation's plusarg.
    assert!(
        stdout(&passed).contains("n 3\nrandom 1\nplusarg\n"),
        "{}",
        stdout(&passed)
    );
    // A simulator's option after `--` wins over the profile's.
    assert!(stdout(&failed).contains("random 0"), "{}", stdout(&failed));
    assert!(
        stderr(&passed).contains("Running `.astli/build/verilator/dev/bench/tb/Vtb`"),
        "{}",
        stderr(&passed)
    );
    assert!(!stderr(&failed).contains("Building"), "{}", stderr(&failed));
    assert!(!failed.status.success());
    // What failed, the simulation said.
    assert!(!stderr(&failed).contains("error:"), "{}", stderr(&failed));
}

#[test]
fn clean_deletes_the_builds_and_keeps_the_patches() {
    let fixture = Fixture::new("clean");
    fixture.file("astli.toml", "");
    fixture.file(".astli/patch.toml", "[patch]\n");
    fixture.file(".astli/build/verilator/dev/clean/tb/Vtb", "");

    let output = driver(&fixture).args(["clean"]);
    let again = driver(&fixture).args(["clean"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!fixture.path().join(".astli/build").exists());
    assert!(fixture.path().join(".astli/patch.toml").is_file());
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(
        stderr(&again).contains("Removed nothing"),
        "{}",
        stderr(&again)
    );
}
