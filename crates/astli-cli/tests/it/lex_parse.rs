//! `lex`, `preprocess` and `parse`, and how a run reads the files it is named.

use crate::support::*;

const USES_MACRO: &str = "\
`define WIDTH 8
`define REG(q, d) always_ff @(posedge clk_i) q <= d

module uses_macro (input logic clk_i, input logic [`WIDTH-1:0] d_i, output logic [`WIDTH-1:0] q_o);
  `REG(q_o, d_i);
endmodule
";

#[test]
fn lex_prints_kinds_and_round_trips() {
    let fixture = Fixture::new("lex");
    let file = fixture.file("tiny.sv", TINY);

    let output = driver(&fixture).args(["lex".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("MODULE_KW@0..6 \"module\""), "{text}");
    assert!(text.contains("round-trips: true"), "{text}");
}

#[test]
fn lex_without_trivia_hides_whitespace() {
    let fixture = Fixture::new("lex-no-trivia");
    let file = fixture.file("tiny.sv", TINY);

    let output = driver(&fixture).args(["lex".as_ref(), file.as_os_str(), "--no-trivia".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!text.contains("WHITESPACE"), "{text}");
    assert!(text.contains("MODULE_KW"), "{text}");
}

#[test]
fn preprocess_expands_a_macro() {
    let fixture = Fixture::new("pp");
    let file = fixture.file("uses_macro.sv", USES_MACRO);

    let output = driver(&fixture).args(["preprocess".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("always_ff @(posedge clk_i)"), "{text}");
    // Macro definitions are consumed during expansion.
    assert!(!text.contains("`define"), "{text}");
}

#[test]
fn preprocess_follows_an_include_found_on_the_search_path() {
    let fixture = Fixture::new("pp-include");
    let file = fixture.file("top.sv", "`include \"macros.svh\"\n`SHOUT\n");
    fixture.file(
        "include/macros.svh",
        "`define SHOUT initial $display(\"hi\");\n",
    );

    let output = driver(&fixture).args([
        "pp".as_ref(),
        file.as_os_str(),
        "-I".as_ref(),
        fixture.path().join("include").as_os_str(),
    ]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("initial $display(\"hi\");"), "{text}");
}

#[test]
fn a_command_line_define_is_used_like_any_other() {
    let fixture = Fixture::new("pp-define");
    let file = fixture.file("top.sv", "localparam int W = `WIDTH;\n");

    let output = driver(&fixture).args(["pp".as_ref(), file.as_os_str(), "-DWIDTH=16".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 16;"), "{text}");
}

#[test]
fn a_command_line_define_is_in_the_table_and_says_where_it_came_from() {
    let fixture = Fixture::new("pp-table");
    let file = fixture.file("tiny.sv", TINY);

    let output = driver(&fixture).args([
        "pp".as_ref(),
        file.as_os_str(),
        "-DSYNTHESIS".as_ref(),
        "--emit".as_ref(),
        "table".as_ref(),
    ]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("SYNTHESIS"), "{text}");
    assert!(text.contains("<command-line>"), "{text}");
}

#[test]
fn the_table_holds_what_an_include_defined_and_not_only_the_named_file() {
    let fixture = Fixture::new("pp-table-include");
    fixture.file(
        "inc/defs.svh",
        "`define FROM_HEADER(q, d) always_ff @(posedge clk_i) q <= d\n",
    );
    // Everything used comes from the header; the file defines nothing itself.
    let file = fixture.file(
        "top.sv",
        "`include \"defs.svh\"\nmodule top; `FROM_HEADER(q, d); endmodule\n",
    );

    let output = driver(&fixture).args([
        "pp".as_ref(),
        file.as_os_str(),
        "-I".as_ref(),
        fixture.path().join("inc").as_os_str(),
        "--emit".as_ref(),
        "table".as_ref(),
    ]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    // Macro arity determines whether following parentheses form an argument list.
    assert!(text.contains("FROM_HEADER/2"), "{text}");
    // Grouped under the header where it was defined.
    assert!(text.contains("defs.svh"), "{text}");
}

#[test]
fn an_include_path_tells_parse_an_arity_it_would_otherwise_guess_at() {
    let fixture = Fixture::new("parse-arity");
    // Nullary macro standing in for a keyword; following parentheses belong to the expression.
    fixture.file("inc/defs.svh", "`define WITH iff\n");
    let file = fixture.file(
        "uses.sv",
        "`include \"defs.svh\"\n\
         module m;\n\
           property p; a `WITH (!b) |-> c; endproperty\n\
         endmodule\n",
    );

    let guessed = stdout(&driver(&fixture).args(["parse".as_ref(), file.as_os_str()]));
    let told = stdout(&driver(&fixture).args([
        "parse".as_ref(),
        file.as_os_str(),
        "-I".as_ref(),
        fixture.path().join("inc").as_os_str(),
    ]));

    // When arity is known, the call is just the macro name without an argument list.
    assert!(guessed.contains("MACRO_ARG_LIST"), "{guessed}");
    assert!(!told.contains("MACRO_ARG_LIST"), "{told}");
    // Both parse trees round-trip back to the source.
    assert!(guessed.contains("round-trips: true"), "{guessed}");
    assert!(told.contains("round-trips: true"), "{told}");
}

#[test]
fn parse_names_the_expansion_only_when_one_ran() {
    let fixture = Fixture::new("parse-seed");
    let file = fixture.file("tiny.sv", TINY);

    let bare = stdout(&driver(&fixture).args(["parse".as_ref(), "-q".as_ref(), file.as_os_str()]));
    let built = stdout(&driver(&fixture).args([
        "parse".as_ref(),
        "-q".as_ref(),
        file.as_os_str(),
        "-DSYNTHESIS".as_ref(),
    ]));

    assert!(!bare.contains("expand"), "{bare}");
    assert!(built.contains("expand"), "{built}");
}

#[test]
fn parse_expand_parses_what_the_macros_write() {
    let fixture = Fixture::new("parse-expand");
    let file = fixture.file(
        "inst.sv",
        "`define INST(t) t u_i ();\nmodule top;\n  `INST(core)\nendmodule\n",
    );

    let output = driver(&fixture).args(["parse".as_ref(), "--expand".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("INSTANTIATION@"), "{text}");
    assert!(!text.contains("MACRO_CALL@"), "{text}");
    assert!(text.contains("round-trips: true"), "{text}");
}

#[test]
fn parse_prints_a_tree_that_round_trips() {
    let fixture = Fixture::new("parse");
    let file = fixture.file("tiny.sv", TINY);

    let output = driver(&fixture).args(["parse".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.starts_with("SOURCE_FILE@"), "{text}");
    assert!(text.contains("MODULE_DECL@"), "{text}");
    assert!(text.contains("round-trips: true"), "{text}");
}

#[test]
fn the_summary_is_one_line_over_the_run_and_quiet_prints_only_it() {
    let fixture = Fixture::new("summary");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let output = driver(&fixture).args(["lex".as_ref(), a.as_os_str(), b.as_os_str()]);
    let text = stdout(&output);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("MODULE_KW"), "{text}");
    let totals: Vec<_> = text.lines().filter(|line| line.contains("files")).collect();
    assert_eq!(totals.len(), 1, "{text}");
    assert!(totals[0].starts_with("2 files, "), "{text}");
    assert_eq!(text.matches("round-trips:").count(), 1, "{text}");

    // `-q` is a run flag, so it may come before the subcommand too.
    for argv in [["lex", "-q"], ["-q", "lex"]] {
        let mut args: Vec<&std::ffi::OsStr> = argv.iter().map(std::ffi::OsStr::new).collect();
        args.extend([a.as_os_str(), b.as_os_str()]);
        let output = driver(&fixture).args(args);
        let text = stdout(&output);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(!text.contains("==="), "{argv:?}: {text}");
        assert!(!text.contains("MODULE_KW"), "{argv:?}: {text}");
        assert!(
            text.trim_start().starts_with("2 files, "),
            "{argv:?}: {text}"
        );
    }
}

#[test]
fn a_file_that_is_not_there_is_reported_with_its_path() {
    let fixture = Fixture::new("a-file-that-is-not-there-is-reported-with-its-path");
    let output = driver(&fixture).args(["parse", "no-such-file.sv"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert!(
        stderr(&output).contains("no-such-file.sv"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn what_did_not_parse_fails_only_the_commands_that_read_a_design() {
    let fixture = Fixture::new("not-parsed");
    let file = fixture.file("m.sv", UNPARSED);

    // `parse` shows the tree it kept, and `lint` with no design rule on reads
    // only the raw file, where valid code may not parse.
    for (args, succeeds, said) in [
        (&["parse", "-q"][..], true, "m.sv:2:3: warning[not-parsed]"),
        (&["check"], false, "m.sv:2:3: error[not-parsed]"),
        (&["lint"], false, "m.sv:2:3: error[not-parsed]"),
        (
            &["lint", "--allow", "suspicious"],
            true,
            "m.sv:2:3: warning[not-parsed]",
        ),
    ] {
        let mut argv: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
        argv.extend(["--diag".as_ref(), "short".as_ref(), file.as_os_str()]);
        let output = driver(&fixture).args(argv);
        let text = stderr(&output);
        assert_eq!(output.status.success(), succeeds, "{args:?}: {text}");
        assert!(text.contains(said), "{args:?}: {text}");
        assert_eq!(text.matches("[not-parsed]").count(), 1, "{args:?}: {text}");
    }
}

#[test]
fn several_files_are_separated_by_a_heading_that_expanded_source_can_hold() {
    let fixture = Fixture::new("many");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let trees = driver(&fixture).args(["parse".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert!(trees.status.success(), "{}", stderr(&trees));
    assert!(stdout(&trees).contains("=== "), "{}", stdout(&trees));

    // File headings in expanded output are formatted as comments to remain valid SystemVerilog.
    let text = driver(&fixture).args(["pp".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert!(text.status.success(), "{}", stderr(&text));
    for line in stdout(&text).lines().filter(|line| line.contains("=== ")) {
        assert!(line.starts_with("// "), "{line}");
    }
}

#[test]
fn a_file_that_cannot_be_read_fails_the_run_and_not_the_files_after_it() {
    let fixture = Fixture::new("many-failing");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file(
        "b.sv",
        "module b;\n  logic q;\n  lost u_lost ();\nendmodule\n",
    );
    let run = |args: &[&str]| {
        let mut argv: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
        argv.extend([a.as_os_str(), "no-such-file.sv".as_ref(), b.as_os_str()]);
        let output = driver(&fixture).args(argv);
        assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
        (stdout(&output), stderr(&output))
    };

    let (text, said) = run(&["lex"]);
    assert!(text.contains("b.sv ==="), "{text}");
    assert!(text.contains("2 of 3 files, "), "{text}");
    assert!(said.contains("1 of 3 files failed"), "{said}");

    // The design is analysed without what the file would have declared,
    // and the run fails once the rest is reported.
    let (_, said) = run(&["check"]);
    assert!(said.contains("`lost`"), "{said}");
    assert!(
        said.ends_with("1 of 3 files failed, and 1 has errors\n"),
        "{said}"
    );
    let (_, said) = run(&["lint"]);
    assert!(said.contains("unused-signal"), "{said}");
    assert!(said.ends_with("1 of 3 files failed\n"), "{said}");
    let (text, said) = run(&["elab", "--top", "b"]);
    assert!(text.contains("u_lost: lost, declared nowhere"), "{text}");
    assert!(said.ends_with("1 of 3 files failed\n"), "{said}");
    // On its own, the file's problem is all there is to say.
    let output = driver(&fixture).args(["check", "no-such-file.sv"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr(&output).lines().count(), 1, "{}", stderr(&output));
}

#[test]
fn the_output_is_the_order_the_files_were_named_whatever_the_threads_did() {
    let fixture = Fixture::new("ordered");
    // Varying file sizes to ensure output order matches input order regardless
    // of completion order, with a file that cannot be read keeping its place.
    let mut files: Vec<_> = (0..24)
        .map(|at| {
            let body = "module m; endmodule\n".repeat(1 + (at * 37) % 200);
            fixture.file(&format!("f{at:02}.sv",), &body)
        })
        .collect();
    files.insert(7, fixture.path().join("missing.sv"));
    let args: Vec<_> = files.iter().map(|file| file.as_os_str()).collect();

    let headings = |jobs: &str| {
        let mut argv = vec!["lex".as_ref(), "-j".as_ref(), jobs.as_ref()];
        argv.extend(args.iter().copied());
        let output = driver(&fixture).args(argv);
        assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
        stdout(&output)
            .lines()
            .filter(|line| line.starts_with("==="))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };

    let named: Vec<_> = files
        .iter()
        .map(|file| format!("=== {} ===", file.display()))
        .collect();
    assert_eq!(headings("1"), named);
    assert_eq!(headings("4"), named);
}

#[test]
fn parse_reports_what_the_seeding_pass_found() {
    let fixture = Fixture::new("diag-parse");
    let file = fixture.file("wrong.sv", WRONG);

    // Macro definitions passed to parse trigger a seeding pass that reports preprocessor errors.
    let output = driver(&fixture).args([
        "parse".as_ref(),
        "--quiet".as_ref(),
        "-D".as_ref(),
        "FOO=1".as_ref(),
        file.as_os_str(),
    ]);

    assert!(
        stderr(&output).contains("[include-not-found]"),
        "{}",
        stderr(&output)
    );
    assert!(!output.status.success());
}
