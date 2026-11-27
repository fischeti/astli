//! `lint`, and the `astli.toml` that configures it.

use crate::support::*;

const BLOCKING_FLOP: &str = "\
module flop (input logic clk_i, input logic d_i, output logic q_o);
  always_ff @(posedge clk_i) q_o = d_i;
endmodule
";

#[test]
fn lint_passes_a_clean_file_without_a_word() {
    let fixture = Fixture::new("lint-clean");
    let file = fixture.file("tiny.sv", TINY);

    let output = driver(&fixture).args(["lint".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).is_empty() && stderr(&output).is_empty());
}

#[test]
fn lint_fails_on_a_denied_rule_and_names_it() {
    let fixture = Fixture::new("lint-deny");
    let file = fixture.file("flop.sv", BLOCKING_FLOP);

    let output = driver(&fixture).args(["lint".as_ref(), file.as_os_str()]);

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

    let warned = driver(&fixture).args(
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
    let allowed = driver(&fixture).args(
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
    let fixture = Fixture::new("lint-refuses-a-name-that-is-no-rule");
    let output = driver(&fixture).args(["lint", "--allow", "no-such-rule", "-"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no-such-rule"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn lint_list_names_every_rule_with_its_group_and_options() {
    let fixture = Fixture::new("lint-list-names-every-rule-with-its-group-and-options");
    let output = driver(&fixture).args(["lint", "--list"]);

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
    // A rule's options follow it, each set to its default as `astli.toml`
    // writes it.
    let options: Vec<&str> = (listed.lines())
        .skip_while(|line| !line.starts_with("port-name-suffix "))
        .skip(1)
        .take_while(|line| line.starts_with(' '))
        .map(|line| line.trim().split("  ").next().unwrap())
        .collect();
    assert_eq!(
        options,
        [
            r#"input = ["*_i", "*_ni", "*_pi"]"#,
            r#"output = ["*_o", "*_no", "*_po"]"#,
            r#"inout = ["*_io", "*_nio", "*_pio"]"#,
        ],
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

    let built = driver(&fixture).args(["lint", "top.sv", "--top", "top"]);
    let alone = driver(&fixture).args(["lint", "top.sv"]);
    let missing = driver(&fixture).args(["lint", "top.sv", "--top", "tpo"]);

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
    fixture.file(
        "tb/tb_a.sv",
        "module tb_a;\n  logic unread;\n  always @* ;\nendmodule\n",
    );
    fixture.file(
        "tb/tb_b.sv",
        "module tb_b;\n  logic unread = 1'b0;\nendmodule\n",
    );

    let output = driver(&fixture).args(["lint", "--diag", "short"]);

    let said = stderr(&output);
    // A file's findings are one list in line order, whichever rules read
    // the design and whichever the file as written.
    let design = said.find("tb_a.sv:2:9: warning[unused-signal]");
    let tree = said.find("tb_a.sv:3:3: warning[always-comb]");
    assert!(design.is_some() && design < tree, "{said}");
    // No top builds `tb_b`, so the design rules leave it alone, but how it
    // is written is still read.
    assert!(
        !said.contains("tb_b.sv:2:9: warning[unused-signal]")
            && said.contains("tb_b.sv:2:9: warning[variable-initializer]"),
        "{said}"
    );
}

#[test]
fn astli_toml_sets_levels_found_from_a_directory_below_it() {
    let fixture = Fixture::new("lint-config");
    fixture.file("astli.toml", "[lint]\nwarn = [\"correctness\"]\n");
    fixture.file("rtl/flop.sv", BLOCKING_FLOP);

    let output = driver(&fixture)
        .dir(fixture.path().join("rtl"))
        .args(["lint", "flop.sv"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("Warning"), "{}", stderr(&output));
}

#[test]
fn a_flag_wins_over_a_path_in_astli_toml() {
    let fixture = Fixture::new("lint-config-paths");
    let config = "[lint.paths]\n\"vendor/**\" = { allow = [\"always-ff-non-blocking\"] }\n\
                  \"rtl/**\" = { deny = [\"always-ff-non-blocking\"] }\n";
    fixture.file("astli.toml", config);
    fixture.file("vendor/ip/flop.sv", BLOCKING_FLOP);
    fixture.file("rtl/flop.sv", BLOCKING_FLOP);

    let vendored = driver(&fixture).args(["lint", "vendor/ip/flop.sv"]);
    let denied = driver(&fixture).args([
        "lint",
        "--deny",
        "always-ff-non-blocking",
        "vendor/ip/flop.sv",
    ]);
    let own = driver(&fixture).args(["lint", "rtl/flop.sv"]);
    let allowed =
        driver(&fixture).args(["lint", "--allow", "always-ff-non-blocking", "rtl/flop.sv"]);

    assert!(vendored.status.success(), "{}", stderr(&vendored));
    assert!(stderr(&vendored).is_empty(), "{}", stderr(&vendored));
    assert!(!denied.status.success());
    assert!(!own.status.success());
    assert!(allowed.status.success(), "{}", stderr(&allowed));
    assert!(stderr(&allowed).is_empty(), "{}", stderr(&allowed));
}

#[test]
fn a_config_named_on_the_command_line_is_the_one_read() {
    let fixture = Fixture::new("lint-config-explicit");
    fixture.file("astli.toml", "[lint]\ndeny = [\"correctness\"]\n");
    fixture.file("lax.toml", "[lint]\nallow = [\"correctness\"]\n");
    fixture.file("flop.sv", BLOCKING_FLOP);

    let output = driver(&fixture).args(["lint", "--config", "lax.toml", "flop.sv"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

/// A module whose ports say their direction first, as lowRISC's style does
/// not.
const PREFIXED_PORTS: &str =
    "module top (\n  input  logic i_a,\n  output logic o_b\n);\n  assign o_b = i_a;\nendmodule\n";

/// What says ports are named by a prefix.
const PREFIX_STYLE: &str = "[lint.rules.port-name-suffix]\ninput = [\"i_*\"]\noutput = [\"o_*\"]\n";

#[test]
fn astli_toml_sets_a_rules_options() {
    let fixture = Fixture::new("lint-config-options");
    fixture.file("top.sv", PREFIXED_PORTS);

    let lowrisc = driver(&fixture).args(["lint", "top.sv"]);
    fixture.file("astli.toml", PREFIX_STYLE);
    let prefixed = driver(&fixture).args(["lint", "top.sv"]);

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
fn astli_toml_sets_a_rules_flag() {
    let fixture = Fixture::new("lint-config-flag");
    fixture.file(
        "top.sv",
        "module top (\n  input  logic       clk_i,\n  input  logic       rst_ni,\n  input  logic       clear_i,\n  input  logic [7:0] d_i\n);\n  logic [7:0] q;\n  always_ff @(posedge clk_i or negedge rst_ni) begin\n    if (!rst_ni) q <= '0;\n    else if (clear_i) q <= '0;\n    else q <= d_i;\n  end\nendmodule\n",
    );
    let args = ["lint", "--deny", "always-ff-registers-only", "top.sv"];

    let cleared = driver(&fixture).args(args);
    let rule = "[lint.rules.always-ff-registers-only]\nclear = false\n";
    fixture.file("astli.toml", rule);
    let forbidden = driver(&fixture).args(args);

    assert!(
        !stderr(&cleared).contains("always-ff-registers-only"),
        "{}",
        stderr(&cleared)
    );
    assert!(
        stderr(&forbidden).contains("clear in `always_ff`"),
        "{}",
        stderr(&forbidden)
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
        let output = driver(&fixture)
            .dir(&root)
            .cache()
            .args(["lint"].iter().chain(args));
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

    let output = driver(&fixture).args(["lint", "top.sv"]);

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
    // A virtual root reads no dev-dependencies, so a name is no style.
    std::fs::create_dir_all(fixture.path().join("virtual/.git")).expect("a repository");
    fixture.file(
        "virtual/astli.toml",
        "[workspace]\n\n[lint]\nextends = \"style\"\n",
    );
    fixture.file("virtual/top.sv", PREFIXED_PORTS);

    let dependency = driver(&fixture)
        .dir(fixture.path().join("root"))
        .cache()
        .args(["lint", "top.sv"]);
    let paths = driver(&fixture)
        .dir(fixture.path().join("other"))
        .args(["lint", "top.sv"]);
    let in_virtual = driver(&fixture)
        .dir(fixture.path().join("virtual"))
        .args(["lint", "top.sv"]);

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
    assert!(!in_virtual.status.success());
    assert!(
        stderr(&in_virtual).contains("a virtual workspace root has no dev-dependencies"),
        "{}",
        stderr(&in_virtual)
    );
}

#[test]
fn lint_fix_rewrites_what_it_can_and_reports_the_rest() {
    let fixture = Fixture::new("lint-fix");
    let source = "module top (\n  input logic a_i\n);\n  reg q;\n  always_ff @(posedge a_i) q = 1'b0;\nendmodule\n";
    fixture.file("top.sv", source);

    let output = driver(&fixture).args(["lint", "--fix", "top.sv"]);

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

    let output = driver(&fixture).args(["lint", "top.sv"]);
    let broken = driver(&fixture).args(["lint", "broken.sv"]);

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

    let output = driver(&fixture).args(["lint", "--fix", "top.sv"]);

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

    let allowed = driver(&fixture).args(["lint", "--fix", "--allow", "forbid-reg", "top.sv"]);
    let unchanged = std::fs::read_to_string(fixture.path().join("top.sv")).unwrap();
    let waived = driver(&fixture).args(["lint", "--fix", "top.sv"]);
    let written = std::fs::read_to_string(fixture.path().join("top.sv")).unwrap();

    assert!(allowed.status.success() && waived.status.success());
    assert_eq!(unchanged, source);
    assert_eq!(written, source.replace("reg q", "logic q"));
}

#[test]
fn an_astli_toml_a_command_cannot_read_is_refused_with_its_path() {
    let lint = "lint";
    let fmt = "fmt";
    for (run, config, named) in [
        (
            lint,
            "[lint.paths]\n\"*.sv\" = { allow = [\"no-such-rule\"] }\n",
            "no-such-rule",
        ),
        (
            lint,
            "[lint.rules.port-name-suffix]\nin = [\"i_*\"]\n",
            "has no option `in`",
        ),
        (
            lint,
            "[lint.rules.unused-signal]\nignore = \"*_nc\"\n",
            "a list of strings, or `true` or `false`",
        ),
        (
            lint,
            "[lint.rules.unused-parameter]\nignore = [1]\n",
            "a list of strings, or `true` or `false`",
        ),
        (
            lint,
            "[lint.rules.always-ff-registers-only]\nclear = [\"no\"]\n",
            "`clear`: is `true` or `false`",
        ),
        (fmt, "[fmt]\ntabs = true\n", "tabs"),
        (fmt, "[fmt]\nexclude = [\"rtl/[ab\"]\n", "rtl/[ab"),
    ] {
        let fixture = Fixture::new("config-refused");
        fixture.file("astli.toml", config);
        fixture.file("tiny.sv", TINY);

        let output = driver(&fixture).args([run, "tiny.sv"]);

        assert!(!output.status.success(), "{config}");
        let said = stderr(&output);
        assert!(
            said.contains("astli.toml") && said.contains(named),
            "{config}: {said}"
        );
    }
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
        driver(&fixture).args(all)
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
