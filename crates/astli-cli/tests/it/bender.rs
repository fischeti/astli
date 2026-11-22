//! Importing from Bender and `bender export`.

use crate::support::*;

#[test]
fn init_leaves_the_dependencies_of_a_bender_yml_to_import() {
    let fixture = Fixture::new("bender-import");
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

    let left = driver(&fixture).args(["init"]);
    let named = std::fs::read_to_string(&manifest).expect("a manifest");
    let copied = driver(&fixture).args(["bender", "import"]);
    let into = std::fs::read_to_string(&manifest).expect("a manifest");
    let again = driver(&fixture).args(["bender", "import"]);
    std::fs::remove_file(&manifest).expect("the manifest removed");
    let created = driver(&fixture).args(["bender", "import"]);

    assert!(left.status.success(), "{}", stderr(&left));
    assert!(
        stderr(&left).contains("still read from Bender.yml"),
        "{}",
        stderr(&left)
    );
    assert_eq!(named, "[package]\nname = \"soc\"\n");
    assert!(copied.status.success(), "{}", stderr(&copied));
    assert_eq!(
        stderr(&copied),
        "      Copied the 3 dependencies of Bender.yml into astli.toml\n"
    );
    let imported = "[package]\nname = \"soc\"\n\n[dependencies]\n\
         axi = { git = \"https://example.com/axi.git\", version = \"0.39.8\" }\n\
         cva6 = { git = \"https://example.com/cva6.git\", rev = \"pulp-v2.0.0\" }\n\
         ip = { path = \"../ip\" }\n";
    assert_eq!(into, imported);
    assert!(!again.status.success());
    assert!(
        stderr(&again).contains("already lists its dependencies"),
        "{}",
        stderr(&again)
    );
    assert!(created.status.success(), "{}", stderr(&created));
    assert_eq!(
        stderr(&created),
        "     Created astli.toml for `soc`, with the 3 dependencies of Bender.yml\n"
    );
    assert_eq!(
        std::fs::read_to_string(&manifest).expect("a manifest"),
        imported
    );
}

/// A package with a default feature, one bringing in a dependency and
/// enabling a feature of another, and testbenches, beside the two packages
/// it depends on.
fn exported(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.file(
        "soc/astli.toml",
        "[package]\nname = \"soc\"\ndev = [\"tb\"]\n\n\
         [features]\ndefault = [\"fast\"]\n\
         fast = { sources = [\"src/fast.sv\"] }\n\
         vip = { sources = [\"vip\"], include-dirs = [\"vip/include\"], \
         dependencies = [\"model\", \"cells/sim\"] }\n\n\
         [dependencies]\n\
         cells = { path = \"../cells\", features = [\"x\"] }\n\
         model = { path = \"../model\" }\n",
    );
    fixture.file("soc/src/soc_pkg.sv", "package soc_pkg;\nendpackage\n");
    fixture.file(
        "soc/src/a_soc.sv",
        "module a_soc import soc_pkg::*; ();\n  gate u_gate ();\nendmodule\n",
    );
    fixture.file("soc/src/fast.sv", "module fast;\nendmodule\n");
    fixture.file("soc/vip/include/vip.svh", "`define VIP\n");
    fixture.file(
        "soc/vip/drv.sv",
        "`include \"vip.svh\"\nmodule drv;\n  model u_model ();\nendmodule\n",
    );
    fixture.file(
        "soc/tb/tb.sv",
        "module tb;\n  a_soc u_soc ();\n  drv u_drv ();\nendmodule\n",
    );
    fixture.file(
        "cells/astli.toml",
        "[package]\nname = \"cells\"\n\n[features]\nx = {}\nsim = {}\n",
    );
    fixture.file("cells/cell.sv", "module gate;\nendmodule\n");
    fixture.file("model/model.sv", "module model;\nendmodule\n");
    fixture
}

