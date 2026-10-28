//! Integration tests for the astli CLI driver.
//!
//! Tests execute the compiled `astli` binary against temporary file fixtures
//! to verify argument parsing, exit codes, output streams (stdout/stderr),
//! and subcommand behavior.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const TINY: &str = "\
module tiny #(
  parameter int unsigned Width = 8
) (
  input  logic [Width-1:0] a_i,
  output logic [Width-1:0] z_o
);
  assign z_o = ~a_i;
endmodule
";

const USES_MACRO: &str = "\
`define WIDTH 8
`define REG(q, d) always_ff @(posedge clk_i) q <= d

module uses_macro (input logic clk_i, input logic [`WIDTH-1:0] d_i, output logic [`WIDTH-1:0] q_o);
  `REG(q_o, d_i);
endmodule
";

/// Temporary directory fixture automatically cleaned up on drop.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("astli-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        Fixture(dir)
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a directory for the file");
        }
        std::fs::write(&path, text).expect("a written fixture");
        path
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn astli<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_astli"))
        .args(args)
        .output()
        .expect("the driver runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 on stdout")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8 on stderr")
}

#[test]
fn lex_prints_kinds_and_round_trips() {
    let fixture = Fixture::new("lex");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli(["lex".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("MODULE_KW@0..6 \"module\""), "{text}");
    assert!(text.contains("round-trips: true"), "{text}");
}

#[test]
fn lex_without_trivia_hides_whitespace() {
    let fixture = Fixture::new("lex-no-trivia");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli(["lex".as_ref(), file.as_os_str(), "--no-trivia".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!text.contains("WHITESPACE"), "{text}");
    assert!(text.contains("MODULE_KW"), "{text}");
}

#[test]
fn preprocess_expands_a_macro() {
    let fixture = Fixture::new("pp");
    let file = fixture.file("uses_macro.sv", USES_MACRO);

    let output = astli(["preprocess".as_ref(), file.as_os_str()]);
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

    let output = astli([
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

    let output = astli(["pp".as_ref(), file.as_os_str(), "-DWIDTH=16".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 16;"), "{text}");
}

#[test]
fn a_command_line_define_is_in_the_table_and_says_where_it_came_from() {
    let fixture = Fixture::new("pp-table");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli([
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

    let output = astli([
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

    let guessed = stdout(&astli(["parse".as_ref(), file.as_os_str()]));
    let told = stdout(&astli([
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

    let bare = stdout(&astli(["parse".as_ref(), "-q".as_ref(), file.as_os_str()]));
    let built = stdout(&astli([
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

    let output = astli(["parse".as_ref(), "--expand".as_ref(), file.as_os_str()]);
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

    let output = astli(["parse".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.starts_with("SOURCE_FILE@"), "{text}");
    assert!(text.contains("MODULE_DECL@"), "{text}");
    assert!(text.contains("round-trips: true"), "{text}");
}

#[test]
fn quiet_prints_the_summary_and_not_the_tree() {
    let fixture = Fixture::new("parse-quiet");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli(["parse".as_ref(), file.as_os_str(), "--quiet".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!text.contains("MODULE_DECL@"), "{text}");
    assert!(text.starts_with("1 file, "), "{text}");
}

#[test]
fn the_summary_is_over_the_run_and_not_over_a_file() {
    let fixture = Fixture::new("summary");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let output = astli(["lex".as_ref(), a.as_os_str(), b.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    // Summary line reporting total files processed.
    let totals: Vec<_> = text.lines().filter(|line| line.contains("files")).collect();
    assert_eq!(totals.len(), 1, "{text}");
    assert!(totals[0].starts_with("2 files, "), "{text}");
    assert_eq!(text.matches("round-trips:").count(), 1, "{text}");
}

#[test]
fn quiet_over_several_files_prints_only_the_summary() {
    let fixture = Fixture::new("quiet-many");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let output = astli(["lex".as_ref(), "-q".as_ref(), a.as_os_str(), b.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    // File headings are omitted in quiet mode.
    assert!(!text.contains("==="), "{text}");
    assert!(!text.contains("MODULE_KW"), "{text}");
    assert!(text.trim_start().starts_with("2 files, "), "{text}");
}

#[test]
fn the_summary_says_how_much_of_the_run_it_covers() {
    let fixture = Fixture::new("summary-partial");
    let a = fixture.file("a.sv", "module a; endmodule\n");

    let output = astli([
        "lex".as_ref(),
        "-q".as_ref(),
        a.as_os_str(),
        "no-such-file.sv".as_ref(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("1 of 2 files, "),
        "{}",
        stdout(&output)
    );
    assert!(
        stderr(&output).contains("1 of 2 files failed"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_file_that_is_not_there_is_reported_with_its_path() {
    let output = astli(["parse", "no-such-file.sv"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert!(
        stderr(&output).contains("no-such-file.sv"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn fmt_writes_a_file_in_place() {
    let fixture = Fixture::new("fmt-write");
    let file = fixture.file("tiny.sv", &TINY.replace("  assign", "      assign"));

    let output = astli(["fmt".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), TINY);
}

#[test]
fn fmt_check_names_nothing_when_everything_is_formatted() {
    let fixture = Fixture::new("fmt-check");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli(["fmt".as_ref(), "--check".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn fmt_diff_is_a_patch_that_formats_the_file() {
    let fixture = Fixture::new("fmt-diff");
    let tidy = fixture.file("tidy.sv", TINY);
    let messy = fixture.file("messy.sv", &TINY.replace("  assign", "      assign"));

    let output = astli([
        "fmt".as_ref(),
        "--diff".as_ref(),
        tidy.as_os_str(),
        messy.as_os_str(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let messy = messy.display();
    assert_eq!(
        stdout(&output),
        format!(
            "\
--- {messy}
+++ {messy}
@@ -4,5 +4,5 @@
   input  logic [Width-1:0] a_i,
   output logic [Width-1:0] z_o
 );
-      assign z_o = ~a_i;
+  assign z_o = ~a_i;
 endmodule
"
        )
    );
}

/// Runs the driver on `file` with `env` alone of the variables that decide
/// colour.
fn astli_coloured(args: &[&str], file: &Path, env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_astli"));
    command
        .args(args)
        .arg(file)
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE");
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().expect("the driver runs")
}

#[test]
fn colour_is_the_flags_then_the_environments() {
    let fixture = Fixture::new("colour");
    let messy = fixture.file("messy.sv", &TINY.replace("  assign", "      assign"));
    let unparsed = fixture.file("unparsed.sv", UNPARSED);

    // A diff on stdout, and a diagnostic on stderr.
    let diff = |color: &str, env: &[(&str, &str)]| {
        let output = astli_coloured(&["--color", color, "fmt", "--diff"], &messy, env);
        output.stdout.contains(&b'\x1b')
    };
    let parse = |color: &str, env: &[(&str, &str)]| {
        let output = astli_coloured(&["--color", color, "parse"], &unparsed, env);
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

/// A module with an item the parser keeps as written.
const UNPARSED: &str = "\
module m;
  + begin
    a;
  end
endmodule
";

#[test]
fn parse_says_what_it_kept_as_written_and_still_succeeds() {
    let fixture = Fixture::new("parse-not-parsed");
    let file = fixture.file("m.sv", UNPARSED);

    let output = astli(["-q".as_ref(), "parse".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    let said = stderr(&output);
    assert!(said.contains("not-parsed"), "{said}");
    assert!(said.contains("m.sv:2:3"), "{said}");
}

#[test]
fn check_fails_on_what_it_did_not_parse() {
    let fixture = Fixture::new("check-not-parsed");
    let file = fixture.file("m.sv", UNPARSED);

    let output = astli([
        "check".as_ref(),
        "--diag".as_ref(),
        "short".as_ref(),
        file.as_os_str(),
    ]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(said.contains("m.sv:2:3: error[not-parsed]"), "{said}");
}

#[test]
fn fmt_keeps_what_it_did_not_parse_without_a_word() {
    let output = astli_with_stdin(["fmt", "-"], UNPARSED);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), UNPARSED);
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn fmt_reports_a_file_that_ends_inside_a_module() {
    let output = astli_with_stdin(["fmt", "-"], "module m;\n  logic q;\n");

    assert!(output.status.success(), "{}", stderr(&output));
    let said = stderr(&output);
    assert!(said.contains("unclosed-at-end-of-file"), "{said}");
    assert!(!said.contains("not-parsed"), "{said}");
}

fn astli_with_stdin<I, S>(args: I, stdin: &str) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut child = Command::new(env!("CARGO_BIN_EXE_astli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the driver runs");
    let mut pipe = child.stdin.take().expect("a pipe to stdin");
    // A command line the driver rejects exits without reading stdin, and may do
    // so before this write; the exit status is what such a test checks.
    match pipe.write_all(stdin.as_bytes()) {
        Err(e) if e.kind() == ErrorKind::BrokenPipe => {}
        written => written.expect("stdin written"),
    }
    drop(pipe);
    child.wait_with_output().expect("the driver finishes")
}

#[test]
fn fmt_formats_stdin_to_stdout() {
    let messy = TINY.replace("  assign", "      assign");

    let output = astli_with_stdin(["fmt", "-"], &messy);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), TINY);
}

#[test]
fn fmt_check_fails_on_stdin_that_is_not_formatted() {
    let messy = TINY.replace("  assign", "      assign");

    let output = astli_with_stdin(["fmt", "--check", "-"], &messy);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "<stdin>\n");
}

#[test]
fn fmt_formats_stdin_on_its_own() {
    let fixture = Fixture::new("fmt-stdin-alone");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli_with_stdin(["fmt".as_ref(), "-".as_ref(), file.as_os_str()], TINY);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
}

#[test]
fn fmt_takes_no_define_so_that_its_output_depends_on_the_file_alone() {
    let output = astli(["fmt", "-D", "X", "anything.sv"]);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_command_line_that_is_wrong_exits_differently_from_a_file_that_is() {
    let output = astli(["parse", "--no-such-flag"]);

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
        "`include \"macros.svh\"\nlocalparam int W = `WIDTH;\n`SHOUT\n",
    );
    fixture.file(
        "inc/macros.svh",
        "`define SHOUT initial $display(\"hi\");\n",
    );
    let list = fixture.file(
        "design.f",
        "// generated\n+incdir+inc\n+define+WIDTH=32\nrtl/top.sv\n",
    );

    // -F resolves relative paths against the directory containing the filelist.
    let output = astli(["pp".as_ref(), "-F".as_ref(), list.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 32;"), "{text}");
    assert!(text.contains("initial $display(\"hi\");"), "{text}");
}

#[test]
fn a_flag_beats_a_filelist_for_the_same_name() {
    let fixture = Fixture::new("flist-override");
    fixture.file("rtl/top.sv", "localparam int W = `WIDTH;\n");
    let list = fixture.file("design.f", "+define+WIDTH=32\nrtl/top.sv\n");

    let output = astli([
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

    let output = astli(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("a.sv ==="), "{text}");
    assert!(text.contains("b.sv ==="), "{text}");
}

#[test]
fn a_filelist_that_names_itself_is_caught() {
    let fixture = Fixture::new("flist-cycle");
    let list = fixture.file("design.f", "-F design.f\n");

    let output = astli(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);

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

    let output = astli(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);
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

    let output = astli_in(&fixture.path().join("ip"), &["files", "--diag", "short"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("astli.toml:3:11: error[manifest]: invalid type: integer `3`"),
        "{said}"
    );
}

#[test]
fn several_files_are_separated_by_a_heading_that_expanded_source_can_hold() {
    let fixture = Fixture::new("many");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let trees = astli(["parse".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert!(trees.status.success(), "{}", stderr(&trees));
    assert!(stdout(&trees).contains("=== "), "{}", stdout(&trees));

    // File headings in expanded output are formatted as comments to remain valid SystemVerilog.
    let text = astli(["pp".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert!(text.status.success(), "{}", stderr(&text));
    for line in stdout(&text).lines().filter(|line| line.contains("=== ")) {
        assert!(line.starts_with("// "), "{line}");
    }
}

#[test]
fn a_file_that_fails_does_not_stop_the_ones_after_it() {
    let fixture = Fixture::new("many-failing");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let output = astli([
        "lex".as_ref(),
        a.as_os_str(),
        "no-such-file.sv".as_ref(),
        b.as_os_str(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("b.sv ==="), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("1 of 3 files failed"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn the_output_is_the_order_the_files_were_named_whatever_the_threads_did() {
    let fixture = Fixture::new("ordered");
    // Varying file sizes to ensure output order matches input order regardless of completion order.
    let files: Vec<_> = (0..24)
        .map(|at| {
            let body = "module m; endmodule\n".repeat(1 + (at * 37) % 200);
            fixture.file(&format!("f{at:02}.sv",), &body)
        })
        .collect();
    let args: Vec<_> = files.iter().map(|file| file.as_os_str()).collect();

    let headings = |jobs: &str| {
        let mut argv = vec!["lex".as_ref(), "-j".as_ref(), jobs.as_ref()];
        argv.extend(args.iter().copied());
        let output = astli(argv);
        assert!(output.status.success(), "{}", stderr(&output));
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
fn a_failure_keeps_its_place_when_the_files_are_read_at_once() {
    let fixture = Fixture::new("ordered-failing");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let output = astli([
        "lex".as_ref(),
        "-j".as_ref(),
        "4".as_ref(),
        a.as_os_str(),
        "no-such-file.sv".as_ref(),
        b.as_os_str(),
    ]);
    let text = stdout(&output);

    assert_eq!(output.status.code(), Some(1));
    let headings: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("==="))
        .collect();
    assert_eq!(headings.len(), 3, "{text}");
    assert!(headings[1].contains("no-such-file.sv"), "{text}");
    assert!(
        stderr(&output).contains("1 of 3 files failed"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn nothing_to_read_says_so() {
    let output = astli(["parse"]);

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
    let output = astli([
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

    let output = astli([
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
        let output = astli([
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

    let output = astli(["pp".as_ref(), file.as_os_str(), "+libext+.sv".as_ref()]);
    let text = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(text.contains("+libext+.sv"), "{text}");
    assert!(text.contains("not a file"), "{text}");
}

#[test]
fn the_run_flags_work_ahead_of_the_subcommand() {
    let fixture = Fixture::new("global-run");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli(["-q".as_ref(), "lex".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("1 file"), "{text}");
    assert!(!text.contains("MODULE_KW"), "{text}");
}

/// Fixture containing an unresolvable include and an undefined macro reference.
const WRONG: &str = "\
`include \"nowhere.svh\"
module wrong;
  logic [`WIDTH-1:0] q;
endmodule
";

#[test]
fn diagnostics_go_to_stderr_and_leave_the_output_alone() {
    let fixture = Fixture::new("diag-streams");
    let file = fixture.file("wrong.sv", WRONG);

    let output = astli(["preprocess".as_ref(), file.as_os_str()]);

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
fn a_file_that_is_wrong_earns_a_failing_exit_code() {
    let fixture = Fixture::new("diag-exit");
    let wrong = fixture.file("wrong.sv", WRONG);
    let fine = fixture.file("tiny.sv", TINY);

    // A file with errors results in a failing exit code even if output was produced.
    let output = astli(["preprocess".as_ref(), wrong.as_os_str()]);
    assert!(!output.status.success(), "{}", stdout(&output));

    let output = astli(["preprocess".as_ref(), fine.as_os_str()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn a_run_says_how_many_files_are_wrong() {
    let fixture = Fixture::new("diag-count");
    let wrong = fixture.file("wrong.sv", WRONG);
    let fine = fixture.file("tiny.sv", TINY);

    let output = astli(["preprocess".as_ref(), fine.as_os_str(), wrong.as_os_str()]);

    assert!(!output.status.success());
    // Reports error count across multiple input files.
    assert!(
        stderr(&output).contains("1 of 2 files have errors"),
        "{}",
        stderr(&output)
    );
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

    let said = stderr(&astli(["preprocess".as_ref(), file.as_os_str()]));

    assert_eq!(said.matches("[undefined-macro]").count(), 20, "{said}");
    assert!(said.contains("30 errors; the first 20 shown"), "{said}");
}

#[test]
fn a_pipe_gets_no_colour() {
    let fixture = Fixture::new("diag-colour");
    let file = fixture.file("wrong.sv", WRONG);

    // ANSI color escape sequences are disabled when stderr is piped.
    let said = stderr(&astli(["preprocess".as_ref(), file.as_os_str()]));
    assert!(
        !said.contains('\u{1b}'),
        "an escape survived a pipe: {said}"
    );
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

    let said = stderr(&astli([
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
fn github_diagnostics_are_workflow_commands() {
    let fixture = Fixture::new("diag-github");
    let file = fixture.file("macro.sv", "logic [`W-1:0] q;\n");

    let said = stderr(&astli([
        "preprocess".as_ref(),
        "--diag=github".as_ref(),
        file.as_os_str(),
    ]));

    let [line] = said.lines().collect::<Vec<_>>()[..] else {
        panic!("one line expected: {said}")
    };
    assert!(
        line.starts_with("::error title=undefined-macro,file=")
            && line.contains(",line=1,col=8,")
            && line.ends_with("macro.sv:1:8: error[undefined-macro]: `W is not defined"),
        "{said}"
    );
}

#[test]
fn json_diagnostics_are_a_line_each_with_the_calls_they_came_through() {
    let fixture = Fixture::new("diag-json");
    fixture.file("macro.sv", "`define BAD `NOPE\nlogic q = `BAD;\n");

    let output = astli_in(fixture.path(), &["preprocess", "--diag=json", "macro.sv"]);

    expect_test::expect![[r#"
        {"severity":"error","code":"undefined-macro","message":"`NOPE is not defined","path":"macro.sv","start":{"line":2,"column":11,"offset":28},"end":{"line":2,"column":15,"offset":32},"label":"not defined here","labels":[],"notes":["the reference stands as written"],"help":[],"spelled":{"path":"macro.sv","start":{"line":1,"column":13,"offset":12},"end":{"line":1,"column":18,"offset":17}},"expansion":[{"macro":"`BAD","path":"macro.sv","start":{"line":2,"column":11,"offset":28},"end":{"line":2,"column":15,"offset":32}}],"included_from":[],"fix":[]}
    "#]]
    .assert_eq(&stderr(&output));
}

#[test]
fn json_diagnostics_carry_their_fix() {
    let fixture = Fixture::new("diag-json-fix");
    fixture.file("top.sv", "module top;\n  reg q;\nendmodule\n");

    let output = astli_in(fixture.path(), &["lint", "--diag=json", "top.sv"]);

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
    let said = stderr(&astli([
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
    let output = astli_in(
        fixture.path(),
        &["preprocess", "--diag=gitlab", "twice.sv", "fine.sv"],
    );
    expect_test::expect![[r#"
        [
        {"description":"[undefined-macro] `W is not defined","check_name":"undefined-macro","fingerprint":"f579ae42b464d564","severity":"major","location":{"path":"twice.sv","lines":{"begin":1}}},
        {"description":"[undefined-macro] `W is not defined","check_name":"undefined-macro","fingerprint":"f57d1442b467b88d","severity":"major","location":{"path":"twice.sv","lines":{"begin":2}}}
        ]
    "#]]
    .assert_eq(&stderr(&output));
    assert!(!output.status.success());

    let output = astli_in(fixture.path(), &["preprocess", "--diag=gitlab", "fine.sv"]);
    assert_eq!(stderr(&output), "[]\n");
    assert!(output.status.success());
}

#[test]
fn diagnostics_survive_the_parallel_path() {
    let fixture = Fixture::new("diag-parallel");
    let wrong = fixture.file("wrong.sv", WRONG);
    let fine = fixture.file("tiny.sv", TINY);

    // Diagnostics are preserved and ordered correctly across parallel workers.
    let output = astli([
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
}

#[test]
fn parse_reports_what_the_seeding_pass_found() {
    let fixture = Fixture::new("diag-parse");
    let file = fixture.file("wrong.sv", WRONG);

    // Macro definitions passed to parse trigger a seeding pass that reports preprocessor errors.
    let output = astli([
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

fn files_in(fixture: &Fixture, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_astli"));
    command.current_dir(fixture.path()).arg("files").args(args);
    command.output().expect("astli runs")
}

#[test]
fn files_keeps_what_the_top_needs_in_dependency_order() {
    let fixture = Fixture::new("files-top");
    design(&fixture);

    let output = files_in(
        &fixture,
        &[
            "-f", "design.f", "--top", "top", "--order", "--diag", "short",
        ],
    );
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
}

#[test]
fn files_refuses_a_name_declared_twice_that_is_used() {
    let fixture = Fixture::new("files-redeclared-used");
    fixture.file("top.sv", "module top;\n  regfile u ();\nendmodule\n");
    fixture.file("ff.sv", "module regfile;\nendmodule\n");
    fixture.file("latch.sv", "module regfile;\nendmodule\n");

    let output = files_in(&fixture, &["top.sv", "ff.sv", "latch.sv"]);

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
}

#[test]
fn files_warns_of_a_name_declared_twice_that_nothing_uses() {
    let fixture = Fixture::new("files-redeclared-unused");
    fixture.file("a/tb_top.sv", "module tb_top;\nendmodule\n");
    fixture.file("b/tb_top.sv", "module tb_top;\nendmodule\n");

    let unused = files_in(&fixture, &["a/tb_top.sv", "b/tb_top.sv"]);
    let as_top = files_in(&fixture, &["a/tb_top.sv", "b/tb_top.sv", "--top", "tb_top"]);

    assert!(unused.status.success(), "{}", stderr(&unused));
    assert!(stderr(&unused).contains("`tb_top` is declared 2 times and used nowhere"));
    // Named as a top, it is used.
    assert!(!as_top.status.success());
}

#[test]
fn files_without_a_top_keeps_every_file() {
    let fixture = Fixture::new("files-all");
    design(&fixture);

    let output = files_in(&fixture, &["-f", "design.f", "--emit", "files"]);
    assert_eq!(
        stdout(&output),
        "top.sv\ncore.sv\nalu.sv\ncfg_pkg.sv\nspare.sv\n"
    );
}

#[test]
fn files_lists_what_could_be_a_top() {
    let fixture = Fixture::new("files-tops");
    design(&fixture);

    let output = files_in(&fixture, &["-f", "design.f", "--emit", "tops"]);
    assert_eq!(stdout(&output), "top\nspare\n");
}

#[test]
fn files_says_why_a_top_needs_a_file() {
    let fixture = Fixture::new("files-why");
    design(&fixture);

    let output = files_in(
        &fixture,
        &["-f", "design.f", "--top", "top", "--why", "alu.sv"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "top.sv\n\
         core.sv: declares `core`, used at top.sv:2:3\n\
         alu.sv: declares `alu`, used at core.sv:4:3\n"
    );

    let output = files_in(
        &fixture,
        &["-f", "design.f", "--top", "top", "--why", "spare.sv"],
    );
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

    let output = files_in(&fixture, &["-f", "design.f", "--top", "tpo"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no file declares the top `tpo`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn files_fails_and_writes_no_list_when_a_file_does_not_parse() {
    let fixture = Fixture::new("files-parse-error");
    fixture.file("top.sv", "module top; sub u(); endmodule\n");
    fixture.file("sub.sv", "module sub; assign = ; endmodule\n");

    let output = files_in(&fixture, &["--top", "top", "top.sv", "sub.sv"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(stderr(&output).contains("sub.sv"), "{}", stderr(&output));
}

fn pickle_in(fixture: &Fixture, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_astli"));
    command.current_dir(fixture.path()).arg("pickle").args(args);
    command.output().expect("astli runs")
}

#[test]
fn pickle_writes_what_the_top_needs_expanded_and_renamed() {
    let fixture = Fixture::new("pickle-top");
    design(&fixture);

    let output = pickle_in(
        &fixture,
        &[
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
        ],
    );
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

    let output = astli(["pickle".as_ref(), "--expand".as_ref(), file.as_os_str()]);
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

    let output = pickle_in(&fixture, &["-f", "design.f", "--top", "top", "--order"]);
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

    let output = astli(["pickle".as_ref(), file.as_os_str()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "`ifdef SIM\n`define WHERE sim\n\n`else\n`define WHERE syn\n\n`endif\nmodule top;\nendmodule\n`undefineall\n"
    );
}

#[test]
fn raw_pickle_renames_nothing() {
    let fixture = Fixture::new("pickle-raw-rename");
    design(&fixture);

    let output = pickle_in(&fixture, &["-f", "design.f", "--prefix", "p_"]);
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

    let output = files_in(
        &fixture,
        &[
            "top.sv",
            "vendor_ip.sv",
            "ip_pkg.sv",
            "spare.sv",
            "--top",
            "top",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "top.sv\nvendor_ip.sv\nip_pkg.sv\n");
    assert!(
        stderr(&output).contains("vendor_ip.sv: encrypted, so it is kept"),
        "{}",
        stderr(&output)
    );
}

/// `b.sv` uses a macro only `a.sv`'s header defines, `c.sv` one it defines
/// itself only after the use, and one only `d.sv` defines.
fn leaking(fixture: &Fixture) {
    fixture.file("include/checks.svh", "`define CHECK(x) assert (x);\n");
    fixture.file(
        "a.sv",
        "`include \"checks.svh\"\nmodule a;\n  initial `CHECK(1)\nendmodule\n",
    );
    fixture.file("b.sv", "module b;\n  initial `CHECK(1)\nendmodule\n");
    fixture.file(
        "c.sv",
        "module c;\n  localparam int P = `LATE;\n  localparam int Q = `ELSEWHERE;\nendmodule\n\
         `define LATE 1\n",
    );
    fixture.file("d.sv", "`define ELSEWHERE 1\n");
}

#[test]
fn check_says_where_else_an_undefined_macro_is_defined() {
    let fixture = Fixture::new("check-defined-elsewhere");
    leaking(&fixture);

    let args = [
        "check",
        "-I",
        "include",
        "--diag=json",
        "a.sv",
        "b.sv",
        "c.sv",
        "d.sv",
    ];
    let output = astli_in(fixture.path(), &args);

    assert!(!output.status.success());
    // A header's path is joined with the platform's separator, which JSON
    // escapes.
    let said = stderr(&output).replace(r"\\", "/");
    let notes: Vec<String> = (said.lines())
        .map(|line| {
            let notes = line.split_once(r#""notes":["#).unwrap().1;
            notes.split_once(']').unwrap().0.to_string()
        })
        .collect();
    assert_eq!(
        notes,
        [
            r#""the reference stands as written","include/checks.svh:1:1 defines it, and b.sv does not include it""#,
            r#""the reference stands as written","it is defined only after this, at c.sv:5:1""#,
            r#""the reference stands as written","d.sv:1:1 defines it, and c.sv does not include it""#,
        ]
    );
}

#[test]
fn files_says_where_else_an_undefined_macro_is_defined() {
    let fixture = Fixture::new("files-defined-elsewhere");
    leaking(&fixture);

    let output = files_in(&fixture, &["-I", "include", "a.sv", "b.sv"]);

    assert!(!output.status.success());
    let said = stderr(&output).replace('\\', "/");
    assert!(said.contains("b.sv:2:11 ]"), "{said}");
    assert!(
        said.contains("include/checks.svh:1:1 defines it, and b.sv does not include it"),
        "{said}"
    );
}

#[test]
fn several_files_defining_an_undefined_macro_are_counted() {
    let fixture = Fixture::new("check-defined-thrice");
    fixture.file("x.sv", "`define W 8\n");
    fixture.file("y.sv", "`define W 8\n");
    fixture.file("z.sv", "`define W 8\n");
    fixture.file("top.sv", "module top;\n  logic [`W-1:0] q;\nendmodule\n");

    let args = ["check", "--diag=json", "x.sv", "y.sv", "z.sv", "top.sv"];
    let said = stderr(&astli_in(fixture.path(), &args));

    assert!(
        said.contains("x.sv:1:1 and 2 other files define it, none of which top.sv includes"),
        "{said}"
    );
}

#[test]
fn pickle_writes_an_envelope_back_as_it_is_and_renames_nothing_in_it() {
    let fixture = Fixture::new("pickle-encrypted");
    encrypted(&fixture);

    let output = pickle_in(
        &fixture,
        &[
            "top.sv",
            "vendor_ip.sv",
            "ip_pkg.sv",
            "--top",
            "top",
            "--expand",
            "--prefix",
            "p_",
        ],
    );
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

const BLOCKING_FLOP: &str = "\
module flop (input logic clk_i, input logic d_i, output logic q_o);
  always_ff @(posedge clk_i) q_o = d_i;
endmodule
";

#[test]
fn lint_passes_a_clean_file_without_a_word() {
    let fixture = Fixture::new("lint-clean");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli(["lint".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).is_empty() && stderr(&output).is_empty());
}

#[test]
fn lint_fails_on_a_denied_rule_and_names_it() {
    let fixture = Fixture::new("lint-deny");
    let file = fixture.file("flop.sv", BLOCKING_FLOP);

    let output = astli(["lint".as_ref(), file.as_os_str()]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("always-ff-non-blocking"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_rule_named_alone_wins_over_its_group() {
    let fixture = Fixture::new("lint-levels");
    let file = fixture.file("flop.sv", BLOCKING_FLOP);

    let warned = astli(
        [
            "lint",
            "--deny",
            "correctness",
            "--warn",
            "always-ff-non-blocking",
        ]
        .map(std::ffi::OsStr::new)
        .into_iter()
        .chain([file.as_os_str()]),
    );
    let allowed = astli(
        [
            "lint",
            "--allow",
            "always-ff-non-blocking",
            "--deny",
            "correctness",
        ]
        .map(std::ffi::OsStr::new)
        .into_iter()
        .chain([file.as_os_str()]),
    );

    assert!(warned.status.success(), "{}", stderr(&warned));
    assert!(stderr(&warned).contains("always-ff-non-blocking"));
    assert!(allowed.status.success(), "{}", stderr(&allowed));
    assert!(stderr(&allowed).is_empty(), "{}", stderr(&allowed));
}

#[test]
fn lint_refuses_a_name_that_is_no_rule() {
    let output = astli(["lint", "--allow", "no-such-rule", "-"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no-such-rule"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn lint_list_names_every_rule_with_its_group() {
    let output = astli(["lint", "--list"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let listed = stdout(&output);
    let row = listed
        .lines()
        .map(|line| line.split_whitespace().take(3).collect::<Vec<_>>())
        .find(|columns| columns.first() == Some(&"always-ff-non-blocking"));
    assert_eq!(
        row,
        Some(vec!["always-ff-non-blocking", "correctness", "deny"]),
        "{listed}"
    );
}

const WIDTHS: &str = "\
module sub #(parameter int W = 8) (input logic [W-1:0] a_i);
endmodule
module top (input logic [7:0] a_i);
  sub #(.W(16)) u_sub (.a_i(a_i));
endmodule
";

#[test]
fn lint_reads_what_the_top_builds_only_given_it() {
    let fixture = Fixture::new("lint-top");
    fixture.file("top.sv", WIDTHS);

    let built = lint_in(fixture.path(), &["top.sv", "--top", "top"]);
    let alone = lint_in(fixture.path(), &["top.sv"]);
    let missing = lint_in(fixture.path(), &["top.sv", "--top", "tpo"]);

    assert!(built.status.success(), "{}", stderr(&built));
    assert!(
        stderr(&built).contains("[width-expand]") && stderr(&built).contains("as built in `top`"),
        "{}",
        stderr(&built)
    );
    assert!(
        !stderr(&alone).contains("width-expand"),
        "{}",
        stderr(&alone)
    );
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("`tpo`"), "{}", stderr(&missing));
}

#[test]
fn lint_with_a_top_judges_only_what_it_reaches_by_the_design() {
    let fixture = Fixture::new("lint-top-reached");
    fixture.file("astli.toml", "[top.tb_a]\n");
    fixture.file("tb/tb_a.sv", "module tb_a;\n  logic unread;\nendmodule\n");
    fixture.file(
        "tb/tb_b.sv",
        "module tb_b;\n  logic unread = 1'b0;\nendmodule\n",
    );

    let output = lint_in(fixture.path(), &["--diag", "short"]);

    let said = stderr(&output);
    assert!(
        said.contains("tb_a.sv:2:9: warning[unused-signal]"),
        "{said}"
    );
    // No top builds `tb_b`, so the design rules leave it alone, but how it
    // is written is still read.
    assert!(
        !said.contains("tb_b.sv:2:9: warning[unused-signal]")
            && said.contains("tb_b.sv:2:9: warning[variable-initializer]"),
        "{said}"
    );
}

/// Runs `astli lint` in `dir`, where it looks for `astli.toml` first.
fn lint_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_astli"))
        .arg("lint")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the driver runs")
}

#[test]
fn astli_toml_sets_levels_found_from_a_directory_below_it() {
    let fixture = Fixture::new("lint-config");
    fixture.file("astli.toml", "[lint]\nwarn = [\"correctness\"]\n");
    fixture.file("rtl/flop.sv", BLOCKING_FLOP);

    let output = lint_in(&fixture.path().join("rtl"), &["flop.sv"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("Warning"), "{}", stderr(&output));
}

#[test]
fn a_path_in_astli_toml_wins_over_the_flags() {
    let fixture = Fixture::new("lint-config-paths");
    let config = "[lint.paths]\n\"vendor/**\" = { allow = [\"always-ff-non-blocking\"] }\n";
    fixture.file("astli.toml", config);
    fixture.file("vendor/ip/flop.sv", BLOCKING_FLOP);
    fixture.file("rtl/flop.sv", BLOCKING_FLOP);

    let vendored = lint_in(
        fixture.path(),
        &["--deny", "correctness", "vendor/ip/flop.sv"],
    );
    let own = lint_in(fixture.path(), &["rtl/flop.sv"]);

    assert!(vendored.status.success(), "{}", stderr(&vendored));
    assert!(stderr(&vendored).is_empty(), "{}", stderr(&vendored));
    assert!(!own.status.success());
}

#[test]
fn astli_toml_naming_no_rule_is_refused_with_its_path() {
    let fixture = Fixture::new("lint-config-unknown");
    fixture.file(
        "astli.toml",
        "[lint.paths]\n\"*.sv\" = { allow = [\"no-such-rule\"] }\n",
    );
    fixture.file("tiny.sv", TINY);

    let output = lint_in(fixture.path(), &["tiny.sv"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("astli.toml") && said.contains("no-such-rule"),
        "{said}"
    );
}

#[test]
fn a_config_named_on_the_command_line_is_the_one_read() {
    let fixture = Fixture::new("lint-config-explicit");
    fixture.file("astli.toml", "[lint]\ndeny = [\"correctness\"]\n");
    fixture.file("lax.toml", "[lint]\nallow = [\"correctness\"]\n");
    fixture.file("flop.sv", BLOCKING_FLOP);

    let output = lint_in(fixture.path(), &["--config", "lax.toml", "flop.sv"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

/// A module whose ports say their direction first, as lowRISC's style does
/// not.
const PREFIXED_PORTS: &str =
    "module top (\n  input  logic i_a,\n  output logic o_b\n);\n  assign o_b = i_a;\nendmodule\n";

/// What says ports are named by a prefix.
const PREFIX_STYLE: &str = "[lint.rules.port-name-suffix]\ninput = [\"i_*\"]\noutput = [\"o_*\"]\n";

/// Runs `astli lint` in `dir`, with a dependency cache of the fixture's own.
fn lint_with_cache(fixture: &Fixture, dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_astli"))
        .arg("lint")
        .args(args)
        .current_dir(dir)
        .env("ASTLI_CACHE_DIR", fixture.path().join("cache"))
        .output()
        .expect("the driver runs")
}

#[test]
fn astli_toml_sets_a_rules_options() {
    let fixture = Fixture::new("lint-config-options");
    fixture.file("top.sv", PREFIXED_PORTS);

    let lowrisc = lint_in(fixture.path(), &["top.sv"]);
    fixture.file("astli.toml", PREFIX_STYLE);
    let prefixed = lint_in(fixture.path(), &["top.sv"]);

    assert!(
        stderr(&lowrisc).contains("port-name-suffix"),
        "{}",
        stderr(&lowrisc)
    );
    assert!(prefixed.status.success(), "{}", stderr(&prefixed));
    assert!(
        !stderr(&prefixed).contains("port-name-suffix"),
        "{}",
        stderr(&prefixed)
    );
}

#[test]
fn astli_toml_naming_an_option_a_rule_lacks_is_refused_with_its_path() {
    let fixture = Fixture::new("lint-config-option-unknown");
    fixture.file(
        "astli.toml",
        "[lint.rules.port-name-suffix]\nin = [\"i_*\"]\n",
    );
    fixture.file("tiny.sv", TINY);

    let output = lint_in(fixture.path(), &["tiny.sv"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("astli.toml") && said.contains("has no option `in`"),
        "{said}"
    );
}

#[test]
fn a_style_a_dev_dependency_holds_is_extended() {
    let fixture = Fixture::new("lint-config-extends");
    fixture.file(
        "style/astli.toml",
        &format!("[package]\nname = \"style\"\n\n{PREFIX_STYLE}"),
    );
    fixture.file(
        "root/astli.toml",
        "[dev-dependencies]\nstyle = { path = \"../style\" }\n\n[lint]\nextends = \"style\"\n",
    );
    fixture.file("root/top.sv", PREFIXED_PORTS);
    let root = fixture.path().join("root");

    // Found among the package's dependencies, or resolved for files named.
    for args in [&[][..], &["top.sv"]] {
        let output = lint_with_cache(&fixture, &root, args);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(
            !stderr(&output).contains("port-name-suffix"),
            "{}",
            stderr(&output)
        );
    }
}

#[test]
fn a_project_sets_over_the_style_it_extends() {
    let fixture = Fixture::new("lint-config-extends-path");
    fixture.file(
        "style.toml",
        &format!("[lint]\ndeny = [\"lowrisc\"]\n\n{PREFIX_STYLE}"),
    );
    fixture.file(
        "astli.toml",
        "[lint]\nextends = \"style.toml\"\n\n[lint.rules.port-name-suffix]\noutput = [\"*_o\"]\n",
    );
    fixture.file("top.sv", PREFIXED_PORTS);

    let output = lint_in(fixture.path(), &["top.sv"]);

    // The style's level and its `input`, the project's `output`.
    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(said.contains("port `o_b` does not end in `_o`"), "{said}");
    assert!(!said.contains("i_a"), "{said}");
}

#[test]
fn a_style_that_is_a_dependency_or_sets_paths_is_refused() {
    let fixture = Fixture::new("lint-config-extends-refused");
    fixture.file(
        "style/astli.toml",
        &format!("[package]\nname = \"style\"\n\n{PREFIX_STYLE}"),
    );
    fixture.file(
        "paths.toml",
        "[lint.paths]\n\"*.sv\" = { allow = [\"lowrisc\"] }\n",
    );
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nstyle = { path = \"../style\" }\n\n[lint]\nextends = \"style\"\n",
    );
    fixture.file("root/top.sv", PREFIXED_PORTS);
    fixture.file("other/astli.toml", "[lint]\nextends = \"../paths.toml\"\n");
    fixture.file("other/top.sv", PREFIXED_PORTS);

    let dependency = lint_with_cache(&fixture, &fixture.path().join("root"), &["top.sv"]);
    let paths = lint_in(&fixture.path().join("other"), &["top.sv"]);

    assert!(!dependency.status.success());
    assert!(
        stderr(&dependency).contains("[dev-dependencies]"),
        "{}",
        stderr(&dependency)
    );
    assert!(!paths.status.success());
    assert!(
        stderr(&paths).contains("[lint.paths]"),
        "{}",
        stderr(&paths)
    );
}

#[test]
fn lint_fix_rewrites_what_it_can_and_reports_the_rest() {
    let fixture = Fixture::new("lint-fix");
    let source = "module top (\n  input logic a_i\n);\n  reg q;\n  always_ff @(posedge a_i) q = 1'b0;\nendmodule\n";
    fixture.file("top.sv", source);

    let output = lint_in(fixture.path(), &["--fix", "top.sv"]);

    let written = std::fs::read_to_string(fixture.path().join("top.sv")).unwrap();
    assert_eq!(written, source.replace("reg q", "logic q"));
    // The blocking assignment has no fix, and still fails the run.
    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(said.contains("Fixed 1 problem in 1 file"), "{said}");
    assert!(
        said.contains("always-ff-non-blocking") && !said.contains("forbid-reg"),
        "{said}"
    );
}

#[test]
fn a_finding_with_a_fix_says_lint_fix_applies_it() {
    let fixture = Fixture::new("lint-fix-help");
    fixture.file("top.sv", "module top;\n  reg q;\nendmodule\n");
    fixture.file("broken.sv", "module broken;\n  reg q;\n");

    let output = lint_in(fixture.path(), &["top.sv"]);
    let broken = lint_in(fixture.path(), &["broken.sv"]);

    let said = stderr(&output);
    assert!(
        said.contains("Help: `astli lint --fix` fixes this"),
        "{said}"
    );
    // `--fix` leaves a file it did not parse whole.
    assert!(!stderr(&broken).contains("--fix"), "{}", stderr(&broken));
}

#[test]
fn lint_fix_leaves_a_file_it_did_not_parse_whole() {
    let fixture = Fixture::new("lint-fix-unparsed");
    let source = "module top;\n  reg q;\n";
    fixture.file("top.sv", source);

    let output = lint_in(fixture.path(), &["--fix", "top.sv"]);

    assert!(!output.status.success());
    let written = std::fs::read_to_string(fixture.path().join("top.sv")).unwrap();
    assert_eq!(written, source);
    assert!(!stderr(&output).contains("Fixed"), "{}", stderr(&output));
}

#[test]
fn lint_fix_applies_no_fix_of_a_rule_allowed_or_waived() {
    let fixture = Fixture::new("lint-fix-allowed");
    let source =
        "module top;\n  (* astli_allow = \"forbid-reg\" *)\n  reg p;\n  reg q;\nendmodule\n";
    fixture.file("top.sv", source);

    let allowed = lint_in(
        fixture.path(),
        &["--fix", "--allow", "forbid-reg", "top.sv"],
    );
    let unchanged = std::fs::read_to_string(fixture.path().join("top.sv")).unwrap();
    let waived = lint_in(fixture.path(), &["--fix", "top.sv"]);
    let written = std::fs::read_to_string(fixture.path().join("top.sv")).unwrap();

    assert!(allowed.status.success() && waived.status.success());
    assert_eq!(unchanged, source);
    assert_eq!(written, source.replace("reg q", "logic q"));
}

/// Runs `astli fmt` in `dir`, where it looks for `astli.toml` first.
fn fmt_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_astli"))
        .arg("fmt")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the driver runs")
}

#[test]
fn astli_toml_sets_the_layout_found_from_a_directory_below_it() {
    let fixture = Fixture::new("fmt-config");
    fixture.file("astli.toml", "[fmt]\nindent = 4\n");
    fixture.file("rtl/m.sv", "module m;\nlogic q;\nendmodule\n");

    let output = fmt_in(&fixture.path().join("rtl"), &["m.sv"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let written = std::fs::read_to_string(fixture.path().join("rtl/m.sv")).unwrap();
    assert_eq!(written, "module m;\n    logic q;\nendmodule\n");
}

#[test]
fn one_astli_toml_serves_both_lint_and_fmt() {
    let fixture = Fixture::new("fmt-lint-config");
    let config = "[lint]\nallow = [\"correctness\"]\n\n[fmt]\nmax-pad = 0\n";
    fixture.file("astli.toml", config);
    fixture.file("flop.sv", BLOCKING_FLOP);

    let linted = lint_in(fixture.path(), &["flop.sv"]);
    let formatted = fmt_in(fixture.path(), &["flop.sv"]);

    assert!(linted.status.success(), "{}", stderr(&linted));
    assert!(formatted.status.success(), "{}", stderr(&formatted));
}

#[test]
fn astli_toml_with_an_unknown_layout_key_is_refused_with_its_path() {
    let fixture = Fixture::new("fmt-config-unknown");
    fixture.file("astli.toml", "[fmt]\ntabs = true\n");
    fixture.file("tiny.sv", TINY);

    let output = fmt_in(fixture.path(), &["tiny.sv"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("astli.toml") && said.contains("tabs"),
        "{said}"
    );
}

#[test]
fn a_file_astli_toml_excludes_is_left_alone_named_or_found() {
    let fixture = Fixture::new("fmt-config-exclude");
    let config = "[fmt]\nexclude = [\"vendor\", \"rtl/*_old.sv\"]\n";
    fixture.file("astli.toml", config);
    let unformatted = "module m;\nlogic q;\nendmodule\n";
    fixture.file("vendor/ip/m.sv", unformatted);
    fixture.file("rtl/m_old.sv", unformatted);
    fixture.file("rtl/m.sv", unformatted);

    let found = fmt_in(fixture.path(), &["--check"]);
    let named = fmt_in(&fixture.path().join("rtl"), &["--check", "m_old.sv"]);

    assert!(!found.status.success());
    let checked = stdout(&found);
    assert_eq!(checked.lines().count(), 1, "{checked}");
    assert!(Path::new(checked.trim()).ends_with("rtl/m.sv"), "{checked}");
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(stdout(&named).is_empty(), "{}", stdout(&named));
}

#[test]
fn astli_toml_with_a_malformed_exclude_is_refused_with_its_path() {
    let fixture = Fixture::new("fmt-config-exclude-malformed");
    fixture.file("astli.toml", "[fmt]\nexclude = [\"rtl/[ab\"]\n");
    fixture.file("tiny.sv", TINY);

    let output = fmt_in(fixture.path(), &["tiny.sv"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("astli.toml") && said.contains("rtl/[ab"),
        "{said}"
    );
}

#[test]
fn the_command_spec_builds() {
    // The site's command reference is generated from it, and building it is
    // where two flags answering to one name are found.
    let output = astli(["__usage_spec__"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("lint"));
}

#[test]
fn lint_defines_with_d_as_check_does() {
    let fixture = Fixture::new("lint-define");
    let path = fixture.file(
        "m.sv",
        "module m (output logic o_o);\n`ifdef DRIVE\n  assign o_o = 1'b0;\n`endif\nendmodule\n",
    );
    let undriven = |args: &[&str]| {
        let mut all = vec!["lint", "--allow", "suspicious", "--allow", "lowrisc"];
        all.extend(args);
        all.push(path.to_str().unwrap());
        astli(all)
    };
    let output = undriven(&["--deny", "undriven-signal"]);
    assert!(
        stderr(&output).contains("undriven-signal"),
        "{}",
        stderr(&output)
    );
    assert!(!output.status.success());
    let output = undriven(&["--deny", "undriven-signal", "-D", "DRIVE"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

/// Runs `astli` in `dir` with `args`.
/// What a run said on stderr, with `fixture`'s directory written `<dir>`
/// and `/` between the parts of a path, as on every system.
fn said_in(fixture: &Fixture, output: &Output) -> String {
    let dir = fixture.path().to_string_lossy().into_owned();
    stderr(output).replace(&dir, "<dir>").replace('\\', "/")
}

fn astli_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_astli"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the driver runs")
}

/// A package with a unit, a header it includes by suffix, and a testbench.
fn package(fixture: &Fixture) {
    fixture.file("astli.toml", "");
    fixture.file(
        "rtl/core.sv",
        "`include \"regs/defs.svh\"\nmodule core;\nendmodule\n",
    );
    fixture.file("rtl/gen/regs/defs.svh", "localparam int W  =  4;\n");
    fixture.file(
        "tb/tb_core.sv",
        "module tb_core;\n  core u_core ();\nendmodule\n",
    );
}

#[test]
fn files_with_nothing_named_reads_the_package() {
    let fixture = Fixture::new("discover-files");
    package(&fixture);

    let output = astli_in(fixture.path(), &["files", "--top", "tb_core"]);

    assert!(output.status.success(), "{}", stderr(&output));
    // Each path is as the current directory names it, which differs from the
    // fixture's own name by links on macOS and by short names on Windows.
    let listed = stdout(&output);
    let lines: Vec<&str> = listed.lines().collect();
    let is = |line: &str, prefix: &str, path: &str| {
        (line.strip_prefix(prefix)).is_some_and(|rest| Path::new(rest).ends_with(path))
    };
    // The header is not a unit, and a tool finds it through its directory.
    assert!(
        lines.len() == 3
            && is(lines[0], "+incdir+", "rtl/gen")
            && is(lines[1], "", "rtl/core.sv")
            && is(lines[2], "", "tb/tb_core.sv"),
        "{listed}"
    );
}

#[test]
fn files_with_nothing_named_orders_the_package() {
    let fixture = Fixture::new("discover-files-order");
    fixture.file("astli.toml", "");
    fixture.file(
        "rtl/a_core.sv",
        "module a_core;\n  import z_pkg::*;\nendmodule\n",
    );
    fixture.file("rtl/z_pkg.sv", "package z_pkg;\nendpackage\n");

    let output = astli_in(fixture.path(), &["files", "--emit", "files"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let listed = stdout(&output);
    let lines: Vec<&str> = listed.lines().collect();
    // A package found has no order of its own, so its files follow what they
    // depend on rather than their paths.
    assert!(
        lines.len() == 2
            && Path::new(lines[0]).ends_with("rtl/z_pkg.sv")
            && Path::new(lines[1]).ends_with("rtl/a_core.sv"),
        "{listed}"
    );
}

#[test]
fn check_with_nothing_named_checks_the_package() {
    let fixture = Fixture::new("discover-check");
    package(&fixture);
    fixture.file(
        "tb/tb_broken.sv",
        "module tb_broken;\n  gone u ();\nendmodule\n",
    );

    let output = astli_in(fixture.path(), &["check", "--diag", "short"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("tb_broken.sv:2:3: error[unknown-definition]"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn check_with_a_top_reports_only_the_files_it_reaches() {
    let fixture = Fixture::new("check-top-reached");
    package(&fixture);
    fixture.file(
        "tb/tb_broken.sv",
        "module tb_broken;\n  gone u ();\n  )\nendmodule\n",
    );

    let core = astli_in(fixture.path(), &["check", "--top", "tb_core"]);
    let broken = astli_in(
        fixture.path(),
        &["check", "--top", "tb_broken", "--diag", "short"],
    );
    let all = astli_in(fixture.path(), &["check", "--diag", "short"]);

    // Neither what `tb_broken` instantiates nor what it fails to parse is
    // `tb_core`'s.
    assert!(core.status.success(), "{}", stderr(&core));
    assert!(!broken.status.success());
    let said = stderr(&broken);
    assert!(
        said.contains("tb_broken.sv:2:3: error[unknown-definition]")
            && said.contains("tb_broken.sv:3:3: error[not-parsed]"),
        "{said}"
    );
    // A file is counted once, whatever found its errors.
    assert!(
        stderr(&all).ends_with("error: 1 of 3 files have errors\n"),
        "{}",
        stderr(&all)
    );
}

#[test]
fn check_with_a_top_refuses_what_files_refuses_in_a_dependency() {
    let fixture = Fixture::new("check-top-redeclared");
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nip = { path = '../ip' }\n",
    );
    fixture.file("root/top.sv", "module top;\n  ip u_ip ();\nendmodule\n");
    fixture.file("ip/ip.sv", "module ip;\n  ff u_ff ();\nendmodule\n");
    fixture.file("ip/ff.sv", "module ff;\nendmodule\n");
    fixture.file("ip/ff_latch.sv", "module ff;\nendmodule\n");
    let root = fixture.path().join("root");

    let files = astli_with_cache(&fixture, &root, &["files", "--top", "top"]);
    let check = astli_with_cache(&fixture, &root, &["check", "--top", "top"]);
    let without = astli_with_cache(&fixture, &root, &["check"]);

    for output in [&files, &check] {
        assert!(!output.status.success());
        assert!(
            stderr(output).contains("`ff` is declared 2 times and used"),
            "{}",
            stderr(output)
        );
    }
    // Without a top, a dependency's problems are its authors' to fix.
    assert!(without.status.success(), "{}", stderr(&without));
}

/// A package whose bench builds only with `MY_FEATURE` defined, and the
/// manifest `toml`.
fn needs_a_define(fixture: &Fixture, toml: &str) {
    fixture.file("astli.toml", toml);
    fixture.file(
        "tb/tb_chip.sv",
        "module tb_chip;\n`ifndef MY_FEATURE\n  gone u ();\n`endif\nendmodule\n",
    );
}

#[test]
fn a_top_table_says_what_its_top_needs_defined() {
    let fixture = Fixture::new("top-table");
    needs_a_define(&fixture, "[top.tb_chip]\ndefines = ['MY_FEATURE']\n");

    let check = astli_in(fixture.path(), &["check", "--top", "tb_chip"]);
    let files = astli_in(fixture.path(), &["files", "--top", "tb_chip"]);

    assert!(check.status.success(), "{}", stderr(&check));
    // A tool given the list builds what `check` checked.
    assert!(
        stdout(&files).starts_with("+define+MY_FEATURE\n"),
        "{}",
        stdout(&files)
    );
}

#[test]
fn a_top_table_can_name_the_module_it_builds() {
    let fixture = Fixture::new("top-table-alias");
    needs_a_define(
        &fixture,
        "[top.chip]\ntop = 'tb_chip'\ndefines = ['MY_FEATURE']\n",
    );

    let output = astli_in(fixture.path(), &["check", "--top", "chip"]);

    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn a_top_table_named_as_another_module_is_refused() {
    let fixture = Fixture::new("top-table-ambiguous");
    needs_a_define(
        &fixture,
        "[top.chip]\ntop = 'tb_chip'\ndefines = ['MY_FEATURE']\n",
    );
    fixture.file("rtl/chip.sv", "module chip;\nendmodule\n");

    let output = astli_in(fixture.path(), &["check", "--top", "chip"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("`chip` is a module, and the name of `top.chip`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn without_a_top_the_only_table_is_the_default() {
    let fixture = Fixture::new("top-table-only");
    needs_a_define(&fixture, "[top.tb_chip]\ndefines = ['MY_FEATURE']\n");

    let check = astli_in(fixture.path(), &["check"]);
    let files = astli_in(fixture.path(), &["files"]);

    assert!(check.status.success(), "{}", stderr(&check));
    assert!(
        stdout(&files).starts_with("+define+MY_FEATURE\n"),
        "{}",
        stdout(&files)
    );
}

#[test]
fn without_a_top_several_tables_need_a_default() {
    let fixture = Fixture::new("top-table-several");
    let tables = "[top.tb_chip]\ndefines = ['MY_FEATURE']\n\n[top.tb_other]\n";
    needs_a_define(&fixture, tables);
    fixture.file("tb/tb_other.sv", "module tb_other;\nendmodule\n");

    let none = astli_in(fixture.path(), &["check"]);
    let candidates = astli_in(fixture.path(), &["files", "--emit", "tops"]);
    fixture.file(
        "astli.toml",
        &format!("[package]\ndefault-top = 'tb_chip'\n\n{tables}"),
    );
    let defaulted = astli_in(fixture.path(), &["check"]);

    assert!(!none.status.success());
    assert!(
        stderr(&none).contains("no top named, and the manifest has 2: `tb_chip`, `tb_other`"),
        "{}",
        stderr(&none)
    );
    // Listing the candidates builds from none of them.
    assert!(candidates.status.success(), "{}", stderr(&candidates));
    assert!(defaulted.status.success(), "{}", stderr(&defaulted));
}

#[test]
fn tops_defining_a_macro_two_ways_are_refused() {
    let fixture = Fixture::new("top-table-clash");
    needs_a_define(
        &fixture,
        "[top.tb_chip]\ndefines = ['MY_FEATURE', 'W=8']\n\n[top.tb_other]\ndefines = ['W=16']\n",
    );
    fixture.file("tb/tb_other.sv", "module tb_other;\nendmodule\n");

    let output = astli_in(
        fixture.path(),
        &["check", "--top", "tb_chip", "--top", "tb_other"],
    );

    assert!(!output.status.success());
    assert!(
        stderr(&output)
            .contains("`W` is defined as `8` by `top.tb_chip` and as `16` by `top.tb_other`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_dependency_keeps_its_tops_to_itself() {
    let fixture = Fixture::new("top-table-dependency");
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nip = { path = '../ip' }\n",
    );
    fixture.file("root/top.sv", "module top;\n  ip u_ip ();\nendmodule\n");
    fixture.file(
        "ip/astli.toml",
        "[package]\nname = 'ip'\ndefault-top = 'tb_ip'\n\n[top.tb_ip]\ndefines = ['IP_BENCH']\n",
    );
    fixture.file("ip/ip.sv", "module ip;\nendmodule\n");
    let root = fixture.path().join("root");

    let output = astli_with_cache(&fixture, &root, &["files"]);

    // The root has no table, so it builds every file, and defines nothing.
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stdout(&output).contains("+define+"), "{}", stdout(&output));
}

#[test]
fn fmt_with_nothing_named_formats_headers_too() {
    let fixture = Fixture::new("discover-fmt");
    package(&fixture);

    let output = astli_in(fixture.path(), &["fmt", "--check"]);

    assert!(!output.status.success());
    let listed = stdout(&output);
    let last = listed.lines().last().unwrap_or_default();
    assert!(
        Path::new(last).ends_with("rtl/gen/regs/defs.svh"),
        "{listed}"
    );
}

#[test]
fn features_select_the_packages_files() {
    let fixture = Fixture::new("discover-features");
    package(&fixture);
    fixture.file("astli.toml", "[features]\nvip = { sources = [\"vip\"] }\n");
    fixture.file("vip/core_vip.sv", "module core_vip;\nendmodule\n");
    let files = |args: &[&str]| -> Vec<String> {
        let output = astli_in(
            fixture.path(),
            &[&["files", "--emit", "files"], args].concat(),
        );
        assert!(output.status.success(), "{}", stderr(&output));
        let listed = stdout(&output);
        let name = |line: &str| Path::new(line).file_name().unwrap_or_default().to_owned();
        (listed.lines().map(name))
            .map(|name| name.to_string_lossy().into_owned())
            .collect()
    };

    // The package's own runs enable all its features.
    assert_eq!(files(&[]), ["core.sv", "tb_core.sv", "core_vip.sv"]);
    assert_eq!(files(&["--no-features"]), ["core.sv", "tb_core.sv"]);
    assert_eq!(files(&["--no-features", "--features", "vip"]).len(), 3);

    let unknown = astli_in(fixture.path(), &["files", "--features", "nope"]);
    let named = astli_in(fixture.path(), &["files", "--no-features", "rtl/core.sv"]);
    assert!(
        stderr(&unknown).contains("enables `nope`"),
        "{}",
        stderr(&unknown)
    );
    assert!(
        stderr(&named).contains("not from files named"),
        "{}",
        stderr(&named)
    );
}

#[test]
fn nothing_named_and_no_package_is_refused() {
    let fixture = Fixture::new("discover-none");
    fixture.file("lonely.sv", "module lonely;\nendmodule\n");

    let output = astli_in(fixture.path(), &["check"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no package here"),
        "{}",
        stderr(&output)
    );
}

/// Runs `git` in `dir`, the user's and the system's configuration left out.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join("../gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "astli")
        .env("GIT_AUTHOR_EMAIL", "astli@example.com")
        .env("GIT_COMMITTER_NAME", "astli")
        .env("GIT_COMMITTER_EMAIL", "astli@example.com")
        .output()
        .expect("git runs");
    assert!(output.status.success(), "{}", stderr(&output));
    String::from_utf8(output.stdout)
        .expect("UTF-8")
        .trim()
        .to_string()
}

#[test]
fn update_writes_the_lock_and_moves_only_what_it_names() {
    let fixture = Fixture::new("update");
    fixture.file("gitconfig", "");
    for name in ["cells", "axi"] {
        let dir = fixture.path().join(name);
        std::fs::create_dir_all(&dir).expect("a repository");
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(
            &dir,
            &["commit", "--quiet", "--allow-empty", "--message", "one"],
        );
        git(&dir, &["tag", "v1.0.0"]);
    }
    let url = |name: &str| fixture.path().join(name).display().to_string();
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\naxi = {{ git = '{}', version = '1' }}\ncells = {{ git = '{}', version = '1' }}\n",
            url("axi"),
            url("cells")
        ),
    );
    let root = fixture.path().join("root");
    let update = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_astli"))
            .arg("update")
            .args(args)
            .current_dir(&root)
            .env("ASTLI_CACHE_DIR", fixture.path().join("cache"))
            .output()
            .expect("the driver runs");
        assert!(output.status.success(), "{}", stderr(&output));
        said_in(&fixture, &output)
    };

    let first = update(&[]);
    for name in ["cells", "axi"] {
        let dir = fixture.path().join(name);
        git(
            &dir,
            &["commit", "--quiet", "--allow-empty", "--message", "two"],
        );
        git(&dir, &["tag", "v1.1.0"]);
    }
    let named = update(&["cells"]);
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");
    let all = update(&[]);

    assert_eq!(
        first,
        "    Fetching axi (<dir>/axi)\n    Fetching cells (<dir>/cells)\n      \
         Adding axi 1.0.0\n      Adding cells 1.0.0\n"
    );
    assert_eq!(
        named,
        "    Fetching cells (<dir>/cells)\n    Updating cells 1.0.0 -> 1.1.0\n"
    );
    assert!(
        lock.contains("version = \"1.0.0\"") && lock.contains("version = \"1.1.0\""),
        "{lock}"
    );
    assert_eq!(
        all,
        "    Fetching axi (<dir>/axi)\n    Fetching cells (<dir>/cells)\n    Updating axi 1.0.0 -> 1.1.0\n"
    );
}

#[test]
fn update_refuses_a_name_the_lock_does_not_hold() {
    let fixture = Fixture::new("update-unknown");
    fixture.file("astli.toml", "");
    fixture.file("astli.lock", "version = 1\n");

    let output = astli_in(fixture.path(), &["update", "axi"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("`axi` is not in"),
        "{}",
        stderr(&output)
    );
}

/// A repository `cells` under `fixture`, its one commit tagged v1.0.0 and
/// declaring `cells`, and the URL to require it by.
fn cells_repository(fixture: &Fixture) -> String {
    fixture.file("gitconfig", "");
    let dir = fixture.path().join("cells");
    fixture.file("cells/cells.sv", "module cells;\nendmodule\n");
    git(&dir, &["init", "--quiet", "--initial-branch=main"]);
    git(&dir, &["add", "--all"]);
    git(&dir, &["commit", "--quiet", "--message", "one"]);
    git(&dir, &["tag", "v1.0.0"]);
    dir.display().to_string()
}

/// `astli files` in `dir`, with the cache under `fixture`.
fn files_with_cache(fixture: &Fixture, dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_astli"))
        .arg("files")
        .args(args)
        .current_dir(dir)
        .env("ASTLI_CACHE_DIR", fixture.path().join("cache"))
        .output()
        .expect("the driver runs")
}

#[test]
fn a_package_command_reads_its_dependencies_and_writes_the_lock() {
    let fixture = Fixture::new("package-lock");
    let url = cells_repository(&fixture);
    let manifest = |required: &str| {
        format!(
            "[dependencies]\ncells = {{ git = '{url}', version = '{required}' }}\n\
             ip = {{ path = 'vendor/ip' }}\n"
        )
    };
    fixture.file("root/astli.toml", &manifest("1"));
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\n  ip u_ip ();\nendmodule\n",
    );
    fixture.file("root/vendor/ip/ip.sv", "module ip;\nendmodule\n");
    let root = fixture.path().join("root");

    let first = files_with_cache(&fixture, &root, &[]);
    let locked = files_with_cache(&fixture, &root, &["--locked"]);
    fixture.file("root/astli.toml", &manifest("2"));
    let refused = files_with_cache(&fixture, &root, &["--locked"]);

    assert!(first.status.success(), "{}", stderr(&first));
    // Compared with `/`, which Windows prints as `\`.
    let listed = stdout(&first).replace('\\', "/");
    assert!(
        listed.contains("cells.sv") && listed.contains("vendor/ip/ip.sv"),
        "{listed}"
    );
    // Once, from the dependency, not again as the root's own.
    assert_eq!(listed.matches("ip.sv").count(), 1, "{listed}");
    assert_eq!(
        said_in(&fixture, &first),
        "    Fetching cells (<dir>/cells)\n      Adding cells 1.0.0\nChecking out cells 1.0.0\n"
    );
    assert!(root.join("astli.lock").is_file());
    assert!(locked.status.success(), "{}", stderr(&locked));
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("version"), "{}", stderr(&refused));
}

#[test]
fn a_package_only_bender_manages_gets_no_lock_unless_asked() {
    let fixture = Fixture::new("bender-lock");
    let url = cells_repository(&fixture);
    fixture.file(
        "root/Bender.yml",
        &format!(
            "package:\n  name: root\ndependencies:\n  cells: {{ git: '{url}', version: 1.0.0 }}\n"
        ),
    );
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\nendmodule\n",
    );
    let root = fixture.path().join("root");
    git(&root, &["init", "--quiet", "--initial-branch=main"]);

    let run = files_with_cache(&fixture, &root, &[]);
    let refused = files_with_cache(&fixture, &root, &["--locked"]);

    assert!(run.status.success(), "{}", stderr(&run));
    assert!(stdout(&run).contains("cells.sv"), "{}", stdout(&run));
    assert!(!root.join("astli.lock").exists());
    assert!(
        stderr(&refused).contains("is missing"),
        "{}",
        stderr(&refused)
    );
}

#[test]
fn a_feature_brings_in_an_optional_dependency_and_dev_dependencies_are_the_roots() {
    let fixture = Fixture::new("optional-dependency");
    fixture.file(
        "root/astli.toml",
        "[features]\nasic = { dependencies = ['tech'] }\n\
         [dependencies]\ntech = { path = '../tech' }\n\
         [dev-dependencies]\nverif = { path = '../verif' }\n",
    );
    fixture.file("root/top.sv", "module top;\nendmodule\n");
    fixture.file("tech/tech.sv", "module tech;\nendmodule\n");
    fixture.file("verif/verif.sv", "module verif;\nendmodule\n");
    let root = fixture.path().join("root");

    let all = files_with_cache(&fixture, &root, &[]);
    let none = files_with_cache(&fixture, &root, &["--no-features"]);

    assert!(all.status.success(), "{}", stderr(&all));
    assert!(none.status.success(), "{}", stderr(&none));
    let (all, none) = (stdout(&all), stdout(&none));
    assert!(all.contains("tech.sv") && all.contains("verif.sv"), "{all}");
    assert!(
        !none.contains("tech.sv") && none.contains("verif.sv"),
        "{none}"
    );
}

#[test]
fn after_fetch_a_run_needs_no_upstream() {
    let fixture = Fixture::new("fetch");
    let url = cells_repository(&fixture);
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{url}', version = '1' }}\n"),
    );
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\nendmodule\n",
    );
    let root = fixture.path().join("root");

    let fetched = Command::new(env!("CARGO_BIN_EXE_astli"))
        .arg("fetch")
        .current_dir(&root)
        .env("ASTLI_CACHE_DIR", fixture.path().join("cache"))
        .output()
        .expect("the driver runs");
    std::fs::remove_dir_all(fixture.path().join("cells")).expect("upstream removed");
    let offline = files_with_cache(&fixture, &root, &["--locked"]);

    assert!(fetched.status.success(), "{}", stderr(&fetched));
    assert!(
        stderr(&fetched).contains("Fetched 1 package into"),
        "{}",
        stderr(&fetched)
    );
    assert!(offline.status.success(), "{}", stderr(&offline));
    assert!(
        stdout(&offline).contains("cells.sv"),
        "{}",
        stdout(&offline)
    );
}

/// `astli` in `dir` with `args`, the cache under `fixture`.
fn astli_with_cache(fixture: &Fixture, dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_astli"))
        .args(args)
        .current_dir(dir)
        .env("ASTLI_CACHE_DIR", fixture.path().join("cache"))
        .output()
        .expect("the driver runs")
}

/// A root under `fixture` depending on a `cells` repository.
fn depends_on_cells(fixture: &Fixture) -> PathBuf {
    let url = cells_repository(fixture);
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{url}', version = '1' }}\n"),
    );
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\nendmodule\n",
    );
    fixture.path().join("root")
}

#[test]
fn path_prints_where_a_dependency_is_checked_out() {
    let fixture = Fixture::new("path");
    let root = depends_on_cells(&fixture);

    let output = astli_with_cache(&fixture, &root, &["path", "cells"]);
    let unknown = astli_with_cache(&fixture, &root, &["path", "axi"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let dir = PathBuf::from(stdout(&output).trim_end());
    assert!(
        dir.starts_with(fixture.path().join("cache")),
        "{}",
        dir.display()
    );
    assert!(dir.join("cells.sv").is_file(), "{}", dir.display());
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("`axi` is not a dependency"),
        "{}",
        stderr(&unknown)
    );
}

#[test]
fn tree_lists_what_a_shared_dependency_needs_once() {
    let fixture = Fixture::new("tree");
    cells_repository(&fixture);
    let url = |name: &str| fixture.path().join(name).display().to_string();
    for (name, needs) in [("axi", "cells"), ("soc", "axi")] {
        fixture.file(
            &format!("{name}/astli.toml"),
            &format!(
                "[dependencies]\n{needs} = {{ git = '{}', version = '1' }}\n",
                url(needs)
            ),
        );
        let dir = fixture.path().join(name);
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(&dir, &["add", "--all"]);
        git(&dir, &["commit", "--quiet", "--message", "one"]);
        git(&dir, &["tag", "v1.0.0"]);
    }
    fixture.file(
        "root/astli.toml",
        &format!(
            "[package]\nname = 'top'\n[dependencies]\n\
             soc = {{ git = '{}', version = '1' }}\naxi = {{ git = '{}', version = '1' }}\n",
            url("soc"),
            url("axi")
        ),
    );
    let root = fixture.path().join("root");

    let output = astli_with_cache(&fixture, &root, &["tree", "-q"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "top\n\
         ├── axi 1.0.0\n\
         │   └── cells 1.0.0\n\
         └── soc 1.0.0\n\
         \x20   └── axi 1.0.0 (*)\n"
    );
}

#[test]
fn a_dependency_without_a_manifest_is_read_as_the_root_describes_it() {
    let fixture = Fixture::new("described");
    fixture.file("gitconfig", "");
    fixture.file("tcells/src/rtl/cell.sv", "module tc_cell;\nendmodule\n");
    fixture.file("tcells/src/fpga/cell.sv", "module tc_cell;\nendmodule\n");
    fixture.file("tcells/src/include/tcells/defs.svh", "`define TC_W 8\n");
    let upstream = fixture.path().join("tcells");
    git(&upstream, &["init", "--quiet", "--initial-branch=main"]);
    git(&upstream, &["add", "--all"]);
    git(&upstream, &["commit", "--quiet", "--message", "one"]);
    git(&upstream, &["tag", "v1.0.0"]);
    let description = "exclude = [\"src/fpga/**\"]\ninclude-dirs = [\"src/include\"]\n";
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\ntcells = {{ git = '{}', version = '1' }}\n\
             [described.tcells]\n{description}[described.ghost]\n",
            upstream.display()
        ),
    );
    fixture.file(
        "root/top.sv",
        "`include \"tcells/defs.svh\"\nmodule top;\n  logic [`TC_W-1:0] w;\n  tc_cell u_cell ();\nendmodule\n",
    );
    let root = fixture.path().join("root");

    let files = astli_with_cache(&fixture, &root, &["files", "--emit", "files"]);
    let quiet = astli_with_cache(&fixture, &root, &["files", "--emit", "files", "-q"]);
    let tree = astli_with_cache(&fixture, &root, &["tree", "-q"]);
    let check = astli_with_cache(&fixture, &root, &["check"]);
    // Upstream takes the description as its own manifest.
    fixture.file("tcells/astli.toml", &format!("[package]\n{description}"));
    git(&upstream, &["add", "--all"]);
    git(&upstream, &["commit", "--quiet", "--message", "two"]);
    git(&upstream, &["tag", "v1.1.0"]);
    let updated = astli_with_cache(&fixture, &root, &["update", "-q"]);
    let after = astli_with_cache(&fixture, &root, &["files", "--emit", "files"]);

    assert!(files.status.success(), "{}", stderr(&files));
    let listed = stdout(&files).replace('\\', "/");
    assert!(
        listed.contains("src/rtl/cell.sv") && !listed.contains("src/fpga"),
        "{listed}"
    );
    let said = stderr(&files);
    assert!(
        said.contains(
            "note: dependency `tcells` has no astli.toml, so astli uses `[described.tcells]` \
             from this one instead\n  \
             = help: the same keys under `[package]` in a dependency's own astli.toml make its \
             description unnecessary\n"
        ),
        "{said}"
    );
    let ghost = "warning: `[described.ghost]` describes a package the design does not hold\n";
    assert!(said.contains(ghost), "{said}");
    // A note is left out under `-q`, and a warning is not.
    let quiet = stderr(&quiet);
    assert!(
        quiet.contains(ghost) && !quiet.contains("has no astli.toml"),
        "{quiet}"
    );
    // Only the commands about dependencies note the description.
    let checked = stderr(&check);
    assert!(
        checked.contains(ghost) && !checked.contains("has no astli.toml"),
        "{checked}"
    );
    assert!(
        stdout(&tree).contains("tcells 1.0.0 (described)"),
        "{}",
        stdout(&tree)
    );
    assert!(updated.status.success(), "{}", stderr(&updated));
    assert!(after.status.success(), "{}", stderr(&after));
    assert!(
        stderr(&after).contains("`tcells` has its own astli.toml, which is read instead"),
        "{}",
        stderr(&after)
    );
    assert!(!stdout(&after).contains("src/fpga"), "{}", stdout(&after));
}

#[test]
fn a_cache_dir_named_through_dot_dot_reads_its_checkouts() {
    let fixture = Fixture::new("cache-dot-dot");
    fixture.file("gitconfig", "");
    fixture.file("cells/cells.sv", "module cells;\nendmodule\n");
    fixture.file("cells/fpga/cells.sv", "module cells;\nendmodule\n");
    let upstream = fixture.path().join("cells");
    git(&upstream, &["init", "--quiet", "--initial-branch=main"]);
    git(&upstream, &["add", "--all"]);
    git(&upstream, &["commit", "--quiet", "--message", "one"]);
    git(&upstream, &["tag", "v1.0.0"]);
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\ncells = {{ git = '{}', version = '1' }}\n\
             [described.cells]\nexclude = [\"fpga\"]\n",
            upstream.display()
        ),
    );
    let root = fixture.path().join("root");
    let files = |cache: &Path| {
        Command::new(env!("CARGO_BIN_EXE_astli"))
            .args(["files", "--emit", "files", "-q"])
            .current_dir(&root)
            .env("ASTLI_CACHE_DIR", cache)
            .output()
            .expect("the driver runs")
    };

    // Checked out the first time, and read from the store the second, the
    // second time through a relative path.
    let cold = files(&root.join("../cache"));
    let warm = files(Path::new("../cache"));

    for output in [cold, warm] {
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(stderr(&output), "");
        let listed = stdout(&output).replace('\\', "/");
        assert!(
            listed.contains("/cells.sv") && !listed.contains("fpga"),
            "{listed}"
        );
    }
}

#[test]
fn init_names_the_package_after_its_directory_and_writes_once() {
    let fixture = Fixture::new("init");
    fixture.file("ip/ip.sv", "module ip;\nendmodule\n");
    let dir = fixture.path().join("ip");

    let first = astli_in(&dir, &["init"]);
    let again = astli_in(&dir, &["init"]);
    let bender = astli_in(&dir, &["init", "--bender"]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert_eq!(stderr(&first), "     Created astli.toml for `ip`\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("astli.toml")).expect("a manifest"),
        "[package]\nname = \"ip\"\n"
    );
    assert!(!again.status.success());
    assert!(
        stderr(&again).contains("already exists"),
        "{}",
        stderr(&again)
    );
    assert!(!bender.status.success());
}

#[test]
fn init_copies_the_dependencies_of_a_bender_yml_only_when_asked() {
    let fixture = Fixture::new("init-bender");
    fixture.file(
        "Bender.yml",
        "package:\n  name: SoC\n\
         dependencies:\n\
         \x20 axi: { git: \"https://example.com/axi.git\", version: 0.39.8 }\n\
         \x20 cva6: { git: \"https://example.com/cva6.git\", rev: pulp-v2.0.0 }\n\
         \x20 ip: { path: ../ip }\n\
         \x20 vip: { target: test, path: ../vip }\n",
    );
    let manifest = fixture.path().join("astli.toml");

    let left = astli_in(fixture.path(), &["init"]);
    let named = std::fs::read_to_string(&manifest).expect("a manifest");
    std::fs::remove_file(&manifest).expect("the manifest removed");
    let copied = astli_in(fixture.path(), &["init", "--bender"]);

    assert!(left.status.success(), "{}", stderr(&left));
    assert!(
        stderr(&left).contains("still read from Bender.yml"),
        "{}",
        stderr(&left)
    );
    assert_eq!(named, "[package]\nname = \"soc\"\n");
    assert!(copied.status.success(), "{}", stderr(&copied));
    assert_eq!(
        std::fs::read_to_string(&manifest).expect("a manifest"),
        "[package]\nname = \"soc\"\n\n[dependencies]\n\
         axi = { git = \"https://example.com/axi.git\", version = \"0.39.8\" }\n\
         cva6 = { git = \"https://example.com/cva6.git\", rev = \"pulp-v2.0.0\" }\n\
         ip = { path = \"../ip\" }\n"
    );
}

#[test]
fn init_check_passes_while_the_two_lists_agree_and_writes_nothing() {
    let fixture = Fixture::new("init-check");
    fixture.file(
        "Bender.yml",
        "package:\n  name: soc\n\
         dependencies:\n\
         \x20 axi: { git: \"https://example.com/axi.git\", version: 0.39.8 }\n\
         \x20 ip: { path: ../ip }\n",
    );
    let manifest = fixture.path().join("astli.toml");

    let missing = astli_in(fixture.path(), &["init", "--bender", "--check"]);
    let unwritten = !manifest.exists();
    let copied = astli_in(fixture.path(), &["init", "--bender"]);
    let checked = astli_in(fixture.path(), &["init", "--bender", "--check"]);
    std::fs::write(&manifest, "[package]\nname = \"soc\"\n").expect("a manifest");
    let read_from_bender = astli_in(fixture.path(), &["init", "--bender", "--check"]);
    let alone = astli_in(fixture.path(), &["init", "--check"]);

    assert!(!missing.status.success());
    assert!(
        stderr(&missing).contains("no astli.toml here to compare"),
        "{}",
        stderr(&missing)
    );
    assert!(unwritten);
    assert!(copied.status.success(), "{}", stderr(&copied));
    assert!(checked.status.success(), "{}", stderr(&checked));
    assert_eq!(stderr(&checked), "");
    assert!(
        read_from_bender.status.success(),
        "{}",
        stderr(&read_from_bender)
    );
    assert!(!alone.status.success());
}

#[test]
fn init_check_reports_each_dependency_the_two_lists_disagree_on() {
    let fixture = Fixture::new("init-check-differ");
    fixture.file(
        "Bender.yml",
        "package:\n  name: soc\n\
         dependencies:\n\
         \x20 axi: { git: \"https://example.com/axi.git\", version: 0.39.8 }\n\
         \x20 cells: { git: \"https://example.com/cells.git\", version: 1.38.0 }\n\
         \x20 ip: { path: ../ip }\n",
    );
    fixture.file(
        "astli.toml",
        "[package]\nname = \"soc\"\n\n[dependencies]\n\
         axi = { git = \"https://example.com/axi.git\", version = \"0.39.9\" }\n\
         ip = { path = \"../ip\", features = [\"fast\"] }\n\
         cva6 = { git = \"https://example.com/cva6.git\", rev = \"v5\" }\n",
    );
    let before = std::fs::read_to_string(fixture.path().join("astli.toml")).expect("a manifest");

    let output = astli_in(fixture.path(), &["init", "--bender", "--check"]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "error: `axi` comes from different places\n\
         \x20 = note: Bender.yml has `{ git = \"https://example.com/axi.git\", version = \"0.39.8\" }`\n\
         \x20 = note: astli.toml has `{ git = \"https://example.com/axi.git\", version = \"0.39.9\" }`\n\
         error: `cells` is in Bender.yml but not in astli.toml\n\
         error: `cva6` is in astli.toml but not in Bender.yml\n\
         error: astli.toml and Bender.yml disagree on 3 dependencies\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.path().join("astli.toml")).expect("a manifest"),
        before
    );
}

#[test]
fn a_file_put_into_a_checkout_by_hand_is_refused() {
    let fixture = Fixture::new("stray");
    let root = depends_on_cells(&fixture);
    let path = astli_with_cache(&fixture, &root, &["path", "cells"]);
    let dir = PathBuf::from(stdout(&path).trim_end());

    std::fs::write(dir.join("generated.sv"), "module generated;\nendmodule\n")
        .expect("a directory of a checkout stays writable");
    let output = files_with_cache(&fixture, &root, &[]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("generated.sv: not a file of the commit"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn cache_clean_deletes_the_checkouts_and_a_run_fetches_again() {
    let fixture = Fixture::new("cache-clean");
    let root = depends_on_cells(&fixture);
    let cache = fixture.path().join("cache");

    let dir = astli_with_cache(&fixture, &root, &["cache", "dir"]);
    let fetched = astli_with_cache(&fixture, &root, &["fetch"]);
    let cleaned = astli_with_cache(&fixture, &root, &["cache", "clean"]);
    let gone = !cache.exists();
    let again = astli_with_cache(&fixture, &root, &["cache", "clean"]);
    let refetched = files_with_cache(&fixture, &root, &[]);

    assert_eq!(stdout(&dir).trim_end(), cache.display().to_string());
    assert!(fetched.status.success(), "{}", stderr(&fetched));
    assert!(cleaned.status.success(), "{}", stderr(&cleaned));
    assert!(gone, "the cache is still there");
    assert!(
        stderr(&again).starts_with("     Removed nothing, since "),
        "{}",
        stderr(&again)
    );
    assert!(refetched.status.success(), "{}", stderr(&refetched));
    assert!(
        stdout(&refetched).contains("cells.sv"),
        "{}",
        stdout(&refetched)
    );
}

#[test]
fn a_patch_reads_a_dependency_from_a_clone_and_leaves_the_lock_alone() {
    let fixture = Fixture::new("patch");
    let root = depends_on_cells(&fixture);
    let first = files_with_cache(&fixture, &root, &[]);
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");

    let patched = astli_with_cache(&fixture, &root, &["patch", "cells"]);
    let clone = root.join(".astli/patch/cells");
    std::fs::write(clone.join("fifo.sv"), "module fifo;\nendmodule\n").expect("a clone to edit");
    let run = files_with_cache(&fixture, &root, &[]);
    let locked = files_with_cache(&fixture, &root, &["--locked"]);
    let update = astli_with_cache(&fixture, &root, &["update"]);
    let removed = astli_with_cache(&fixture, &root, &["patch", "--remove", "cells"]);
    let after = files_with_cache(&fixture, &root, &[]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert!(patched.status.success(), "{}", stderr(&patched));
    assert!(
        stderr(&patched).contains("Cloning cells at"),
        "{}",
        stderr(&patched)
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".astli/.gitignore")).expect("an ignore file"),
        "*\n"
    );
    let origin = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&clone)
        .output()
        .expect("git runs");
    assert_eq!(
        stdout(&origin).trim_end(),
        fixture.path().join("cells").display().to_string()
    );
    assert!(run.status.success(), "{}", stderr(&run));
    // Once, as the patched dependency's, not again as the root's own.
    assert_eq!(
        stdout(&run).matches("fifo.sv").count(),
        1,
        "{}",
        stdout(&run)
    );
    assert!(
        stderr(&run).contains("`cells` is patched: read from"),
        "{}",
        stderr(&run)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("astli.lock")).expect("a lock"),
        lock
    );
    assert!(!locked.status.success());
    assert!(
        stderr(&locked).contains("`--locked` needs a design without patches"),
        "{}",
        stderr(&locked)
    );
    assert!(!update.status.success());
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(clone.join("fifo.sv").is_file());
    assert!(after.status.success(), "{}", stderr(&after));
    assert!(!stdout(&after).contains("fifo.sv"), "{}", stdout(&after));
}

#[test]
fn a_patch_points_at_a_directory_given() {
    let fixture = Fixture::new("patch-dir");
    let root = depends_on_cells(&fixture);
    fixture.file("work/cells/cells.sv", "module cells;\nendmodule\n");
    fixture.file("work/cells/cdc.sv", "module cdc;\nendmodule\n");

    let patched = astli_with_cache(&fixture, &root, &["clone", "cells", "../work/cells"]);
    let run = files_with_cache(&fixture, &root, &[]);
    let unknown = astli_with_cache(&fixture, &root, &["patch", "axi", "../work/cells"]);

    assert!(patched.status.success(), "{}", stderr(&patched));
    assert!(stdout(&run).contains("cdc.sv"), "{}", stdout(&run));
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("`axi` is not a dependency"),
        "{}",
        stderr(&unknown)
    );
}

#[test]
fn a_snapshot_requires_the_pushed_commit_of_a_clone() {
    let fixture = Fixture::new("snapshot");
    let cells = cells_repository(&fixture);
    let axi = fixture.path().join("axi");
    fixture.file(
        "axi/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{cells}', version = '1' }}\n"),
    );
    git(&axi, &["init", "--quiet", "--initial-branch=main"]);
    git(&axi, &["add", "--all"]);
    git(&axi, &["commit", "--quiet", "--message", "one"]);
    git(&axi, &["tag", "v1.0.0"]);
    fixture.file(
        "root/astli.toml",
        &format!(
            "# The design.\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n\
             cells = {{ git = '{cells}', version = '1' }}\n",
            axi.display()
        ),
    );
    fixture.file("root/top.sv", "module top;\nendmodule\n");
    let root = fixture.path().join("root");
    let clone = root.join(".astli/patch/cells");
    let first = astli_with_cache(&fixture, &root, &["patch", "cells"]);

    fixture.file(
        "root/.astli/patch/cells/fifo.sv",
        "module fifo;\nendmodule\n",
    );
    let dirty = astli_with_cache(&fixture, &root, &["snapshot", "cells"]);
    git(&clone, &["add", "--all"]);
    git(&clone, &["commit", "--quiet", "--message", "fifo"]);
    let unpushed = astli_with_cache(&fixture, &root, &["snapshot", "cells"]);
    git(
        &clone,
        &["push", "--quiet", "origin", "HEAD:refs/heads/fifo"],
    );
    let snapshot = astli_with_cache(&fixture, &root, &["snapshot", "cells"]);
    let after = files_with_cache(&fixture, &root, &["--locked"]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert!(
        stderr(&dirty).contains("has changes not committed"),
        "{}",
        stderr(&dirty)
    );
    assert!(
        stderr(&unpushed).contains("push it first"),
        "{}",
        stderr(&unpushed)
    );
    assert!(snapshot.status.success(), "{}", stderr(&snapshot));
    let commit = git(&clone, &["rev-parse", "HEAD"]);
    let manifest = std::fs::read_to_string(root.join("astli.toml")).expect("a manifest");
    assert_eq!(
        manifest,
        format!(
            "# The design.\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n\
             cells = {{ git = '{cells}', rev = \"{commit}\" }}\n\n[resolve]\npinned = [\"cells\"]\n",
            axi.display()
        )
    );
    assert!(
        stderr(&snapshot).contains("`axi` depends on `cells` `^1` of"),
        "{}",
        stderr(&snapshot)
    );
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");
    assert!(lock.contains(&commit), "{lock}");
    assert_eq!(
        std::fs::read_to_string(root.join(".astli/patch.toml")).expect("the patches"),
        "# What this machine reads each dependency named from, written by `astli patch`.\n\
         [patch]\n"
    );
    assert!(after.status.success(), "{}", stderr(&after));
    assert!(stdout(&after).contains("fifo.sv"), "{}", stdout(&after));
    assert!(clone.join("fifo.sv").is_file());
}
