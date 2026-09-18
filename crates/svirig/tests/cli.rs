//! The driver as a reader meets it: a process, its output, and what it exits
//! with.
//!
//! Run against the built binary rather than against the command functions,
//! because what a driver gets wrong is the parts a library call does not have
//! -- an exit code, which stream something went to, a flag that reaches
//! nothing. The crate is a binary and has no library to link against anyway.
//!
//! The fixtures are written to a temporary directory instead of being
//! committed. A path is the one thing these tests cannot hold in memory, so
//! they make the smallest real one they can and take it away again.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// A directory that goes away when the test does.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("svirig-cli-{name}-{}", std::process::id()));
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

fn svirig<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_svirig"))
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

    let output = svirig(["lex".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("MODULE_KW@0..6 \"module\""), "{text}");
    assert!(text.contains("round-trips: true"), "{text}");
}

#[test]
fn lex_without_trivia_hides_whitespace() {
    let fixture = Fixture::new("lex-no-trivia");
    let file = fixture.file("tiny.sv", TINY);

    let output = svirig(["lex".as_ref(), file.as_os_str(), "--no-trivia".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!text.contains("WHITESPACE"), "{text}");
    assert!(text.contains("MODULE_KW"), "{text}");
}

#[test]
fn preprocess_expands_a_macro() {
    let fixture = Fixture::new("pp");
    let file = fixture.file("uses_macro.sv", USES_MACRO);

    let output = svirig(["preprocess".as_ref(), file.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("always_ff @(posedge clk_i)"), "{text}");
    // The definitions themselves are gone, which is what expansion means.
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

    let output = svirig([
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

    let output = svirig(["pp".as_ref(), file.as_os_str(), "-DWIDTH=16".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("localparam int W = 16;"), "{text}");
}

#[test]
fn a_command_line_define_is_in_the_table_and_says_where_it_came_from() {
    let fixture = Fixture::new("pp-table");
    let file = fixture.file("tiny.sv", TINY);

    let output = svirig([
        "pp".as_ref(),
        file.as_os_str(),
        "-DSYNTHESIS".as_ref(),
        "--emit".as_ref(),
        "table".as_ref(),
    ]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("SYNTHESIS"), "{text}");
    assert!(text.contains("[<command-line>]"), "{text}");
}

#[test]
fn parse_prints_a_tree_that_round_trips() {
    let fixture = Fixture::new("parse");
    let file = fixture.file("tiny.sv", TINY);

    let output = svirig(["parse".as_ref(), file.as_os_str()]);
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

    let output = svirig(["parse".as_ref(), file.as_os_str(), "--quiet".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!text.contains("MODULE_DECL@"), "{text}");
    assert!(text.contains("nodes"), "{text}");
    assert!(text.contains("Mtok/s"), "{text}");
}

#[test]
fn the_summary_is_over_the_run_and_not_over_a_file() {
    let fixture = Fixture::new("summary");
    let a = fixture.file("a.sv", "module a; endmodule\n");
    let b = fixture.file("b.sv", "module b; endmodule\n");

    let output = svirig(["lex".as_ref(), a.as_os_str(), b.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    // One line about two files, at the end, and nothing per file before it.
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

    let output = svirig(["lex".as_ref(), "-q".as_ref(), a.as_os_str(), b.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    // Headings too: with nothing under them they would be the whole output.
    assert!(!text.contains("==="), "{text}");
    assert!(!text.contains("MODULE_KW"), "{text}");
    assert!(text.trim_start().starts_with("2 file(s), "), "{text}");
}

#[test]
fn the_summary_says_how_much_of_the_run_it_covers() {
    let fixture = Fixture::new("summary-partial");
    let a = fixture.file("a.sv", "module a; endmodule\n");

    let output = svirig([
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
    let output = svirig(["parse", "no-such-file.sv"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert!(
        stderr(&output).contains("no-such-file.sv"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn fmt_says_it_is_not_implemented_rather_than_pretending() {
    let output = svirig(["fmt", "anything.sv"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert!(
        stderr(&output).contains("not implemented"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_command_line_that_is_wrong_exits_differently_from_a_file_that_is() {
    let output = svirig(["parse", "--no-such-flag"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--no-such-flag"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn completion_writes_a_script_for_each_shell() {
    for shell in ["bash", "zsh", "fish"] {
        let output = svirig(["completion", shell]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(stdout(&output).contains("svirig"), "{shell}");
    }
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

    // `-F`, so that the paths in it are the filelist's own and the test does
    // not depend on where it was run from.
    let output = svirig(["pp".as_ref(), "-F".as_ref(), list.as_os_str()]);
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

    let output = svirig([
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

    let output = svirig(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("a.sv ==="), "{text}");
    assert!(text.contains("b.sv ==="), "{text}");
}

#[test]
fn a_filelist_that_names_itself_is_caught() {
    let fixture = Fixture::new("flist-cycle");
    let list = fixture.file("design.f", "-F design.f\n");

    let output = svirig(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);

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

    let output = svirig(["lex".as_ref(), "-F".as_ref(), list.as_os_str()]);
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

    let trees = svirig(["parse".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert!(trees.status.success(), "{}", stderr(&trees));
    assert!(stdout(&trees).contains("=== "), "{}", stdout(&trees));

    // The expanded source is read by something else, so its heading is a
    // comment and the output is still a SystemVerilog file.
    let text = svirig(["pp".as_ref(), a.as_os_str(), b.as_os_str()]);
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

    let output = svirig([
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
fn nothing_to_read_says_so() {
    let output = svirig(["parse"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("no input files"),
        "{}",
        stderr(&output)
    );
}