#[test]
fn bender_export_writes_the_bender_yml_astli_toml_says() {
    let fixture = exported("bender-export");
    let soc = fixture.path().join("soc");

    let output = driver(&fixture).dir(&soc).args(["bender", "export"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).ends_with("    Exported Bender.yml for `soc`\n"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        std::fs::read_to_string(soc.join("Bender.yml")).expect("a Bender.yml"),
        "# Written by `astli bender export` from astli.toml: edit that instead.\n\
         package:\n  name: soc\n\n\
         dependencies:\n\
         \x20 cells: { path: \"../cells\", pass_targets: [\"x\", { target: \"vip\", pass: \"sim\" }] }\n\
         \x20 model: { path: \"../model\", target: \"vip\" }\n\n\
         export_include_dirs:\n\
         \x20 - { target: \"vip\", dir: vip/include }\n\n\
         sources:\n\
         \x20 - src/soc_pkg.sv\n\
         \x20 - src/a_soc.sv\n\
         \x20 - src/fast.sv\n\
         \x20 - target: vip\n\
         \x20   files:\n\
         \x20     - vip/drv.sv\n\
         \x20 - target: soc_dev\n\
         \x20   files:\n\
         \x20     - tb/tb.sv\n"
    );
}

#[test]
fn bender_export_builds_dev_features_under_the_dev_target() {
    let fixture = exported("bender-export-dev-features");
    let soc = fixture.path().join("soc");
    let manifest = std::fs::read_to_string(soc.join("astli.toml")).expect("a manifest");
    fixture.file(
        "soc/astli.toml",
        &manifest.replace(
            "dev = [\"tb\"]\n",
            "dev = [\"tb\"]\ndev-features = [\"vip\", \"cells/x\", \"model/fast\"]\n",
        ),
    );
    fixture.file(
        "model/astli.toml",
        "[package]\nname = \"model\"\n\n[features]\nfast = {}\n",
    );

    let output = driver(&fixture).dir(&soc).args(["bender", "export"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let written = std::fs::read_to_string(soc.join("Bender.yml")).expect("a Bender.yml");
    // An own feature builds in the package's own builds too, and a
    // dependency's is passed in them alone.
    for line in [
        "  cells: { path: \"../cells\", pass_targets: [\"x\", { target: \"any(vip, soc_dev)\", pass: \"sim\" }] }\n",
        "  model: { path: \"../model\", target: \"any(vip, soc_dev)\", pass_targets: [{ target: \"soc_dev\", pass: \"fast\" }] }\n",
        "  - target: \"any(vip, soc_dev)\"\n    files:\n      - vip/drv.sv\n",
    ] {
        assert!(written.contains(line), "{line}\nin\n{written}");
    }
}

#[test]
fn bender_export_lists_a_replacement_before_what_uses_it() {
    let fixture = exported("bender-export-override");
    let soc = fixture.path().join("soc");
    let manifest = std::fs::read_to_string(soc.join("astli.toml")).expect("a manifest");
    fixture.file(
        "soc/astli.toml",
        &format!("{manifest}\n[overrides]\ncells = [\"gate\"]\n"),
    );
    fixture.file("soc/src/cell.sv", "module gate;\nendmodule\n");

    let output = driver(&fixture).dir(&soc).args(["bender", "export"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let written = std::fs::read_to_string(soc.join("Bender.yml")).expect("a Bender.yml");
    assert!(
        written.ends_with(
            "sources:\n\
             \x20 - src/soc_pkg.sv\n\
             \x20 - src/fast.sv\n\
             \x20 - override_files: true\n\
             \x20   files:\n\
             \x20     - src/cell.sv\n\
             \x20 - src/a_soc.sv\n\
             \x20 - target: vip\n\
             \x20   files:\n\
             \x20     - vip/drv.sv\n\
             \x20 - target: soc_dev\n\
             \x20   files:\n\
             \x20     - tb/tb.sv\n"
        ),
        "{written}"
    );
}

#[test]
fn bender_export_check_fails_once_the_package_changes() {
    let fixture = exported("bender-export-check");
    let soc = fixture.path().join("soc");

    let missing = driver(&fixture)
        .dir(&soc)
        .args(["bender", "export", "--check"]);
    let unwritten = !soc.join("Bender.yml").exists();
    let written = driver(&fixture).dir(&soc).args(["bender", "export"]);
    let checked = driver(&fixture)
        .dir(&soc)
        .args(["bender", "export", "--check"]);
    fixture.file("soc/src/more.sv", "module more;\nendmodule\n");
    let stale = driver(&fixture)
        .dir(&soc)
        .args(["bender", "export", "--check"]);

    assert!(!missing.status.success());
    assert!(unwritten);
    assert!(written.status.success(), "{}", stderr(&written));
    assert!(checked.status.success(), "{}", stderr(&checked));
    assert!(!stale.status.success());
    assert!(
        stderr(&stale).contains("Bender.yml is not what astli.toml exports"),
        "{}",
        stderr(&stale)
    );
}

#[test]
fn bender_export_refuses_what_bender_would_build_differently() {
    let fixture = exported("bender-export-refused");
    let soc = fixture.path().join("soc");
    let manifest = std::fs::read_to_string(soc.join("astli.toml")).expect("a manifest");
    fixture.file(
        "cells/astli.toml",
        "[package]\nname = \"cells\"\n\n[features]\ndefault = [\"x\"]\nx = {}\nsim = {}\n",
    );
    fixture.file(
        "soc/astli.toml",
        &manifest.replace("features = [\"x\"]", "default-features = false"),
    );

    let defaults = driver(&fixture).dir(&soc).args(["bender", "export"]);
    fixture.file(
        "soc/astli.toml",
        &format!("{manifest}\n[overrides]\ncells = [\"gate\"]\n"),
    );
    fixture.file("soc/src/my_cell.sv", "module gate;\nendmodule\n");
    let renamed = driver(&fixture).dir(&soc).args(["bender", "export"]);
    std::fs::remove_file(soc.join("src/my_cell.sv")).expect("the replacement removed");
    fixture.file("soc/src/cell.sv", "module gate;\nendmodule\n");
    fixture.file("model/cell.sv", "module other;\nendmodule\n");
    let clashing = driver(&fixture).dir(&soc).args(["bender", "export"]);
    std::fs::remove_file(fixture.path().join("model/cell.sv")).expect("the clash removed");
    fixture.file("soc/src/soc.svh", "`define SOC\n");
    fixture.file(
        "soc/src/cell.sv",
        "`include \"soc.svh\"\nmodule gate;\nendmodule\n",
    );
    let including = driver(&fixture).dir(&soc).args(["bender", "export"]);

    assert!(!defaults.status.success());
    assert!(
        stderr(&defaults)
            .contains("turns off the default features of `cells`, which bender cannot"),
        "{}",
        stderr(&defaults)
    );
    assert!(!renamed.status.success());
    assert!(
        stderr(&renamed).contains("bender replaces a file only by one of the same name"),
        "{}",
        stderr(&renamed)
    );
    assert!(!clashing.status.success());
    assert!(
        stderr(&clashing)
            .replace('\\', "/")
            .contains("model/cell.sv` too"),
        "{}",
        stderr(&clashing)
    );
    assert!(!including.status.success());
    assert!(
        stderr(&including).contains("which bender would not find there"),
        "{}",
        stderr(&including)
    );
    assert!(!soc.join("Bender.yml").exists());
}

#[test]
fn bender_export_refuses_a_root_whose_bender_yml_lists_its_dependencies() {
    let fixture = Fixture::new("bender-export-unimported");
    fixture.file("astli.toml", "[package]\nname = \"soc\"\n");
    fixture.file("Bender.yml", "package:\n  name: soc\n");
    let before = std::fs::read_to_string(fixture.path().join("Bender.yml")).expect("a Bender.yml");

    let output = driver(&fixture).args(["bender", "export"]);

    assert!(!output.status.success());
    assert_eq!(stderr(&output), FROM_BENDER.replace("{who}", "the root"));
    assert_eq!(
        std::fs::read_to_string(fixture.path().join("Bender.yml")).expect("a Bender.yml"),
        before
    );
}
