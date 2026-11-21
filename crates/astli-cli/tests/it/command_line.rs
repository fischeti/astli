//! The command line: filelists, the plus-separated spellings, and what a
//! wrong one says.

use crate::support::*;

#[test]
fn a_command_line_that_is_wrong_exits_differently_from_a_file_that_is() {
    let fixture =
        Fixture::new("a-command-line-that-is-wrong-exits-differently-from-a-file-that-is");
    let output = driver(&fixture).args(["parse", "--no-such-flag"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--no-such-flag"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_filelist_supplies_the_sources_and_the_build() {
    let fixture = Fixture::new("flist");
    fixture.file(
        "rtl/top.sv",
        "`include \"macros.svh\"\nlocalparam int W = `WIDTH;\n`SHOUT\nlocalparam string S = `NAME;\n",
    );
    fixture.file(
        "inc/macros.svh",
        "`define SHOUT initial $display(\"hi\");\n",
    );
    // A backslash keeps a quote, as simulators read it.
    let list = fixture.file(
        "design.f",
        "// generated\n+incdir+inc\n+define+WIDTH=32\n+define+NAME=\\\"top\\\"\nrtl/top.sv\n",
    );

    // -F resolves relative paths against the directory containing the filelist.
    let output = driver(&fixture).args(["pp".as_ref(), "-F".as_ref(), list.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 32;"), "{text}");
    assert!(text.contains("initial $display(\"hi\");"), "{text}");
    assert!(text.contains("localparam string S = \"top\";"), "{text}");
}

#[test]
fn a_hash_comments_out_the_rest_of_a_filelist_line() {
    let fixture = Fixture::new("flist-hash");
    fixture.file("rtl/a#1.sv", "module a; endmodule\n");
    fixture.file("rtl/b.sv", "module b; endmodule\n");
    let list = fixture.file(
        "design.f",
        "# generated, $UNSET\nrtl/a#1.sv # rtl/c.sv\n  #rtl/d.sv\nrtl/b.sv\n",
    );

    let output = driver(&fixture).args(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("a#1.sv ==="), "{text}");
    assert!(text.contains("b.sv ==="), "{text}");
}

#[test]
fn a_flag_beats_a_filelist_for_the_same_name() {
    let fixture = Fixture::new("flist-override");
    fixture.file("rtl/top.sv", "localparam int W = `WIDTH;\n");
    let list = fixture.file("design.f", "+define+WIDTH=32\nrtl/top.sv\n");

    let output = driver(&fixture).args([
        "pp".as_ref(),
        "-F".as_ref(),
        list.as_os_str(),
        "-DWIDTH=64".as_ref(),
    ]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 64;"), "{text}");
}

#[test]
fn one_filelist_names_another() {
    let fixture = Fixture::new("flist-nested");
    fixture.file("rtl/a.sv", "module a; endmodule\n");
    fixture.file("rtl/b.sv", "module b; endmodule\n");
    fixture.file("more.f", "rtl/b.sv\n");
    let list = fixture.file("design.f", "rtl/a.sv\n-F more.f\n");

    let output = driver(&fixture).args(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("a.sv ==="), "{text}");
    assert!(text.contains("b.sv ==="), "{text}");

    // One that is missing is reported at the line that names it.
    let outer = fixture.file("outer.f", "-F inner.f\n");
    fixture.file("inner.f", "rtl/a.sv\n-F missing.f\n");
    let output = driver(&fixture).args(["lex".as_ref(), "-F".as_ref(), outer.as_os_str()]);
    let text = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(text.contains("inner.f:2:1"), "{text}");
    assert!(text.contains("missing.f: "), "{text}");
}

#[test]
fn a_filelist_that_names_itself_is_caught() {
    let fixture = Fixture::new("flist-cycle");
    let list = fixture.file("design.f", "-F design.f\n");

    let output = driver(&fixture).args(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("includes itself"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_option_a_filelist_may_not_carry_is_rejected_by_name() {
    let fixture = Fixture::new("flist-unknown");
    let list = fixture.file("design.f", "rtl/a.sv\n-y lib/\n");

    let output = driver(&fixture).args(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);
    let text = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(text.contains("[filelist] Error"), "{text}");
    assert!(text.contains("design.f:2:1"), "{text}");
    assert!(text.contains(" 2 | -y lib/"), "{text}");
}

#[test]
fn a_manifest_that_does_not_parse_is_shown_where_it_goes_wrong() {
    let fixture = Fixture::new("manifest-unparsed");
    fixture.file("ip/astli.toml", "[package]\nname = \"ip\"\nsources = 3\n");
    fixture.file("ip/ip.sv", "module ip;\nendmodule\n");

    let output = driver(&fixture)
        .dir(fixture.path().join("ip"))
        .args(["files", "--diag", "short"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("astli.toml:3:11: error[manifest]: invalid type: integer `3`"),
        "{said}"
    );
}

#[test]
fn nothing_to_read_says_so() {
    let fixture = Fixture::new("nothing-to-read-says-so");
    let output = driver(&fixture).args(["parse"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("no input files"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn the_plus_separated_spellings_carry_a_whole_build() {
    let fixture = Fixture::new("plusargs");
    fixture.file(
        "inc/macros.svh",
        "`define SHOUT initial $display(\"hi\");\n",
    );
    fixture.file("inc2/more.svh", "`define ALSO wire also;\n");
    let file = fixture.file(
        "top.sv",
        "`include \"macros.svh\"\n`include \"more.svh\"\n\
         localparam int W = `WIDTH;\n`SHOUT\n`ALSO\n",
    );

    // Multiple directories and definitions combined with plus separators.
    let incdir = format!(
        "+incdir+{}+{}",
        fixture.path().join("inc").display(),
        fixture.path().join("inc2").display()
    );
    let output = driver(&fixture).args([
        "pp".as_ref(),
        file.as_os_str(),
        incdir.as_ref(),
        "+define+WIDTH=32+UNUSED".as_ref(),
    ]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 32;"), "{text}");
    assert!(text.contains("initial $display(\"hi\");"), "{text}");
    assert!(text.contains("wire also;"), "{text}");
}

#[test]
fn a_plus_separated_option_may_sit_anywhere_among_the_files() {
    let fixture = Fixture::new("plusargs-order");
    let a = fixture.file("a.sv", "localparam int W = `WIDTH;\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let output = driver(&fixture).args([
        "pp".as_ref(),
        a.as_os_str(),
        "+define+WIDTH=8".as_ref(),
        b.as_os_str(),
    ]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 8;"), "{text}");
    assert!(text.contains("module b;"), "{text}");
}

#[test]
fn the_dash_spelling_is_the_later_one_whatever_the_order() {
    let fixture = Fixture::new("plusargs-precedence");
    let file = fixture.file("top.sv", "localparam int W = `WIDTH;\n");

    // Test both argument orders to verify consistent precedence.
    for argv in [
        ["+define+WIDTH=32", "-DWIDTH=64"],
        ["-DWIDTH=64", "+define+WIDTH=32"],
    ] {
        let output = driver(&fixture).args([
            "pp".as_ref(),
            file.as_os_str(),
            argv[0].as_ref(),
            argv[1].as_ref(),
        ]);
        let text = stdout(&output);

        assert!(output.status.success(), "{}", stderr(&output));
        assert!(text.contains("localparam int W = 64;"), "{argv:?}: {text}");
    }
}

#[test]
fn a_plus_separated_option_nothing_takes_is_not_read_as_a_file() {
    let fixture = Fixture::new("plusargs-unknown");
    let file = fixture.file("top.sv", TINY);

    let output = driver(&fixture).args(["pp".as_ref(), file.as_os_str(), "+libext+.sv".as_ref()]);
    let text = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(text.contains("+libext+.sv"), "{text}");
    assert!(text.contains("not a file"), "{text}");
}

#[test]
fn the_command_spec_builds() {
    let fixture = Fixture::new("the-command-spec-builds");
    // The site's command reference is generated from it, and building it is
    // where two flags answering to one name are found.
    let output = driver(&fixture).args(["__usage_spec__"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("lint"));
}
