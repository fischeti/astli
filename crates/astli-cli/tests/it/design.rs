//! A package read as a design: what is named when nothing is, its tops and
//! their tables, and its features.

use std::path::Path;
use std::process::Output;

use crate::support::*;

#[test]
fn check_reports_an_elaboration_error_only_where_a_top_builds_it() {
    let fixture = Fixture::new("check-elaboration-error");
    let file = fixture.file(
        "m.sv",
        "module m #(parameter int W = 3) ();\n\
         \x20 if (W != 4) begin : gen_err\n\
         \x20   $error(\"W must be 4, is %0d\", W);\n\
         \x20 end\n\
         endmodule\n",
    );
    let check = |top: &[&str]| {
        let mut args: Vec<&std::ffi::OsStr> =
            vec!["check".as_ref(), "--diag".as_ref(), "short".as_ref()];
        args.extend(top.iter().map(std::ffi::OsStr::new));
        args.push(file.as_os_str());
        driver(&fixture).args(args)
    };

    let output = check(&["--top", "m"]);
    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(
        said.contains("m.sv:3:5: error[elaboration-error]: W must be 4, is 3"),
        "{said}"
    );

    // Without a top, nothing says the defaults are what is built.
    let output = check(&[]);
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn files_with_nothing_named_reads_the_package() {
    let fixture = Fixture::new("discover-files");
    package(&fixture);

    let output = driver(&fixture).args(["files", "--top", "tb_core"]);

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
fn files_names_the_directory_of_a_header_beside_its_file() {
    let fixture = Fixture::new("files-include-beside");
    include_beside(&fixture);

    let output = driver(&fixture).args(["files", "--top", "tb", "--emit", "incdirs"]);

    assert!(output.status.success(), "{}", stderr(&output));
    // Found by suffix within the package, since no include directory holds
    // it, and named, since a compiler searches only those.
    let listed = stdout(&output);
    let dir = listed
        .strip_prefix("+incdir+")
        .and_then(|rest| rest.strip_suffix('\n'));
    assert!(
        dir.is_some_and(|dir| Path::new(dir).ends_with("tb")),
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

    let output = driver(&fixture).args(["files", "--emit", "files"]);

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

    let output = driver(&fixture).args(["check", "--diag", "short"]);
    let full = driver(&fixture).args(["check"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("tb_broken.sv:2:3: error[unknown-definition]"),
        "{}",
        stderr(&output)
    );
    // A cell a library supplies has a stub to declare it by.
    assert!(
        stderr(&full).contains("declare its ports in a file with `extern module gone (...);`"),
        "{}",
        stderr(&full)
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

    let core = driver(&fixture).args(["check", "--top", "tb_core"]);
    let broken = driver(&fixture).args(["check", "--top", "tb_broken", "--diag", "short"]);
    let all = driver(&fixture).args(["check", "--diag", "short"]);

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
        stderr(&all).ends_with("error: 1 of 3 files has errors\n"),
        "{}",
        stderr(&all)
    );
}

#[test]
fn check_with_a_top_refuses_what_files_refuses_in_a_dependency() {
    let fixture = Fixture::new("check-top-redeclared");
    fixture.file("root/top.sv", "module top;\n  ip u_ip ();\nendmodule\n");
    fixture.file("ip/ip.sv", "module ip;\n  ff u_ff ();\nendmodule\n");
    fixture.file("ip/ff.sv", "module ff;\nendmodule\n");
    fixture.file("ip/ff_latch.sv", "module ff;\nendmodule\n");
    let url = repository(&fixture, "ip");
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\nip = {{ git = '{url}', version = '1' }}\n"),
    );
    let root = fixture.path().join("root");

    let files = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--top", "top"]);
    let check = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["check", "--top", "top"]);
    let without = driver(&fixture).dir(&root).cache().args(["check"]);

    for output in [&files, &check] {
        assert!(!output.status.success());
        assert!(
            stderr(output).contains("`ff` is declared 2 times and used"),
            "{}",
            stderr(output)
        );
    }
    // Without a top, a fetched dependency's problems are its authors' to
    // fix.
    assert!(without.status.success(), "{}", stderr(&without));
}

#[test]
fn check_with_a_top_refuses_an_include_a_dependency_cannot_resolve() {
    let fixture = Fixture::new("check-top-dependency-include");
    fixture.file("root/top.sv", "module top;\n  leaf u_leaf ();\nendmodule\n");
    fixture.file(
        "ip/src/leaf.sv",
        "`include \"defs.svh\"\nmodule leaf;\nendmodule\n",
    );
    // No include directory holds either, and both end in the name.
    fixture.file("ip/inc/defs.svh", "");
    fixture.file("ip/other/defs.svh", "");
    let url = repository(&fixture, "ip");
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\nip = {{ git = '{url}', version = '1' }}\n"),
    );
    let root = fixture.path().join("root");

    let files = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--top", "top"]);
    let check = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["check", "--top", "top"]);
    let without = driver(&fixture).dir(&root).cache().args(["check"]);

    // The list would lack the header, so neither passes.
    for output in [&files, &check] {
        assert!(!output.status.success());
        let said = stderr(output);
        assert!(
            said.contains("`defs.svh` could be any of 2 files"),
            "{said}"
        );
        assert!(said.contains("[described.ip] include-dirs"), "{said}");
    }
    assert!(stdout(&files).is_empty(), "{}", stdout(&files));
    // Without a top, a fetched dependency's problems are its authors' to
    // fix.
    assert!(without.status.success(), "{}", stderr(&without));
}

