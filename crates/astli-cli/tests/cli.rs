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
    assert!(said.contains("... and 10 more"), "{said}");
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
