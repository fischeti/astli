//! `test`, and `check --tests`.

use std::process::{Command, Output};

use crate::support::*;

#[test]
fn check_tests_covers_every_point_of_each_test_and_a_point_is_a_top() {
    let fixture = Fixture::new("check-points");
    fixture.file("astli.toml", "[package]\nname = 'fifo'\n");
    // What one point builds is wrong, and what both build. Each point has
    // an interface instance of its own, which one assignment drives.
    fixture.file(
        "tb/tb_fifo.sv",
        "(* astli_test *)\nmodule tb_fifo;\n\
         \x20 (* astli_sweep = \"1, 16\" *) parameter int unsigned Depth = 4;\n\
         \x20 if (Depth > 8) begin : g_deep\n    $error(\"too deep\");\n  end\n\
         \x20 missing u_missing ();\n\
         \x20 bus_if bus ();\n  assign bus.valid = 1'b1;\n\
         endmodule\n",
    );
    fixture.file(
        "tb/bus_if.sv",
        "interface bus_if;\n  logic valid;\nendinterface\n",
    );
    // What no test reaches.
    fixture.file(
        "rtl/helper.sv",
        "module helper;\n  parameter int N = 0;\n  gone u_gone ();\nendmodule\n",
    );
    let check = |args: &[&str]| {
        let mut all = vec!["check", "--diag", "short"];
        all.extend(args);
        driver(&fixture).args(&all)
    };

    let plain = check(&[]);
    let every = check(&["--tests"]);
    let one = check(&["--top", "tb_fifo[Depth=1]"]);
    let deep = check(&["--top", "tb_fifo[Depth=16]"]);
    let none = check(&["--top", "tb_fifo[Depth=3]"]);
    let no_test = check(&["--top", "helper[N=1]"]);
    let with_top = check(&["--tests", "--top", "helper", "--top", "tb_fifo[Depth=1]"]);
    let named = check(&["--tests", "rtl/helper.sv"]);
    let swept = check(&["--tests", "-G", "Depth=4"]);
    let point_swept = check(&["--top", "tb_fifo[Depth=1]", "-G", "Depth=4"]);
    let undeclared = check(&["--tests", "-G", "N=1"]);
    let files = driver(&fixture).args(["files", "--top", "tb_fifo[Depth=16]"]);

    // Without `--tests`, a test is a module, and the package is checked
    // whole at its defaults.
    let said = stderr(&plain);
    assert!(!said.contains("too deep"), "{said}");
    assert!(said.contains("`missing`"), "{said}");
    assert!(said.contains("`gone`"), "{said}");
    assert!(!every.status.success());
    let said = stderr(&every);
    assert!(!said.contains("`gone`"), "{said}");
    assert!(!said.contains("multiple-drivers"), "{said}");
    assert_eq!(said.matches("too deep").count(), 1, "{said}");
    assert_eq!(said.matches("`missing`").count(), 1, "{said}");
    assert!(!stderr(&one).contains("too deep"), "{}", stderr(&one));
    assert!(stderr(&one).contains("`missing`"), "{}", stderr(&one));
    assert!(stderr(&deep).contains("too deep"), "{}", stderr(&deep));
    assert!(
        stderr(&none).contains("`tb_fifo[Depth=3]` names no point of `tb_fifo`"),
        "{}",
        stderr(&none)
    );
    assert!(
        stderr(&no_test).contains("`helper[N=1]` names a point of `helper`, which is no test"),
        "{}",
        stderr(&no_test)
    );
    // The tops named, and every point besides, each finding once.
    let said = stderr(&with_top);
    assert!(said.contains("`gone`"), "{said}");
    assert_eq!(said.matches("too deep").count(), 1, "{said}");
    assert_eq!(said.matches("`missing`").count(), 1, "{said}");
    assert!(
        stderr(&named)
            .contains("`--tests` adds the tests a package marks, and files named are no package's"),
        "{}",
        stderr(&named)
    );
    for swept in [&swept, &point_swept] {
        assert!(
            stderr(swept)
                .contains("`-G Depth=4` sets `Depth`, whose values a sweep of `tb_fifo` lists"),
            "{}",
            stderr(swept)
        );
        assert!(
            stderr(swept).contains("`--top 'tb_fifo[Depth=4]'`"),
            "{}",
            stderr(swept)
        );
    }
    assert!(
        stderr(&undeclared).contains("`-G N=1` sets a parameter no top declares"),
        "{}",
        stderr(&undeclared)
    );
    assert!(files.status.success(), "{}", stderr(&files));
    assert!(
        stdout(&files).ends_with("tb_fifo.sv\n"),
        "{}",
        stdout(&files)
    );
}

