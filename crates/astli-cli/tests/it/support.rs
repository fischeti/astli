//! What the CLI tests share: a fixture to run the driver in, the one way
//! to run it, and the sources and packages several commands' tests read.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

pub(crate) const TINY: &str = "\
module tiny #(
  parameter int unsigned Width = 8
) (
  input  logic [Width-1:0] a_i,
  output logic [Width-1:0] z_o
);
  assign z_o = ~a_i;
endmodule
";

/// Temporary directory fixture automatically cleaned up on drop.
pub(crate) struct Fixture(PathBuf);

impl Fixture {
    pub(crate) fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("astli-cli-{name}-{}", std::process::id()));
        let fixture = Fixture(dir);
        fixture.remove();
        std::fs::create_dir_all(fixture.path()).expect("a temporary directory");
        fixture
    }

    pub(crate) fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a directory for the file");
        }
        std::fs::write(&path, text).expect("a written fixture");
        path
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// The cache of a run that asks for none: beside the fixture rather than
    /// in it, where a command reading the package would find it.
    fn no_cache(&self) -> PathBuf {
        let mut path = self.0.clone().into_os_string();
        path.push("-no-cache");
        PathBuf::from(path)
    }

    fn remove(&self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_dir_all(self.no_cache());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.remove();
    }
}

/// What the driver reads from the environment besides `ASTLI_*`: the root
/// GitLab reports paths from, and what decides whether it writes colour.
pub(crate) const READ_FROM_THE_ENVIRONMENT: &[&str] = &[
    "CI_PROJECT_DIR",
    "NO_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "TERM",
    "CI",
];

/// A run of the driver in a fixture, which sees nothing of the environment
/// the driver reads but what the test sets: a developer's or CI's settings
/// would change what it writes, and their cache what it finds.
pub(crate) struct Run {
    command: Command,
    cache: PathBuf,
    stdin: Option<String>,
}

pub(crate) fn driver(fixture: &Fixture) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_astli"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("ASTLI_") {
            command.env_remove(name);
        }
    }
    for name in READ_FROM_THE_ENVIRONMENT {
        command.env_remove(name);
    }
    command
        .current_dir(fixture.path())
        .env("ASTLI_CACHE_DIR", fixture.no_cache());
    Run {
        command,
        cache: fixture.path().join("cache"),
        stdin: None,
    }
}

impl Run {
    /// Runs in `dir` rather than the fixture.
    pub(crate) fn dir(mut self, dir: impl AsRef<Path>) -> Run {
        self.command.current_dir(dir);
        self
    }

    /// Uses the fixture's `cache`, which the test's other runs share.
    pub(crate) fn cache(mut self) -> Run {
        self.command.env("ASTLI_CACHE_DIR", &self.cache);
        self
    }

    /// As if run from a hook or `rebase --exec` in the repository `repo`,
    /// with what `git` sets for one naming it.
    pub(crate) fn hook_in(mut self, repo: &Path) -> Run {
        (self.command)
            .env("GIT_DIR", repo.join(".git"))
            .env("GIT_WORK_TREE", repo)
            .env("GIT_INDEX_FILE", repo.join(".git/index"));
        self
    }

    pub(crate) fn stdin(mut self, text: &str) -> Run {
        self.stdin = Some(text.to_string());
        self
    }

    pub(crate) fn env(mut self, name: &str, value: impl AsRef<std::ffi::OsStr>) -> Run {
        self.command.env(name, value);
        self
    }

    /// Starts the driver with `args`, its stdin and stdout piped, for a test
    /// that talks to it while it runs.
    pub(crate) fn spawn<I, S>(mut self, args: I) -> Child
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        (self.command.args(args))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the driver runs")
    }

    /// Runs the driver with `args` and waits for it.
    pub(crate) fn args<I, S>(mut self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.command.args(args);
        let Some(stdin) = self.stdin else {
            return self.command.output().expect("the driver runs");
        };
        let mut child = (self.command)
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
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 on stdout")
}

pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8 on stderr")
}

/// A module with an item the parser keeps as written.
pub(crate) const UNPARSED: &str = "\
module m;
  + begin
    a;
  end
endmodule
";

/// Fixture containing an unresolvable include and an undefined macro reference.
pub(crate) const WRONG: &str = "\
`include \"nowhere.svh\"
module wrong;
  logic [`WIDTH-1:0] q;
endmodule
";

/// What a run said on stderr, with `fixture`'s directory written `<dir>`
/// and `/` between the parts of a path, as on every system.
pub(crate) fn said_in(fixture: &Fixture, output: &Output) -> String {
    let dir = fixture.path().to_string_lossy().into_owned();
    stderr(output).replace(&dir, "<dir>").replace('\\', "/")
}

/// A package with a unit, a header it includes by suffix, and a testbench.
pub(crate) fn package(fixture: &Fixture) {
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

/// A testbench including a header beside it, with no include directory.
pub(crate) fn include_beside(fixture: &Fixture) {
    fixture.file("astli.toml", "[package]\nname = \"beside\"\n\n[top.tb]\n");
    fixture.file("tb/defs.svh", "`define VAL 7\n");
    fixture.file(
        "tb/tb.sv",
        "`include \"defs.svh\"\n\
         module tb;\n\
         \x20 initial begin\n\
         \x20   $display(\"val %0d\", `VAL);\n\
         \x20   $finish;\n\
         \x20 end\n\
         endmodule\n",
    );
}

