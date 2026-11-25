//! Workspaces: members, `-p` and `--workspace`.

use std::path::Path;
use std::process::Output;

use crate::support::*;

/// The files `astli files` lists in `dir`, by name.
fn listed_in(fixture: &Fixture, dir: &Path, args: &[&str]) -> Vec<String> {
    let output = driver(fixture)
        .dir(dir)
        .cache()
        .args(["files"].iter().chain(args));
    assert!(output.status.success(), "{}", stderr(&output));
    let mut names: Vec<String> = (stdout(&output).lines())
        .filter_map(|line| Path::new(line).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_workspace_locks_every_member_at_its_root_and_each_builds_its_own_design() {
    let fixture = Fixture::new("workspace");
    let ws = workspace(&fixture);

    let other = listed_in(&fixture, &ws.join("ip/other"), &[]);
    let lock = std::fs::read_to_string(ws.join("astli.lock")).expect("a lock at the root");
    let dma = listed_in(&fixture, &ws.join("ip/dma"), &["--locked"]);
    let soc = listed_in(&fixture, &ws, &["--locked"]);

    assert_eq!(other, ["other.sv"]);
    assert!(lock.contains("name = \"cells\""), "{lock}");
    assert!(!ws.join("ip/other/astli.lock").exists());
    assert!(!ws.join("ip/dma/astli.lock").exists());
    assert_eq!(dma, ["cells.sv", "dma.sv"]);
    assert_eq!(soc, ["cells.sv", "dma.sv", "soc.sv"]);
}

#[test]
fn a_member_shares_the_roots_patches_and_build_and_is_told_what_it_holds_unread() {
    let fixture = Fixture::new("workspace-shared");
    let ws = workspace(&fixture);
    fixture.file(
        "ws/ip/other/astli.toml",
        "[package]\nname = 'other'\n\n[resolve]\npinned = ['nothing']\n\n[build]\n\n\
         [tool.regtool]\nfiles = []\n",
    );
    fixture.file("ws/ip/other/.astli/patch.toml", "[patch]\n");
    fixture.file("ws/.astli/build/verilator/dev/soc/tb/Vtb", "");
    fixture.file("work/cells/cells.sv", "module cells;\nendmodule\n");
    let in_member = |args: &[&str]| driver(&fixture).dir(ws.join("ip/dma")).cache().args(args);

    let tree = in_member(&["tree"]);
    let patched = in_member(&["patch", "cells", "../../../work/cells"]);
    let cleaned = in_member(&["clean"]);

    assert!(tree.status.success(), "{}", stderr(&tree));
    assert_eq!(stdout(&tree), "dma\n└── cells 1.0.0\n");
    let said = said_in(&fixture, &tree);
    assert!(
        said.contains(
            "ws/ip/other/astli.toml`: astli reads `[resolve]`, `[build]` only from the \
             workspace root's astli.toml"
        ),
        "{said}"
    );
    assert!(!said.contains("[tool]"), "{said}");
    assert!(
        said.contains("ws/ip/other/.astli/patch.toml` is not read"),
        "{said}"
    );
    assert!(patched.status.success(), "{}", stderr(&patched));
    let patches = std::fs::read_to_string(ws.join(".astli/patch.toml")).expect("the root's");
    // Compared with `/`, which Windows writes as `\`.
    assert!(
        patches.replace('\\', "/").contains("work/cells"),
        "{patches}"
    );
    assert!(!ws.join("ip/dma/.astli").exists());
    assert!(cleaned.status.success(), "{}", stderr(&cleaned));
    assert!(!ws.join(".astli/build").exists());
}

#[test]
fn verilator_a_member_builds_into_the_workspace_root_under_its_name() {
    let fixture = Fixture::new("verilator-member");
    fixture.file("astli.toml", "[workspace]\n");
    adder_in(&fixture, "ip/adder/");

    let output = driver(&fixture)
        .dir(fixture.path().join("ip/adder"))
        .args(["build"]);
    let built = driver(&fixture).args(["build", "-p", "adder"]);
    let ran = driver(&fixture).args(["run", "-p", "adder"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let dir = fixture.path().join(".astli/build/verilator/dev/adder/tb");
    assert!(dir.join("Vtb").is_file(), "{}", stderr(&output));
    assert!(!fixture.path().join("ip/adder/.astli").exists());
    // From the virtual root, as if run in the member: the same build.
    assert!(built.status.success(), "{}", stderr(&built));
    assert!(!stderr(&built).contains("Building"), "{}", stderr(&built));
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert!(
        stderr(&ran).contains("Running `.astli/build/verilator/dev/adder/tb/Vtb`"),
        "{}",
        stderr(&ran)
    );
}

/// What `output` printed, every path with `/`.
fn slashed(output: &Output) -> String {
    stdout(output).replace('\\', "/")
}

#[test]
fn p_acts_on_the_workspace_package_it_names_from_anywhere_in_it() {
    let fixture = Fixture::new("workspace-p");
    let ws = workspace(&fixture);
    fixture.file(
        "ws/ip/other/astli.toml",
        "[package]\nname = 'other'\n\n[fmt]\nexclude = ['bad.sv']\n",
    );
    fixture.file("ws/ip/other/bad.sv", "module   bad;\nendmodule\n");
    fixture.file("ws/ip/dma/bad.sv", "module   bad;\nendmodule\n");
    let in_other = |args: &[&str]| driver(&fixture).dir(ws.join("ip/other")).cache().args(args);

    let dma = listed_in(&fixture, &ws.join("ip/other"), &["-p", "dma"]);
    let tree = in_other(&["tree", "-p", "soc"]);
    let other = driver(&fixture)
        .dir(&ws)
        .cache()
        .args(["fmt", "--check", "-p", "other"]);
    let unformatted = in_other(&["fmt", "--check", "--package", "dma"]);
    let unknown = in_other(&["tree", "-p", "nothing"]);
    let outside = driver(&fixture)
        .dir(fixture.path().join("cells"))
        .args(["tree", "-p", "cells"]);
    let alone = driver(&fixture)
        .dir(fixture.path().join("cells"))
        .args(["tree", "--workspace"]);

    assert_eq!(dma, ["bad.sv", "cells.sv", "dma.sv"]);
    assert!(tree.status.success(), "{}", stderr(&tree));
    assert_eq!(
        slashed(&tree),
        "soc\n└── dma (ip/dma)\n    └── cells 1.0.0\n"
    );
    assert!(other.status.success(), "{}", stderr(&other));
    assert!(!unformatted.status.success());
    assert!(
        slashed(&unformatted).ends_with("ip/dma/bad.sv\n"),
        "{}",
        stdout(&unformatted)
    );
    assert!(!unknown.status.success());
    let said = said_in(&fixture, &unknown);
    assert!(said.contains("<dir>/ws` is called `nothing`"), "{said}");
    assert!(
        said.contains("its packages are `soc`, `dma`, `other`"),
        "{said}"
    );
    assert!(!outside.status.success());
    assert!(
        stderr(&outside).contains("`--package` chooses among the packages of a workspace"),
        "{}",
        stderr(&outside)
    );
    // A package in no workspace is the only one `--workspace` runs on.
    assert!(alone.status.success(), "{}", stderr(&alone));
    assert_eq!(stdout(&alone), "cells\n");
}

#[test]
fn workspace_runs_once_per_package_and_fails_if_any_fails() {
    let fixture = Fixture::new("workspace-all");
    let ws = workspace(&fixture);
    fixture.file("ws/ip/other/bad.sv", "module   bad;\nendmodule\n");
    let at_root = |args: &[&str]| driver(&fixture).dir(&ws).cache().args(args);

    let tree = at_root(&["tree", "--workspace"]);
    let fmt =
        driver(&fixture)
            .dir(ws.join("ip/dma"))
            .cache()
            .args(["fmt", "--check", "--workspace"]);
    let lint = at_root(&["lint", "--workspace"]);
    let both = at_root(&["check", "--workspace", "-p", "dma"]);
    let named = at_root(&["fmt", "--workspace", "soc.sv"]);
    let files = at_root(&["files", "--workspace"]);

    assert!(tree.status.success(), "{}", stderr(&tree));
    assert_eq!(
        slashed(&tree),
        "soc\n└── dma (ip/dma)\n    └── cells 1.0.0\n\ndma\n└── cells 1.0.0\n\nother\n"
    );
    assert!(!fmt.status.success());
    assert!(
        slashed(&fmt).ends_with("ip/other/bad.sv\n"),
        "{}",
        stdout(&fmt)
    );
    let said = stderr(&fmt);
    assert!(
        said.contains("`other`: 1 of 2 files is not formatted"),
        "{said}"
    );
    assert!(said.contains("1 of 3 packages failed"), "{said}");
    assert!(lint.status.success(), "{}", stderr(&lint));
    assert!(!both.status.success());
    assert!(!named.status.success());
    assert!(
        stderr(&named).contains("`--workspace` acts on the packages' own files"),
        "{}",
        stderr(&named)
    );
    assert!(!files.status.success());
}

#[test]
fn a_member_is_told_only_of_what_its_design_holds() {
    let fixture = Fixture::new("workspace-told");
    let ws = workspace(&fixture);
    let cells = fixture.path().join("cells");
    git(
        &cells,
        &["commit", "--quiet", "--allow-empty", "--message", "two"],
    );
    let two = git(&cells, &["rev-parse", "HEAD"]);
    let url = cells.display().to_string();
    let axi = axi_requiring_cells(&fixture, &url);
    fixture.file(
        "ws/astli.toml",
        &format!(
            "[package]\nname = 'soc'\n\n[workspace]\n\n[dependencies]\n\
             dma = {{ path = 'ip/dma' }}\ncells = {{ git = '{url}', rev = '{two}' }}\n\n\
             [resolve]\npinned = ['cells']\n\n[described.cells]\nsources = ['cells.sv']\n"
        ),
    );
    fixture.file(
        "ws/ip/dma/astli.toml",
        &format!(
            "[package]\nname = 'dma'\n\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n",
            axi.display()
        ),
    );

    let dma = driver(&fixture)
        .dir(ws.join("ip/dma"))
        .cache()
        .args(["tree"]);
    let other = driver(&fixture)
        .dir(ws.join("ip/other"))
        .cache()
        .args(["tree"]);

    assert!(dma.status.success(), "{}", stderr(&dma));
    let said = stderr(&dma);
    assert!(
        said.contains("`axi` depends on `cells`") && said.contains("the workspace root pins it"),
        "{said}"
    );
    assert!(
        said.contains("uses `[described.cells]` from the workspace root's astli.toml instead"),
        "{said}"
    );
    // Its design holds neither `axi` nor `cells`.
    assert!(other.status.success(), "{}", stderr(&other));
    assert_eq!(stderr(&other), "", "{}", stderr(&other));
}

#[test]
fn check_workspace_tells_a_finding_in_a_sibling_once() {
    let fixture = Fixture::new("workspace-check");
    let ws = workspace(&fixture);
    fixture.file(
        "ws/ip/dma/dma.sv",
        "module dma;\n  cells u_cells (.missing(1'b0));\nendmodule\n",
    );

    let output = driver(&fixture)
        .dir(&ws)
        .cache()
        .args(["check", "--workspace"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    // Told by the run that finds it first, the root's.
    assert_eq!(said.matches("[unknown-port]").count(), 1, "{said}");
    assert!(said.contains("`soc`: 1 of 2 files has errors"), "{said}");
    // But fails the member's run too, whose own file it is in.
    assert!(said.contains("2 of 3 packages failed"), "{said}");
}

#[test]
fn check_workspace_checks_every_table_of_a_member_and_goes_on_past_it() {
    let fixture = Fixture::new("workspace-tops");
    let ws = workspace(&fixture);
    // Several tables and no default, the second of a module no file holds.
    fixture.file(
        "ws/ip/other/astli.toml",
        "[package]\nname = 'other'\n\n[top.one]\ntop = 'other'\ndefines = { W = 1 }\n\n\
         [top.two]\ntop = 'gone'\ndefines = { W = 2 }\n",
    );
    fixture.file(
        "ws/soc.sv",
        "module soc;\n  dma u_dma (.missing(1'b0));\nendmodule\n",
    );

    let output = driver(&fixture)
        .dir(&ws)
        .cache()
        .args(["check", "--workspace"]);
    let json = driver(&fixture)
        .dir(&ws)
        .cache()
        .args(["check", "--workspace", "--diag=json"]);
    let gitlab = driver(&fixture)
        .dir(&ws)
        .cache()
        .args(["check", "--workspace", "--diag=gitlab"]);

    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("`other`: no module, interface or program is called `gone`"),
        "{said}"
    );
    assert!(said.contains("missing"), "{said}");
    assert!(said.contains("2 of 3 packages failed"), "{said}");
    // Every package's records in one stream, a package's failure among
    // them, and no tally of the packages.
    assert!(!json.status.success());
    let said = stderr(&json);
    assert!(
        said.lines()
            .all(|line| line.starts_with(r#"{"severity":""#) && line.ends_with('}')),
        "{said}"
    );
    assert!(
        said.contains(r#""message":"`other`: no module, interface or program is called"#),
        "{said}"
    );
    assert!(said.contains(r#""code":"unknown-port""#), "{said}");
    assert!(!said.contains("packages failed"), "{said}");
    // One report for the workspace, written once every package has run.
    let said = stderr(&gitlab);
    assert!(
        said.starts_with('[') && said.trim_end().ends_with(']') && !said.contains("]["),
        "{said}"
    );
    assert_eq!(
        said.matches(r#""check_name":"unknown-port""#).count(),
        1,
        "{said}"
    );
}

#[test]
fn a_virtual_root_holds_no_files_and_stands_for_its_members() {
    let fixture = Fixture::new("workspace-virtual");
    let ws = workspace(&fixture);
    // Neither is read: the dependency would not resolve, and the indent
    // would fail `dma.sv`.
    fixture.file(
        "ws/astli.toml",
        "[workspace]\n\n[dependencies]\ngone = { path = 'gone' }\n\n[fmt]\nindent = 4\n\n\
         [tool.ci]\nnightly = true\n",
    );
    fixture.file("ws/soc.sv", "module   soc;\nendmodule\n");
    fixture.file("ws/ip/other/bad.sv", "module   bad;\nendmodule\n");
    let at_root = |args: &[&str]| driver(&fixture).dir(&ws).cache().args(args);

    std::fs::create_dir_all(ws.join("docs")).expect("a directory");
    let tree = driver(&fixture).dir(ws.join("docs")).cache().args(["tree"]);
    let fmt = at_root(&["fmt", "--check"]);
    let files = at_root(&["files"]);
    let dma = listed_in(&fixture, &ws, &["-p", "dma"]);
    let named = at_root(&["fmt", "--check", "ip/dma/dma.sv"]);
    fixture.file(
        "ws/ip/dma/tb_dma.sv",
        "(* astli_test *)\nmodule tb_dma;\nendmodule\n",
    );
    let tests = at_root(&["test", "--list"]);

    assert!(tree.status.success(), "{}", stderr(&tree));
    assert_eq!(stdout(&tree), "dma\n└── cells 1.0.0\n\nother\n");
    assert!(
        stderr(&tree).contains(
            "a virtual workspace root is no design, so astli does not read its \
             `[dependencies]`, `[fmt]`"
        ),
        "{}",
        stderr(&tree)
    );
    assert!(!stderr(&tree).contains("[tool]"), "{}", stderr(&tree));
    assert!(!fmt.status.success());
    assert!(
        slashed(&fmt).ends_with("ip/other/bad.sv\n"),
        "{}",
        stdout(&fmt)
    );
    assert!(!slashed(&fmt).contains("soc.sv"), "{}", stdout(&fmt));
    assert!(
        stderr(&fmt).contains("1 of 2 packages failed"),
        "{}",
        stderr(&fmt)
    );
    assert!(!files.status.success());
    let said = stderr(&files);
    assert!(
        said.contains("the workspace root here is virtual"),
        "{said}"
    );
    assert!(said.contains("`-p NAME`"), "{said}");
    assert_eq!(dma, ["cells.sv", "dma.sv"]);
    assert!(named.status.success(), "{}", stderr(&named));
    // Every member's tests, as under `--workspace`.
    assert!(tests.status.success(), "{}", stderr(&tests));
    assert_eq!(stdout(&tests), "dma tb_dma\n");
}

#[test]
fn bender_export_workspace_writes_each_package_bender_does_not_manage() {
    let fixture = Fixture::new("workspace-bender");
    let ws = workspace(&fixture);
    // Still bender's: its dependencies come from its `Bender.yml`.
    fixture.file("ws/ip/other/astli.toml", "[package]\nname = 'other'\n");
    fixture.file("ws/ip/other/Bender.yml", "package:\n  name: other\n");
    let at_root = |args: &[&str]| driver(&fixture).dir(&ws).cache().args(args);

    let exported = at_root(&["bender", "export", "--workspace"]);
    let checked = at_root(&["bender", "export", "--workspace", "--check"]);
    fixture.file("ws/ip/dma/Bender.yml", "package:\n  name: dma\n");
    let stale = at_root(&["bender", "export", "--workspace", "--check"]);

    assert!(exported.status.success(), "{}", stderr(&exported));
    assert!(ws.join("Bender.yml").is_file());
    assert!(ws.join("ip/dma/Bender.yml").is_file());
    assert_eq!(
        std::fs::read_to_string(ws.join("ip/other/Bender.yml")).expect("its own"),
        "package:\n  name: other\n"
    );
    assert!(checked.status.success(), "{}", stderr(&checked));
    assert!(!stale.status.success());
    let said = stderr(&stale);
    assert!(
        said.contains("`dma`: Bender.yml is not what astli.toml exports"),
        "{said}"
    );
    assert!(said.contains("1 of 3 packages failed"), "{said}");
}

#[test]
fn p_chooses_the_manifest_add_remove_and_bender_export_write() {
    let fixture = Fixture::new("workspace-p-manifest");
    let ws = workspace(&fixture);
    let at_root = |args: &[&str]| driver(&fixture).dir(&ws).cache().args(args);

    let added = at_root(&["add", "-p", "other", "--path", "ip/dma"]);
    let removed = at_root(&["remove", "-p", "dma", "cells"]);
    let exported = at_root(&["bender", "export", "-p", "other"]);

    assert!(added.status.success(), "{}", stderr(&added));
    let other = std::fs::read_to_string(ws.join("ip/other/astli.toml")).expect("a manifest");
    assert!(other.contains("dma = { path = \"../dma\" }"), "{other}");
    assert!(removed.status.success(), "{}", stderr(&removed));
    let dma = std::fs::read_to_string(ws.join("ip/dma/astli.toml")).expect("a manifest");
    assert!(!dma.contains("cells"), "{dma}");
    assert!(exported.status.success(), "{}", stderr(&exported));
    assert!(ws.join("ip/other/Bender.yml").is_file());
    assert!(!ws.join("Bender.yml").exists());
}