#[test]
fn check_reports_a_dependency_on_disk_as_the_roots_own() {
    let fixture = Fixture::new("check-path-dependency");
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nuart = { path = '../uart' }\n",
    );
    fixture.file(
        "root/top.sv",
        "module top;\n  uart #(.AddrWidth(48)) u_uart ();\nendmodule\n",
    );
    fixture.file(
        "uart/uart.sv",
        "module uart #(parameter int AddrWidth = 32) ();\n\
         \x20 if (AddrWidth > 32) begin : gen_err\n\
         \x20   $error(\"AddrWidth over 32\");\n\
         \x20 end\n\
         endmodule\n",
    );
    fixture.file(
        "uart/uart_unused.sv",
        "module uart_unused;\n  gone u ();\nendmodule\n",
    );
    let root = fixture.path().join("root");
    let check = |args: &[&str]| {
        let args: Vec<&str> = ["check", "--diag", "short"]
            .iter()
            .chain(args)
            .copied()
            .collect();
        driver(&fixture).dir(&root).cache().args(&args)
    };

    let top = check(&["--top", "top"]);
    let without = check(&[]);
    let lint = driver(&fixture).dir(&root).cache().args(["lint"]);

    // Only a dependent builds `uart` with the value it fails under, so its
    // run is the one to tell, and a file no top reaches is not told.
    assert!(!top.status.success());
    let said = stderr(&top);
    assert!(
        said.contains("uart.sv:3:5: error[elaboration-error]: AddrWidth over 32"),
        "{said}"
    );
    assert!(!said.contains("uart_unused.sv"), "{said}");
    assert!(!without.status.success());
    let said = stderr(&without);
    assert!(
        said.contains("uart_unused.sv:2:3: error[unknown-definition]"),
        "{said}"
    );
    assert!(lint.status.success(), "{}", stderr(&lint));
}

#[test]
fn files_notes_a_bind_from_a_dependency_without_a_manifest() {
    let fixture = Fixture::new("files-dependency-bind");
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nip = { path = '../ip' }\n",
    );
    fixture.file("root/top.sv", "module top;\n  leaf u_leaf ();\nendmodule\n");
    fixture.file("ip/src/leaf.sv", "module leaf;\nendmodule\n");
    fixture.file(
        "ip/formal/props.sv",
        "module props;\nendmodule\nbind leaf props u_props ();\n",
    );
    let root = fixture.path().join("root");

    let files = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--top", "top"]);
    let check = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["check", "--top", "top"]);

    for output in [&files, &check] {
        assert!(output.status.success(), "{}", stderr(output));
        let said = stderr(output).replace('\\', "/");
        assert!(
            said.contains("formal/props.sv` binds into `leaf`, and `ip` has no astli.toml")
                && said.contains("[described.ip] exclude"),
            "{said}"
        );
    }
    // Still listed: the note says why, and the root decides.
    assert!(stdout(&files).contains("props.sv"), "{}", stdout(&files));

    // Described, the root has decided, and nothing more is said.
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nip = { path = '../ip' }\n\n[described.ip]\nexclude = ['formal/**']\n",
    );
    let described = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--top", "top"]);
    assert!(described.status.success(), "{}", stderr(&described));
    assert!(
        !stderr(&described).contains("binds into"),
        "{}",
        stderr(&described)
    );
    assert!(
        !stdout(&described).contains("props.sv"),
        "{}",
        stdout(&described)
    );
}