#[test]
fn verilator_test_runs_each_test_on_its_own_and_reports_each_failure() {
    let fixture = Fixture::new("verilator-test");
    fixture.file(
        "astli.toml",
        "[package]\nname = \"bench\"\n\n[test]\ntimeout = 2\n\n\
         [top.tb_table]\ndefines = { W = 4 }\nparams = { V = 15 }\n\n\
         [top.tb_same]\ndefines = { W = 4 }\nparams = { V = 3 }\n\n\
         [profile.dev.verilator]\nrun-args = [\"+PROFILE\"]\n",
    );
    let test = |name: &str, body: &str| {
        fixture.file(
            &format!("tb/{name}.sv"),
            &format!("(* astli_test *)\nmodule {name};\n{body}endmodule\n"),
        );
    };
    // A run starts in a directory of its own, told where its package is.
    test(
        "tb_pass",
        "  string root;\n  int seed, fd;\n  initial begin\n\
         \x20   void'($value$plusargs(\"astli_root=%s\", root));\n\
         \x20   void'($value$plusargs(\"verilator+seed+%d\", seed));\n\
         \x20   fd = $fopen(\"written.txt\", \"w\");\n\
         \x20   $fclose(fd);\n\
         \x20   $display(\"root %s seed %0d\", root, seed);\n\
         \x20   $finish;\n  end\n",
    );
    // Built under its table's defines and parameters.
    test(
        "tb_table",
        "  parameter int V = 0;\n  logic [`W-1:0] x;\n  initial begin\n\
         \x20   x = V;\n\
         \x20   if (x != 15) $fatal(1, \"x is %0d\", x);\n\
         \x20   $finish;\n  end\n",
    );
    // Checked with it, as its table defines the same, and built apart.
    test(
        "tb_same",
        "  parameter int V = 0;\n  logic [`W-1:0] x;\n  initial begin\n\
         \x20   x = V;\n\
         \x20   if (x != 3) $fatal(1, \"x is %0d\", x);\n\
         \x20   $finish;\n  end\n",
    );
    test(
        "tb_fatal",
        "  initial begin\n    $display(\"about to fail\");\n    $fatal(1, \"boom\");\n  end\n",
    );
    test(
        "tb_error",
        "  initial begin\n    $error(\"bad\");\n    $finish;\n  end\n",
    );
    test("tb_hang", "  initial forever #1;\n");
    // What the profile gives and what follows `--` reach every run.
    test(
        "tb_args",
        "`ifndef STOP\n`define STOP 0\n`endif\n  initial begin\n\
         \x20   if (!$test$plusargs(\"PROFILE\")) $fatal(1, \"no run-args\");\n\
         \x20   if ($test$plusargs(\"BREAK\") && `STOP == 4) $fatal(1, \"broken\");\n\
         \x20   $finish;\n  end\n",
    );
    // Each point a build of its own, which fails alone.
    test(
        "tb_sweep",
        "  (* astli_sweep = \"1, 2\" *) parameter int N = 1;\n  initial begin\n\
         \x20   if (N == 2) $fatal(1, \"two\");\n\
         \x20   $finish;\n  end\n",
    );
    let builds = fixture.path().join(".astli/build/verilator/dev/bench");

    let all = driver(&fixture).args(["test"]);
    let seeded = driver(&fixture).args(["test", "--seed", "1234", "--nocapture", "pass"]);
    let built = driver(&fixture).args(["test", "--no-run"]);

    assert!(!all.status.success());
    let said = said_in(&fixture, &all);
    assert!(said.contains("Passed `tb_pass`"), "{said}");
    assert!(said.contains("Passed `tb_table`"), "{said}");
    assert!(said.contains("Passed `tb_same`"), "{said}");
    // Verilator before 5.040 ends a `$fatal` by aborting, after it the
    // simulation exits with a status.
    assert!(
        said.contains("`tb_fatal` failed: the simulation exited with status")
            || said.contains("`tb_fatal` failed: the simulation was ended by signal"),
        "{said}"
    );
    assert!(said.contains("`tb_error` failed"), "{said}");
    assert!(
        said.contains("`tb_hang` failed: it ran past its timeout of 2s"),
        "{said}"
    );
    assert!(
        said.contains("`astli test --exact tb_fatal --seed "),
        "{said}"
    );
    assert!(said.contains("Passed `tb_sweep[N=1]`"), "{said}");
    assert!(said.contains("Passed `tb_args`"), "{said}");
    assert!(
        said.contains("4 of 9 tests failed: `tb_error`, `tb_fatal`, `tb_hang`, `tb_sweep[N=2]`"),
        "{said}"
    );
    let points: Vec<_> = std::fs::read_dir(builds.join("tb_sweep"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(points.len(), 2, "{points:?}");
    assert!(points.iter().all(|point| point.join("Vtb_sweep").is_file()));
    // A failure's output is shown, and a pass's is not.
    assert!(stdout(&all).contains("about to fail"), "{}", stdout(&all));
    assert!(!stdout(&all).contains("root "), "{}", stdout(&all));
    // The seed the name gives, beside the one `--seed` gave.
    let runs: Vec<_> = std::fs::read_dir(builds.join("tb_pass/run"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|run| !run.ends_with("1234"))
        .collect();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].join("written.txt").is_file());
    let log = std::fs::read_to_string(runs[0].join("output.log")).unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    assert!(
        log.contains(&format!("root {}", root.display()))
            || log.contains(&format!("root {}", fixture.path().display())),
        "{log}"
    );

    assert!(seeded.status.success(), "{}", stderr(&seeded));
    assert!(!stderr(&seeded).contains("Building"), "{}", stderr(&seeded));
    assert!(stdout(&seeded).contains("seed 1234"), "{}", stdout(&seeded));
    assert!(builds.join("tb_pass/run/1234/output.log").is_file());

    assert!(built.status.success(), "{}", stderr(&built));
    assert!(!stderr(&built).contains("Passed"), "{}", stderr(&built));
    assert!(
        !stderr(&built).contains("Building `tb_sweep[N=2]`"),
        "{}",
        stderr(&built)
    );
    assert!(
        stderr(&built).contains("Finished building 9 tests"),
        "{}",
        stderr(&built)
    );

    // Passed after what astli and the profile give a run, and carried by
    // the command that repeats it, which the profile's are not, as is a
    // define the failure needs. A shell reads the command back as the
    // words it repeats; running it would build with waveforms, which a
    // simulator's headers may not allow.
    let args = driver(&fixture).args([
        "test",
        "--seed",
        "7",
        "-D",
        "STOP=(2*2)",
        "args",
        "--",
        "+BREAK",
        "+a b",
    ]);
    assert!(!args.status.success());
    let said = said_in(&fixture, &args);
    assert!(said.contains("`tb_args` failed"), "{said}");
    let repeat = "astli test --exact tb_args --seed 7 --trace -D 'STOP=(2*2)' -- +BREAK '+a b'";
    assert!(said.contains(&format!("`{repeat}` repeats it")), "{said}");
    let read = Command::new("sh")
        .args(["-c", &format!("printf '%s\\n' {repeat}")])
        .output()
        .unwrap();
    assert_eq!(
        stdout(&read).lines().collect::<Vec<_>>(),
        [
            "astli",
            "test",
            "--exact",
            "tb_args",
            "--seed",
            "7",
            "--trace",
            "-D",
            "STOP=(2*2)",
            "--",
            "+BREAK",
            "+a b"
        ]
    );

    // Three runs from one build, each named by its seed, each in its own
    // directory.
    let seeds = driver(&fixture).args(["test", "--seeds", "3", "pass"]);
    let swept = driver(&fixture).args(["test", "--seeds", "2", "sweep"]);
    assert!(seeds.status.success(), "{}", stderr(&seeds));
    let said = stderr(&seeds);
    assert!(!said.contains("Building"), "{said}");
    assert_eq!(said.matches("Passed `tb_pass seed ").count(), 3, "{said}");
    assert!(said.contains("Finished 3 runs in "), "{said}");
    let runs = std::fs::read_dir(builds.join("tb_pass/run"))
        .unwrap()
        .count();
    assert_eq!(runs, 5);
    // A seed the simulator cannot take is refused before anything runs.
    let large = driver(&fixture).args(["test", "--seed", "2147483648", "pass"]);
    assert!(!large.status.success());
    let said = stderr(&large);
    assert!(
        said.contains(
            "`--seed 2147483648` is larger than the largest seed a simulator takes, 2147483647"
        ),
        "{said}"
    );
    assert!(!said.contains("Passed"), "{said}");
    // Each point of a sweep runs under seeds of its own, in its own build
    // directory, and a failure names its point and its seed, which the
    // command that repeats it takes.
    assert!(!swept.status.success());
    let said = said_in(&fixture, &swept);
    assert_eq!(
        said.matches("Passed `tb_sweep[N=1] seed ").count(),
        2,
        "{said}"
    );
    let failed: Vec<&str> = (said.lines())
        .filter_map(|line| line.strip_prefix("error: `tb_sweep[N=2] seed "))
        .filter_map(|line| line.split_once('`').map(|(seed, _)| seed))
        .collect();
    assert_eq!(failed.len(), 2, "{said}");
    for seed in &failed {
        assert!(
            said.contains(&format!(
                "`astli test --exact 'tb_sweep[N=2]' --seed {seed} --trace`"
            )),
            "{said}"
        );
    }
    assert!(
        said.contains("2 of 4 runs failed: `tb_sweep[N=2] seed "),
        "{said}"
    );
    let dirs: Vec<_> = std::fs::read_dir(builds.join("tb_sweep"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(dirs.len(), 2, "{dirs:?}");
    for dir in dirs {
        assert!(std::fs::read_dir(dir.join("run")).unwrap().count() >= 2);
    }
}

#[test]
fn verilator_test_passes_a_test_that_fails_with_the_text_it_should() {
    let fixture = Fixture::new("verilator-should-fail");
    fixture.file(
        "astli.toml",
        "[package]\nname = \"bench\"\n\n[test]\ntimeout = 2\n",
    );
    let test = |name: &str, body: &str| {
        fixture.file(
            &format!("tb/{name}.sv"),
            &format!(
                "(* astli_test, astli_should_fail = \"[OVF]\" *)\n\
                 module {name};\n  initial begin\n{body}  end\nendmodule\n"
            ),
        );
    };
    test("tb_caught", "    $fatal(1, \"[OVF] write while full\");\n");
    test("tb_passes", "    $finish;\n");
    test("tb_other", "    $error(\"[UNF] read while empty\");\n");
    // A run past its timeout fails whatever it wrote.
    test("tb_hangs", "    $display(\"[OVF]\");\n    forever #1;\n");

    let all = driver(&fixture).args(["test", "--seeds", "2", "--junit", "report.xml"]);

    assert!(!all.status.success());
    let said = said_in(&fixture, &all);
    assert_eq!(said.matches("Passed `tb_caught seed ").count(), 2, "{said}");
    assert!(said.contains("failing as it should"), "{said}");
    assert!(
        said.contains("it passed, and should fail with `[OVF]` in its output"),
        "{said}"
    );
    assert!(said.contains("without `[OVF]` in its output"), "{said}");
    assert!(said.contains("it ran past its timeout of 2s"), "{said}");
    assert!(said.contains("6 of 8 runs failed"), "{said}");
    // A failure's reason is plain text in the report.
    let report = std::fs::read_to_string(fixture.path().join("report.xml")).unwrap();
    assert!(
        report.contains("message=\"it passed, and should fail with `[OVF]` in its output\""),
        "{report}"
    );
}

#[test]
fn verilator_test_runs_every_members_tests_in_one_summary() {
    let fixture = Fixture::new("verilator-test-members");
    // A virtual root has no tests of its own to time.
    fixture.file("astli.toml", "[workspace]\n\n[test]\ntimeout = 5\n");
    let tb_top = |body: &str| {
        format!("(* astli_test *)\nmodule tb_top;\n  initial begin\n{body}  end\nendmodule\n")
    };
    fixture.file("ip/a/astli.toml", "[package]\nname = \"a\"\n");
    fixture.file("ip/a/tb_top.sv", &tb_top("    $finish;\n"));
    // A case for each point.
    fixture.file(
        "ip/a/tb_sweep.sv",
        "(* astli_test *)\nmodule tb_sweep;\n  (* astli_sweep = \"1, 2\" *) parameter int N = 1;\n\
         \x20 initial $finish;\nendmodule\n",
    );
    fixture.file("ip/b/astli.toml", "[package]\nname = \"b\"\n");
    // What it writes is held in the report as text.
    fixture.file(
        "ip/b/tb_top.sv",
        &tb_top("    $display(\"<b> & \\\"c\\\"\");\n    $fatal(1, \"b fails\");\n"),
    );
    // One package's check fails, and the others' tests still run.
    fixture.file("ip/c/astli.toml", "[package]\nname = \"c\"\n");
    fixture.file(
        "ip/c/tb_top.sv",
        "(* astli_test *)\nmodule tb_top;\n  missing u_missing ();\nendmodule\n",
    );
    let report = fixture.path().join("report.xml");

    let output = driver(&fixture).args(["test", "--junit", report.to_str().unwrap()]);

    assert!(!output.status.success());
    let said = said_in(&fixture, &output);
    assert!(said.contains("`[test]`"), "{said}");
    // Two members' builds of one name told apart.
    assert!(
        said.contains("Building `a tb_top` with Verilator"),
        "{said}"
    );
    assert!(
        said.contains("Building `b tb_top` with Verilator"),
        "{said}"
    );
    assert!(said.contains("Passed `a tb_top`"), "{said}");
    assert!(said.contains("`b tb_top` failed"), "{said}");
    assert!(said.contains("--trace -p b`"), "{said}");
    assert!(
        said.contains("no module, interface or program is called `missing`"),
        "{said}"
    );
    assert!(said.contains("1 of 4 tests failed: `b tb_top`"), "{said}");
    assert!(said.contains("1 of 3 packages failed"), "{said}");
    let builds = fixture.path().join(".astli/build/verilator/dev");
    assert!(builds.join("a/tb_top/Vtb_top").is_file());
    assert!(builds.join("b/tb_top/Vtb_top").is_file());

    // A suite for each package, the one whose check failed among them.
    let xml = std::fs::read_to_string(&report).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|err| panic!("{err}\n{xml}"));
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "testsuites");
    assert_eq!(
        (
            root.attribute("tests"),
            root.attribute("failures"),
            root.attribute("errors")
        ),
        (Some("5"), Some("1"), Some("1"))
    );
    let suites: Vec<_> = root.children().filter(|it| it.is_element()).collect();
    let names: Vec<_> = suites
        .iter()
        .map(|it| it.attribute("name").unwrap())
        .collect();
    assert_eq!(names, ["a", "b", "c"]);
    let cases = |suite: usize| -> Vec<_> {
        (suites[suite].children())
            .filter(|it| it.has_tag_name("testcase"))
            .collect()
    };
    let named: Vec<_> = (cases(0).iter())
        .map(|it| it.attribute("name").unwrap())
        .collect();
    assert_eq!(named, ["tb_sweep[N=1]", "tb_sweep[N=2]", "tb_top"]);
    let case = |suite: usize| {
        let case = cases(suite).pop().unwrap();
        assert_eq!(case.attribute("name"), Some("tb_top"));
        assert_eq!(case.attribute("classname"), Some(names[suite]));
        assert!(case.attribute("time").unwrap().parse::<f64>().is_ok());
        case.children().find(|it| it.is_element())
    };
    assert!(cases(0).iter().all(|it| !it.has_children()));
    assert!(case(0).is_none());
    let failure = case(1).unwrap();
    assert_eq!(failure.tag_name().name(), "failure");
    assert!(
        failure
            .attribute("message")
            .unwrap()
            .contains("the simulation"),
        "{xml}"
    );
    assert!(failure.text().unwrap().contains("<b> & \"c\""), "{xml}");
    assert_eq!(case(2).unwrap().tag_name().name(), "error");

    // A relative `-I` is relative to the member it is given in, though the
    // simulator runs at the root.
    fixture.file("include/d_defs.svh", "`define D_DONE $finish\n");
    fixture.file("ip/d/astli.toml", "[package]\nname = \"d\"\n");
    fixture.file(
        "ip/d/tb_top.sv",
        "`include \"d_defs.svh\"\n(* astli_test *)\nmodule tb_top;\n  initial `D_DONE;\nendmodule\n",
    );
    let member = fixture.path().join("ip/d");
    let output = driver(&fixture)
        .dir(&member)
        .args(["test", "-I", "../../include"]);
    assert!(output.status.success(), "{}", said_in(&fixture, &output));
}

#[test]
fn test_list_selects_the_tests_the_package_marks() {
    let fixture = Fixture::new("workspace-test");
    // A dependency's test is its own CI's, and so is a sibling's the
    // package requires by path.
    fixture.file(
        "cells/tb_cells.sv",
        "(* astli_test *)\nmodule tb_cells;\nendmodule\n",
    );
    let ws = workspace(&fixture);
    fixture.file(
        "ws/astli.toml",
        "[package]\nname = 'soc'\ndev = ['tb/**']\n\n[workspace]\n\n\
         [dependencies]\ndma = { path = 'ip/dma' }\n",
    );
    fixture.file(
        "ws/tb/tb_soc.sv",
        "(* astli_test *)\nmodule tb_soc;\nendmodule\n",
    );
    // Two members may each hold a test of one name.
    let tb_top = "(* astli_test *)\nmodule tb_top;\nendmodule\n";
    fixture.file("ws/ip/dma/tb_top.sv", tb_top);
    fixture.file("ws/ip/other/tb_top.sv", tb_top);
    fixture.file(
        "ws/ip/other/tb_other.sv",
        "(* astli_test *) module tb_other;\nendmodule\nmodule helper;\nendmodule\n",
    );
    let at_root = |args: &[&str]| driver(&fixture).dir(&ws).cache().args(args);
    let listed = |output: &Output| {
        assert!(output.status.success(), "{}", stderr(output));
        stdout(output)
    };

    assert_eq!(listed(&at_root(&["test", "--list"])), "tb_soc\n");
    assert_eq!(
        listed(&at_root(&["test", "--list", "--workspace"])),
        "dma tb_top\nother tb_other\nother tb_top\nsoc tb_soc\n"
    );
    assert_eq!(
        listed(&at_root(&["test", "--list", "-p", "other"])),
        "tb_other\ntb_top\n"
    );
    assert_eq!(
        listed(&at_root(&["test", "--list", "--workspace", "top"])),
        "dma tb_top\nother tb_top\n"
    );
    assert_eq!(
        listed(&at_root(&[
            "test", "--list", "-p", "other", "--exact", "tb_top"
        ])),
        "tb_top\n"
    );
    // Under `--workspace`, a filter need only match in one package.
    assert_eq!(
        listed(&at_root(&["test", "--list", "--workspace", "tb_soc"])),
        "soc tb_soc\n"
    );

    let exact = at_root(&["test", "--list", "-p", "other", "--exact", "tb_to"]);
    assert!(!exact.status.success());
    assert!(
        stderr(&exact).contains("no test's name is `tb_to`"),
        "{}",
        stderr(&exact)
    );
    // A package's name is no part of what a filter matches.
    let package = at_root(&["test", "--list", "--workspace", "dma"]);
    assert!(!package.status.success());
    assert!(
        stderr(&package).contains("no test's name is part of `dma`"),
        "{}",
        stderr(&package)
    );
    let elsewhere = at_root(&["test", "--list", "-p", "other", "tb_soc"]);
    assert!(!elsewhere.status.success());

    // Seeds given two ways, or none, are refused.
    for (args, said) in [
        (
            &["--seeds", "0"][..],
            "`--seeds` takes a number of seeds, more than none",
        ),
        (
            &["--seeds", "2", "--seed", "1"],
            "`--seed` runs each test under one seed",
        ),
    ] {
        let refused = at_root(&[&["test", "--list"][..], args].concat());
        assert!(!refused.status.success(), "{args:?}");
        assert!(stderr(&refused).contains(said), "{}", stderr(&refused));
    }

    // A mark with a value, or on what is no module, is refused where it
    // is, as is a failure expected without its text, or of no test.
    fixture.file(
        "ws/tb/wrong.sv",
        "(* astli_test = 1 *)\nmodule tb_valued;\nendmodule\n\
         (* astli_test *)\ninterface tb_if;\nendinterface\n\
         (* astli_test, astli_should_fail = \"\" *)\nmodule tb_empty;\nendmodule\n\
         (* astli_should_fail = \"x\" *)\nmodule tb_unmarked;\nendmodule\n",
    );
    let wrong = at_root(&["test", "--list"]);
    assert!(!wrong.status.success());
    let said = stderr(&wrong).replace('\\', "/");
    assert!(said.contains("`astli_test` takes no value"), "{said}");
    assert!(said.contains("tb/wrong.sv:1:4"), "{said}");
    assert!(
        said.contains("`astli_test` marks a module, and `tb_if` is an interface"),
        "{said}"
    );
    assert!(
        said.contains("`astli_should_fail` takes the text a failing run writes"),
        "{said}"
    );
    assert!(said.contains("tb/wrong.sv:7:16"), "{said}");
    assert!(
        said.contains("`astli_should_fail` says how a test fails, and `tb_unmarked` is no test"),
        "{said}"
    );
    assert!(said.contains("4 attributes marked no test"), "{said}");
}

#[test]
fn test_list_names_each_point_of_the_sweeps_a_test_declares() {
    let fixture = Fixture::new("test-sweeps");
    fixture.file("astli.toml", "[package]\nname = 'fifo'\n");
    fixture.file(
        "fifo_pkg.sv",
        "package fifo_pkg;\n  typedef enum logic [1:0] { Registered, Bypass, Fall } mode_e;\nendpackage\n",
    );
    // The product of a list and an enum's every member, in declaration
    // order, and a bit's two values.
    fixture.file(
        "tb/tb_fifo.sv",
        "(* astli_test *)\nmodule tb_fifo;\n\
         \x20 (* astli_sweep = \"1, 16\" *) parameter int unsigned Depth = 4;\n\
         \x20 parameter int Width = 8;\n\
         \x20 (* astli_sweep *) parameter fifo_pkg::mode_e Mode = fifo_pkg::Registered;\n\
         endmodule\n",
    );
    fixture.file(
        "tb/tb_bit.sv",
        "(* astli_test *)\nmodule tb_bit;\n  (* astli_sweep *) parameter bit Fast = 0;\nendmodule\n",
    );
    let listed = |args: &[&str]| {
        let output = driver(&fixture).args(args);
        assert!(output.status.success(), "{}", stderr(&output));
        stdout(&output)
    };
    let refused = |args: &[&str]| {
        let output = driver(&fixture).args(args);
        assert!(!output.status.success(), "{}", stdout(&output));
        said_in(&fixture, &output)
    };

    assert_eq!(
        listed(&["test", "--list"]),
        "tb_bit[Fast=0]\ntb_bit[Fast=1]\n\
         tb_fifo[Depth=1,Mode=Registered]\ntb_fifo[Depth=1,Mode=Bypass]\n\
         tb_fifo[Depth=1,Mode=Fall]\ntb_fifo[Depth=16,Mode=Registered]\n\
         tb_fifo[Depth=16,Mode=Bypass]\ntb_fifo[Depth=16,Mode=Fall]\n"
    );
    assert_eq!(
        listed(&["test", "--list", "Depth=16"]),
        "tb_fifo[Depth=16,Mode=Registered]\ntb_fifo[Depth=16,Mode=Bypass]\n\
         tb_fifo[Depth=16,Mode=Fall]\n"
    );
    assert_eq!(
        listed(&["test", "--list", "--exact", "tb_fifo[Depth=1,Mode=Fall]"]),
        "tb_fifo[Depth=1,Mode=Fall]\n"
    );
    // `-G` sets a parameter in every test that declares it, and only there.
    assert_eq!(
        listed(&["test", "--list", "-G", "Width=4", "Depth=1,Mode=F"]),
        "tb_fifo[Depth=1,Mode=Fall]\n"
    );
    let width = refused(&["test", "--list", "-G", "Width=4", "tb_bit"]);
    assert!(
        width.contains("`-G Width=4` sets a parameter no test selected declares"),
        "{width}"
    );
    let swept = refused(&["test", "--list", "-G", "Depth=16"]);
    assert!(
        swept.contains("`-G Depth=16` sets `Depth`, whose values a sweep of `tb_fifo` lists"),
        "{swept}"
    );
    assert!(swept.contains("`astli test 'Depth=16'`"), "{swept}");

    // A test under a table that defines a macro is a design apart from the
    // other's, and a problem both meet is told once.
    fixture.file(
        "astli.toml",
        "[package]\nname = 'fifo'\n\n[top.tb_bit]\ndefines = { W = 4 }\n",
    );
    let bad = fixture.path().join("tb/bad.sv");
    std::fs::write(&bad, b"module bad;\xff\nendmodule\n").unwrap();
    let unread = driver(&fixture).args(["test", "--list"]);
    let said = stderr(&unread);
    assert_eq!(
        said.matches("did not contain valid UTF-8").count(),
        1,
        "{said}"
    );
    std::fs::remove_file(bad).unwrap();

    // Each mistake is told at its attribute.
    fixture.file(
        "astli.toml",
        "[package]\nname = 'fifo'\n\n[top.tb_fifo]\nparams = { Depth = 2 }\n",
    );
    fixture.file(
        "tb/tb_wrong.sv",
        "(* astli_test *)\nmodule tb_wrong;\n\
         \x20 (* astli_sweep = \"1, 300\" *) parameter logic [7:0] Narrow = 1;\n\
         \x20 (* astli_sweep *) parameter int Wide = 1;\n\
         \x20 (* astli_sweep = \"Fast\" *) parameter fifo_pkg::mode_e Mode = fifo_pkg::Bypass;\n\
         \x20 (* astli_sweep = \"1\" *) localparam int Local = 1;\n\
         \x20 (* astli_sweep = 3 *) parameter int Number = 1;\n\
         \x20 (* astli_sweep = \"1\" *) parameter real Real = 1.0;\n\
         endmodule\n\
         (* astli_test *)\nmodule tb_listed #(parameter int P = 1);\n\
         \x20 (* astli_sweep = \"1\" *) parameter int Body = 1;\n\
         endmodule\n",
    );
    let wrong = refused(&["test", "--list"]);
    for (said, at) in [
        (
            "`Narrow` of `tb_wrong` cannot hold `300`",
            "tb_wrong.sv:3:6",
        ),
        (
            "`Wide` of `tb_wrong` has a type with more values than a sweep takes without a list",
            "tb_wrong.sv:4:6",
        ),
        (
            "`Mode` of `tb_wrong` is set to `Fast`, which is no member of its enum",
            "tb_wrong.sv:5:6",
        ),
        (
            "`astli_sweep` sweeps a `parameter`, and `Local` is a `localparam`",
            "tb_wrong.sv:6:6",
        ),
        (
            "`astli_sweep` takes its values as a string",
            "tb_wrong.sv:7:6",
        ),
        (
            "`Real` of `tb_wrong` has a type whose values have no short spelling",
            "tb_wrong.sv:8:6",
        ),
        (
            "`Body` of `tb_listed` is local, since `tb_listed` has a parameter port list",
            "tb_wrong.sv:12:6",
        ),
        (
            "`[top.tb_fifo] params` sets `Depth`, whose values a sweep lists",
            "tb_fifo.sv:3:6",
        ),
    ] {
        assert!(wrong.contains(said), "{said} in {wrong}");
        assert!(wrong.contains(at), "{at} in {wrong}");
    }
    assert!(wrong.contains("remove the `#( ... )` list"), "{wrong}");
    assert!(wrong.contains("8 attributes swept no parameter"), "{wrong}");

    // On what is no test, it sweeps nothing.
    std::fs::remove_file(fixture.path().join("tb/tb_wrong.sv")).unwrap();
    fixture.file(
        "tb/helper.sv",
        "module helper;\n  (* astli_sweep = \"1\" *) parameter int N = 1;\nendmodule\n",
    );
    let helper = refused(&["test", "--list"]);
    assert!(
        helper.contains("`astli_sweep` sweeps a test's parameter, and `helper` is no test"),
        "{helper}"
    );
}
