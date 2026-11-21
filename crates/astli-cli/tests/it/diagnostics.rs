//! How the driver reports problems: the formats of `--diag`, and colour.

use crate::support::*;

#[test]
fn colour_is_the_flags_then_the_environments() {
    let fixture = Fixture::new("colour");
    let messy = fixture.file("messy.sv", &TINY.replace("  assign", "      assign"));
    let unparsed = fixture.file("unparsed.sv", UNPARSED);

    // A diff on stdout, and a diagnostic on stderr.
    let diff = |color: &str, env: &[(&str, &str)]| {
        let output = (env.iter())
            .fold(driver(&fixture), |driver, (name, value)| {
                driver.env(name, value)
            })
            .args(
                ["--color", color, "fmt", "--diff"]
                    .map(std::ffi::OsStr::new)
                    .into_iter()
                    .chain([messy.as_os_str()]),
            );
        output.stdout.contains(&b'\x1b')
    };
    let parse = |color: &str, env: &[(&str, &str)]| {
        let output = (env.iter())
            .fold(driver(&fixture), |driver, (name, value)| {
                driver.env(name, value)
            })
            .args(
                ["--color", color, "parse"]
                    .map(std::ffi::OsStr::new)
                    .into_iter()
                    .chain([unparsed.as_os_str()]),
            );
        output.stderr.contains(&b'\x1b')
    };

    for coloured in [&diff as &dyn Fn(&str, &[(&str, &str)]) -> bool, &parse] {
        assert!(!coloured("auto", &[]), "piped");
        assert!(coloured("always", &[]));
        assert!(!coloured("never", &[]));
        assert!(coloured("auto", &[("CLICOLOR_FORCE", "1")]));
        assert!(!coloured("auto", &[("NO_COLOR", "1")]));
        assert!(!coloured("never", &[("CLICOLOR_FORCE", "1")]));
        assert!(coloured("always", &[("NO_COLOR", "1")]));
    }
}

#[test]
fn diagnostics_go_to_stderr_and_leave_the_output_alone() {
    let fixture = Fixture::new("diag-streams");
    let file = fixture.file("wrong.sv", WRONG);

    let output = driver(&fixture).args(["preprocess".as_ref(), file.as_os_str()]);

    // Output on stdout is preserved while diagnostics are sent to stderr.
    let text = stdout(&output);
    assert!(text.contains("module wrong;"), "{text}");
    assert!(
        !text.contains("Error"),
        "a diagnostic reached stdout: {text}"
    );
    assert!(!text.contains("include-not-found"), "{text}");

    let said = stderr(&output);
    assert!(said.contains("[include-not-found]"), "{said}");
    assert!(said.contains("[undefined-macro]"), "{said}");
    assert!(said.contains("wrong.sv:3:10"), "{said}");
}

