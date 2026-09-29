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
    assert!(text.starts_with("1 file(s), "), "{text}");
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
    let totals: Vec<_> = text
        .lines()
        .filter(|line| line.contains("file(s)"))
        .collect();
    assert_eq!(totals.len(), 1, "{text}");
    assert!(totals[0].starts_with("2 file(s), "), "{text}");
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
    assert!(text.trim_start().starts_with("2 file(s), "), "{text}");
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
        stdout(&output).contains("1 of 2 file(s), "),
        "{}",
        stdout(&output)
    );
    assert!(
        stderr(&output).contains("1 of 2 file(s) failed"),
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
fn fmt_prints_a_formatted_file_as_it_is() {
    let fixture = Fixture::new("fmt-print");
    let file = fixture.file("tiny.sv", TINY);

    let output = astli(["fmt".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), TINY);
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

/// A module whose `specify` block the parser keeps as written.
const SPECIFY: &str = "\
module m;
  specify
    $setup(d, posedge clk, 1);
  endspecify
endmodule
";

#[test]
fn parse_says_what_it_kept_as_written_and_still_succeeds() {
    let fixture = Fixture::new("parse-not-parsed");
    let file = fixture.file("specify.sv", SPECIFY);

    let output = astli(["-q".as_ref(), "parse".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    let said = stderr(&output);
    assert!(said.contains("not-parsed"), "{said}");
    assert!(said.contains("specify.sv:2:3"), "{said}");
}

#[test]
fn fmt_keeps_what_it_did_not_parse_without_a_word() {
    let output = astli_with_stdin(["fmt", "-"], SPECIFY);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), SPECIFY);
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
fn fmt_cannot_write_stdin_back() {
    let output = astli_with_stdin(["fmt", "--write", "-"], TINY);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
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
    assert!(text.contains("-y"), "{text}");
    assert!(text.contains("design.f:2"), "{text}");
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
        stderr(&output).contains("1 of 3 file(s) failed"),
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
        stderr(&output).contains("1 of 3 file(s) failed"),
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
    assert!(text.contains("1 file(s)"), "{text}");
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
        stderr(&output).contains("1 of 2 file(s) have errors"),
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
        "--diagnostics=short".as_ref(),
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
    assert!(said.contains("2 of 4 file(s) have errors"), "{said}");
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

    let output = files_in(&fixture, &["-f", "design.f", "--top", "top", "--order"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "+incdir+inc\n+define+X=1\ncfg_pkg.sv\nalu.sv\ncore.sv\ntop.sv\n"
    );
    // Declared in no file, which is worth saying but not failing over.
    assert!(
        stderr(&output)
            .contains("core.sv:5:3: `gone` is instantiated here and declared in no file"),
        "{}",
        stderr(&output)
    );
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
        ["lint", "-D", "correctness", "-W", "always-ff-non-blocking"]
            .map(std::ffi::OsStr::new)
            .into_iter()
            .chain([file.as_os_str()]),
    );
    let allowed = astli(
        ["lint", "-A", "always-ff-non-blocking", "-D", "correctness"]
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
    let output = astli(["lint", "-A", "no-such-rule", "-"]);

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

    let vendored = lint_in(fixture.path(), &["-D", "correctness", "vendor/ip/flop.sv"]);
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