#[test]
fn files_warns_where_the_list_finds_another_header_than_check() {
    let fixture = Fixture::new("files-include-differs");
    fixture.file(
        "root/astli.toml",
        "[package]\ninclude-dirs = ['include']\n\n[dependencies]\nip = { path = '../ip' }\n",
    );
    fixture.file(
        "root/top.sv",
        "`include \"defs.svh\"\nmodule top;\n  leaf u_leaf ();\n  macro u_macro ();\nendmodule\n",
    );
    fixture.file("root/include/defs.svh", "`define W 8\n");
    // Listed after what it instantiates, though walked before it.
    fixture.file(
        "ip/src/leaf.sv",
        "`include \"defs.svh\"\nmodule leaf;\n  sub u_sub ();\nendmodule\n",
    );
    fixture.file(
        "ip/src/sub.sv",
        "`include \"defs.svh\"\nmodule sub;\nendmodule\n",
    );
    // A stub the list leaves out, whose header a tool is never asked for.
    fixture.file(
        "ip/src/macro.sv",
        "`include \"defs.svh\"\nextern module macro;\n",
    );
    fixture.file("ip/include/defs.svh", "`define W 4\n");
    let root = fixture.path().join("root");

    let files = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--top", "top", "--diag", "short"]);
    let check = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["check", "--top", "top", "--diag", "short"]);
    let whole = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["check", "--diag", "short"]);

    // Each package found its own, but the list searches the root's first
    // for both, and the check of the tops warns of the list `files` writes,
    // in its order.
    let differs = |output: &Output| -> Vec<String> {
        assert!(output.status.success(), "{}", stderr(output));
        (stderr(output).replace('\\', "/").lines())
            .filter(|line| line.contains("[include-differs]"))
            .map(str::to_string)
            .collect()
    };
    let listed = differs(&files);
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert!(
        listed[0].contains("sub.sv:1:1: warning[include-differs]: a tool given the list reads")
            && listed[0].contains("root/include/defs.svh` for `defs.svh`")
            && listed[1].contains("leaf.sv:1:1: warning[include-differs]"),
        "{listed:?}"
    );
    assert_eq!(differs(&check), listed);
    assert_eq!(differs(&whole), listed);
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
    needs_a_define(&fixture, "[top.tb_chip]\ndefines = { MY_FEATURE = true }\n");

    let check = driver(&fixture).args(["check", "--top", "tb_chip"]);
    let files = driver(&fixture).args(["files", "--top", "tb_chip"]);

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
        "[top.chip]\ntop = 'tb_chip'\ndefines = { MY_FEATURE = true }\n",
    );

    let output = driver(&fixture).args(["check", "--top", "chip"]);

    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn a_top_table_named_as_another_module_is_refused() {
    let fixture = Fixture::new("top-table-ambiguous");
    needs_a_define(
        &fixture,
        "[top.chip]\ntop = 'tb_chip'\ndefines = { MY_FEATURE = true }\n",
    );
    fixture.file("rtl/chip.sv", "module chip;\nendmodule\n");

    let output = driver(&fixture).args(["check", "--top", "chip"]);

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
    needs_a_define(&fixture, "[top.tb_chip]\ndefines = { MY_FEATURE = true }\n");

    let check = driver(&fixture).args(["check"]);
    let files = driver(&fixture).args(["files"]);

    assert!(check.status.success(), "{}", stderr(&check));
    assert!(
        stdout(&files).starts_with("+define+MY_FEATURE\n"),
        "{}",
        stdout(&files)
    );
}

#[test]
fn without_a_top_check_and_lint_cover_every_table_and_the_rest_need_one() {
    let fixture = Fixture::new("top-table-several");
    // Two sets of defines, which define `W` two ways, and a file both reach.
    let tables = "[top.tb_chip]\ndefines = { MY_FEATURE = true, W = 8 }\n\n\
                  [top.tb_other]\ndefines = { OTHER = true, W = 16 }\n";
    fixture.file("astli.toml", tables);
    fixture.file(
        "tb/tb_chip.sv",
        "module tb_chip;\n`ifdef MY_FEATURE\n  chip_only u_chip ();\n`endif\n  shared u_shared ();\nendmodule\n",
    );
    fixture.file(
        "tb/tb_other.sv",
        "module tb_other;\n`ifdef OTHER\n  other_only u_other ();\n`endif\n  shared u_shared ();\nendmodule\n",
    );
    fixture.file(
        "rtl/shared.sv",
        "`ifndef W\n`define W 4\n`endif\nmodule shared;\n  logic [`W-1:0] unread;\n  missing u_missing ();\nendmodule\n",
    );

    let check = driver(&fixture).args(["check", "--diag", "short"]);
    let lint = driver(&fixture).args(["lint", "--diag", "short"]);
    let files = driver(&fixture).args(["files"]);
    let build = driver(&fixture).args(["build"]);
    let candidates = driver(&fixture).args(["files", "--emit", "tops"]);
    fixture.file(
        "astli.toml",
        &format!("[package]\ndefault-top = 'tb_chip'\n\n{tables}"),
    );
    let defaulted = driver(&fixture).args(["files"]);
    let defaulted_check = driver(&fixture).args(["check", "--diag", "short"]);

    // Each table under its own defines, and what both find told once.
    assert!(!check.status.success());
    let said = stderr(&check);
    assert!(said.contains("`chip_only`"), "{said}");
    assert!(said.contains("`other_only`"), "{said}");
    assert_eq!(said.matches("`missing`").count(), 1, "{said}");
    assert!(said.contains("3 of 3 files have errors"), "{said}");
    let said = stderr(&lint);
    assert_eq!(
        said.matches("shared.sv:5:18: warning[unused-signal]")
            .count(),
        1,
        "{said}"
    );
    // What describes one design still asks which.
    for output in [&files, &build] {
        assert!(!output.status.success());
        assert!(
            stderr(output).contains("no top named, and the manifest has 2: `tb_chip`, `tb_other`"),
            "{}",
            stderr(output)
        );
    }
    // Listing the candidates builds from none of them.
    assert!(candidates.status.success(), "{}", stderr(&candidates));
    assert!(defaulted.status.success(), "{}", stderr(&defaulted));
    assert!(
        stdout(&defaulted).starts_with("+define+MY_FEATURE\n+define+W=8\n"),
        "{}",
        stdout(&defaulted)
    );
    // `default-top` is for one design, and `check` still covers both.
    assert!(
        stderr(&defaulted_check).contains("`other_only`"),
        "{}",
        stderr(&defaulted_check)
    );
}