#[test]
fn a_file_with_errors_fails_the_run_and_is_counted() {
    let fixture = Fixture::new("diag-count");
    let wrong = fixture.file("wrong.sv", WRONG);
    let fine = fixture.file("tiny.sv", TINY);

    let output =
        driver(&fixture).args(["preprocess".as_ref(), fine.as_os_str(), wrong.as_os_str()]);
    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(said.contains("1 of 2 files has errors"), "{said}");

    // Nothing a worker reports is lost on the parallel path.
    let output = driver(&fixture).args([
        "preprocess".as_ref(),
        "-j4".as_ref(),
        fine.as_os_str(),
        wrong.as_os_str(),
        fine.as_os_str(),
        wrong.as_os_str(),
    ]);
    let said = stderr(&output);
    assert_eq!(said.matches("[include-not-found]").count(), 2, "{said}");
    assert!(said.contains("2 of 4 files have errors"), "{said}");

    let output = driver(&fixture).args(["preprocess".as_ref(), fine.as_os_str()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn one_file_shows_only_so_many_and_says_how_many_it_did_not() {
    let fixture = Fixture::new("diag-cap");
    let mut text = String::from("module many;\n");
    for at in 0..30 {
        text.push_str(&format!("  logic [`W{at}-1:0] q{at};\n"));
    }
    text.push_str("endmodule\n");
    let file = fixture.file("many.sv", &text);

    let said = stderr(&driver(&fixture).args(["preprocess".as_ref(), file.as_os_str()]));

    assert_eq!(said.matches("[undefined-macro]").count(), 20, "{said}");
    assert!(said.contains("30 errors; the first 20 shown"), "{said}");
}

#[test]
fn short_diagnostics_are_one_line_each_and_not_capped() {
    let fixture = Fixture::new("diag-short");
    let mut text = String::from("module many;\n");
    for at in 0..30 {
        text.push_str(&format!("  logic [`W{at}-1:0] q{at};\n"));
    }
    text.push_str("endmodule\n");
    let file = fixture.file("many.sv", &text);

    let said = stderr(&driver(&fixture).args([
        "preprocess".as_ref(),
        "--diag=short".as_ref(),
        file.as_os_str(),
    ]));

    let lines: Vec<_> = said.lines().collect();
    assert_eq!(lines.len(), 30, "{said}");
    assert!(
        lines[0].ends_with("many.sv:2:10: error[undefined-macro]: `W0 is not defined"),
        "{said}"
    );
}

#[test]
fn github_diagnostics_are_workflow_commands_with_their_properties_escaped() {
    let fixture = Fixture::new("diag-github");
    fixture.file("a,b.sv", "`define BAD `NOPE\nassign y = `BAD;\n");

    let output = driver(&fixture).args(["preprocess", "--diag=github", "a,b.sv"]);

    expect_test::expect![[r#"
        ::error title=undefined-macro,file=a%2Cb.sv,line=2,col=12,endLine=2,endColumn=16::a,b.sv:2:12: error[undefined-macro]: `NOPE is not defined
    "#]]
    .assert_eq(&stderr(&output));
}

#[test]
fn json_diagnostics_are_a_line_each_with_the_calls_they_came_through() {
    let fixture = Fixture::new("diag-json");
    fixture.file("macro.sv", "`define BAD `NOPE\nlogic q = `BAD;\n");

    let output = driver(&fixture).args(["preprocess", "--diag=json", "macro.sv"]);

    expect_test::expect![[r#"
        {"severity":"error","code":"undefined-macro","message":"`NOPE is not defined","path":"macro.sv","start":{"line":2,"column":11,"offset":28},"end":{"line":2,"column":15,"offset":32},"label":"not defined here","labels":[],"notes":["the reference stands as written"],"help":[],"spelled":{"path":"macro.sv","start":{"line":1,"column":13,"offset":12},"end":{"line":1,"column":18,"offset":17}},"expansion":[{"macro":"`BAD","path":"macro.sv","start":{"line":2,"column":11,"offset":28},"end":{"line":2,"column":15,"offset":32}}],"included_from":[],"fix":[]}
    "#]]
    .assert_eq(&stderr(&output));
}

#[test]
fn json_diagnostics_carry_their_fix() {
    let fixture = Fixture::new("diag-json-fix");
    fixture.file("top.sv", "module top;\n  reg q;\nendmodule\n");

    let output = driver(&fixture).args(["lint", "--diag=json", "top.sv"]);

    let said = stderr(&output);
    assert!(
        said.contains(r#""fix":[{"text":"logic","path":"top.sv","start":{"line":2,"column":3,"offset":14},"end":{"line":2,"column":6,"offset":17}}]"#),
        "{said}"
    );
}

#[test]
fn json_leaves_stderr_to_the_records() {
    let fixture = Fixture::new("diag-json-alone");
    let wrong = fixture.file("wrong.sv", WRONG);
    let missing = fixture.path().join("missing.sv");

    // Neither the tally of the files with errors nor a status is a record,
    // and a problem with no place is one with its place null.
    let said = stderr(&driver(&fixture).args([
        "preprocess".as_ref(),
        "--diag=json".as_ref(),
        "--color=always".as_ref(),
        wrong.as_os_str(),
        wrong.as_os_str(),
        missing.as_os_str(),
    ]));

    assert!(!said.contains('\u{1b}'), "{said}");
    assert!(
        said.lines()
            .all(|line| line.starts_with(r#"{"severity":""#) && line.ends_with('}')),
        "{said}"
    );
    assert!(!said.contains("have errors"), "{said}");
    assert!(
        said.contains(r#""code":null,"message":"#) && said.contains(r#""path":null,"#),
        "{said}"
    );
}

#[test]
fn gitlab_is_one_report_with_a_fingerprint_for_each_finding() {
    let fixture = Fixture::new("diag-gitlab");
    fixture.file("twice.sv", "logic [`W-1:0] a;\nlogic [`W-1:0] b;\n");
    fixture.file("fine.sv", TINY);

    // Alike findings are told apart by which comes first, not by their line.
    let output = driver(&fixture).args(["preprocess", "--diag=gitlab", "twice.sv", "fine.sv"]);
    expect_test::expect![[r#"
        [
        {"description":"[undefined-macro] `W is not defined","check_name":"undefined-macro","fingerprint":"f579ae42b464d564","severity":"major","location":{"path":"twice.sv","lines":{"begin":1}}},
        {"description":"[undefined-macro] `W is not defined","check_name":"undefined-macro","fingerprint":"f57d1442b467b88d","severity":"major","location":{"path":"twice.sv","lines":{"begin":2}}}
        ]
    "#]]
    .assert_eq(&stderr(&output));
    assert!(!output.status.success());

    let output = driver(&fixture).args(["preprocess", "--diag=gitlab", "fine.sv"]);
    assert_eq!(stderr(&output), "[]\n");
    assert!(output.status.success());
}