/// [`adder`], in the directory `dir` under `fixture`, written with its
/// trailing `/`.
pub(crate) fn adder_in(fixture: &Fixture, dir: &str) {
    fixture.file(
        &format!("{dir}astli.toml"),
        "[package]\nname = \"adder\"\n\n[top.tb]\ndefines = { WIDTH = 8 }\nparams = { B = 100 }\n",
    );
    fixture.file(
        &format!("{dir}include/sum.svh"),
        "`define SUM(a, b) ((a) + (b))\n",
    );
    fixture.file(
        &format!("{dir}rtl/adder.sv"),
        "`include \"sum.svh\"\n\
         module adder #(parameter int W = 4) (\n\
         \x20 input logic [W-1:0] a, b,\n\
         \x20 output logic [W:0] y\n\
         );\n\
         \x20 assign y = `SUM(a, b);\n\
         endmodule\n",
    );
    fixture.file(
        &format!("{dir}tb/tb.sv"),
        "module tb #(parameter int B = 0);\n\
         \x20 logic [`WIDTH-1:0] a, b;\n\
         \x20 logic [`WIDTH:0] y;\n\
         \x20 adder #(.W(`WIDTH)) dut (.a, .b, .y);\n\
         \x20 initial begin\n\
         \x20   a = 200; b = B;\n\
         \x20   #1 assert (y == 300) else $fatal(1, \"wrong sum %0d\", y);\n\
         \x20   $display(\"sum %0d\", y);\n\
         \x20   $finish;\n\
         \x20 end\n\
         endmodule\n",
    );
}

/// Runs `git` in `dir`, the user's and the system's configuration left out.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let output = (astli_testing::git(dir, &dir.join("../gitconfig")).args(args))
        .output()
        .expect("git runs");
    assert!(output.status.success(), "{}", stderr(&output));
    String::from_utf8(output.stdout)
        .expect("UTF-8")
        .trim()
        .to_string()
}

/// A repository `cells` under `fixture`, its one commit tagged v1.0.0 and
/// declaring `cells`, and the URL to require it by.
pub(crate) fn cells_repository(fixture: &Fixture) -> String {
    fixture.file("cells/cells.sv", "module cells;\nendmodule\n");
    repository(fixture, "cells")
}

/// The URL of a repository made of what `fixture` holds under `name`, its
/// one commit tagged `v1.0.0`.
pub(crate) fn repository(fixture: &Fixture, name: &str) -> String {
    fixture.file("gitconfig", "");
    let dir = fixture.path().join(name);
    git(&dir, &["init", "--quiet", "--initial-branch=main"]);
    git(&dir, &["add", "--all"]);
    git(&dir, &["commit", "--quiet", "--message", "one"]);
    git(&dir, &["tag", "v1.0.0"]);
    dir.display().to_string()
}

/// A repository `axi` under `fixture` requiring `cells` at `^1`, its one
/// commit tagged `v1.0.0`.
pub(crate) fn axi_requiring_cells(fixture: &Fixture, cells: &str) -> PathBuf {
    let axi = fixture.path().join("axi");
    fixture.file(
        "axi/astli.toml",
        &format!("[package]\nname = 'axi'\n\n[dependencies]\ncells = {{ git = '{cells}', version = '1' }}\n"),
    );
    fixture.file("axi/axi.sv", "module axi;\nendmodule\n");
    repository(fixture, "axi");
    axi
}

/// What `add`, `remove`, `snapshot` and `bender export` say of `{who}`,
/// whose dependencies its `Bender.yml` lists.
pub(crate) const FROM_BENDER: &str = "error: {who}'s dependencies come from its Bender.yml, since its \
     astli.toml has no `[dependencies]` table\n  \
     = help: `astli bender import` copies them into astli.toml, which then lists them\n";

/// A workspace under `ws/` in `fixture`: its root `soc`, requiring the
/// member `dma` by path, which requires `cells` from its repository, and the
/// member `other`, which requires nothing.
pub(crate) fn workspace(fixture: &Fixture) -> PathBuf {
    let url = cells_repository(fixture);
    std::fs::create_dir_all(fixture.path().join("ws/.git")).expect("a repository");
    fixture.file(
        "ws/astli.toml",
        "[package]\nname = 'soc'\n\n[workspace]\n\n[dependencies]\ndma = { path = 'ip/dma' }\n",
    );
    fixture.file("ws/soc.sv", "module soc;\n  dma u_dma ();\nendmodule\n");
    fixture.file(
        "ws/ip/dma/astli.toml",
        &format!("[package]\nname = 'dma'\n\n[dependencies]\ncells = {{ git = '{url}', version = '1' }}\n"),
    );
    fixture.file(
        "ws/ip/dma/dma.sv",
        "module dma;\n  cells u_cells ();\nendmodule\n",
    );
    fixture.file("ws/ip/other/astli.toml", "[package]\nname = 'other'\n");
    fixture.file("ws/ip/other/other.sv", "module other;\nendmodule\n");
    fixture.path().join("ws")
}