#[test]
fn tops_defining_a_macro_two_ways_are_refused() {
    let fixture = Fixture::new("top-table-clash");
    needs_a_define(
        &fixture,
        "[top.tb_chip]\ndefines = { MY_FEATURE = true, W = 8 }\n\n[top.tb_other]\ndefines = { W = 16 }\n",
    );
    fixture.file("tb/tb_other.sv", "module tb_other;\nendmodule\n");

    let output = driver(&fixture).args(["check", "--top", "tb_chip", "--top", "tb_other"]);

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
        "[package]\nname = 'ip'\ndefault-top = 'tb_ip'\n\n[top.tb_ip]\ndefines = { IP_BENCH = true }\n",
    );
    fixture.file("ip/ip.sv", "module ip;\nendmodule\n");
    let root = fixture.path().join("root");

    let output = driver(&fixture).dir(&root).cache().args(["files"]);

    // The root has no table, so it builds every file, and defines nothing.
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stdout(&output).contains("+define+"), "{}", stdout(&output));
}

#[test]
fn files_lists_each_package_together_after_its_dependencies() {
    let fixture = Fixture::new("files-package-groups");
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nip = { path = '../ip' }\n",
    );
    fixture.file(
        "root/a_top.sv",
        "module a_top;\n  b_core u_core ();\n  ip u_ip ();\nendmodule\n",
    );
    fixture.file("root/b_core.sv", "module b_core;\nendmodule\n");
    fixture.file(
        "ip/astli.toml",
        "[dependencies]\ncells = { path = '../cells' }\n",
    );
    fixture.file("ip/ip.sv", "module ip;\n  cells u_cells ();\nendmodule\n");
    fixture.file("ip/ip_spare.sv", "module ip_spare;\nendmodule\n");
    fixture.file("cells/cells.sv", "module cells;\nendmodule\n");
    let root = fixture.path().join("root");
    let files = |args: &[&str]| -> Vec<String> {
        let output = driver(&fixture)
            .dir(&root)
            .cache()
            .args([&["files", "--emit", "files"], args].concat());
        assert!(output.status.success(), "{}", stderr(&output));
        let listed = stdout(&output);
        let name = |line: &str| Path::new(line).file_name().unwrap_or_default().to_owned();
        (listed.lines().map(name))
            .map(|name| name.to_string_lossy().into_owned())
            .collect()
    };

    // A file the root does not use still comes before the root's, and the
    // root's own `b_core` does not split the dependencies.
    assert_eq!(
        files(&[]),
        ["cells.sv", "ip.sv", "ip_spare.sv", "b_core.sv", "a_top.sv"]
    );
    assert_eq!(
        files(&["--top", "a_top"]),
        ["cells.sv", "ip.sv", "b_core.sv", "a_top.sv"]
    );
}

#[test]
fn features_select_the_packages_files() {
    let fixture = Fixture::new("discover-features");
    package(&fixture);
    fixture.file("astli.toml", "[features]\nvip = { sources = [\"vip\"] }\n");
    fixture.file("vip/core_vip.sv", "module core_vip;\nendmodule\n");
    let files = |args: &[&str]| -> Vec<String> {
        let output = driver(&fixture).args([&["files", "--emit", "files"], args].concat());
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

    let unknown = driver(&fixture).args(["files", "--features", "nope"]);
    let named = driver(&fixture).args(["files", "--no-features", "rtl/core.sv"]);
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

    let output = driver(&fixture).args(["check"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no package here"),
        "{}",
        stderr(&output)
    );
}
