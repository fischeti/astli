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
    assert!(text.contains("[-D]"), "{text}");
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
fn parse_with_stats_prints_the_summary_and_not_the_tree() {
    let fixture = Fixture::new("parse-stats");
    let file = fixture.file("tiny.sv", TINY);

    let output = svirig(["parse".as_ref(), file.as_os_str(), "--stats".as_ref()]);
    let text = stdout(&output);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!text.contains("MODULE_DECL@"), "{text}");
    assert!(text.contains("nodes"), "{text}");
    assert!(text.contains("Mtok/s"), "{text}");
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
    let output = svirig(["parse"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("required"), "{}", stderr(&output));
}

#[test]
fn completion_writes_a_script_for_each_shell() {
    for shell in ["bash", "zsh", "fish"] {
        let output = svirig(["completion", shell]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(stdout(&output).contains("svirig"), "{shell}");
    }
}
