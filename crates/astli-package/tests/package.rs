//! Finding the package a directory is in, and what its manifest may say.

use std::path::{Path, PathBuf};

use astli_package::{Error, Package};

/// A directory of its own under the system's temporary directory, removed
/// when dropped.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("astli-package-{name}-{}", std::process::id()));
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

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).expect("a directory");
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

fn found(dir: &Path) -> Package {
    Package::find(dir)
        .expect("a readable package")
        .expect("a package")
}

/// What reading the manifest `text` is refused with.
fn refusal(fixture: &Fixture, text: &str) -> String {
    let manifest = fixture.file("astli.toml", text);
    match Package::read(&manifest) {
        Ok(package) => panic!("read: {package:?}"),
        Err(err) => err.to_string(),
    }
}

#[test]
fn the_nearest_manifest_above_is_the_root() {
    let fixture = Fixture::new("nearest");
    fixture.dir(".git");
    fixture.file("astli.toml", "");
    fixture.file("ip/astli.toml", "[package]\nname = \"ip\"\n");
    let below = fixture.dir("ip/rtl/core");

    let package = found(&below);

    assert_eq!(package.root, fixture.path().join("ip"));
    assert_eq!(package.name.as_deref(), Some("ip"));
    assert_eq!(package.manifest, Some(fixture.path().join("ip/astli.toml")));
}

#[test]
fn without_a_manifest_the_git_root_is_the_root() {
    let fixture = Fixture::new("git-root");
    fixture.dir(".git");
    let below = fixture.dir("hw/rtl");

    let package = found(&below);

    assert_eq!(package.root, fixture.path());
    assert_eq!(package.manifest, None);
    assert_eq!(package.sources, [fixture.path()]);
}

#[test]
fn a_git_file_marks_a_root_too() {
    // As a worktree or a submodule has.
    let fixture = Fixture::new("git-file");
    fixture.file("sub/.git", "gitdir: ../.git/modules/sub\n");

    let package = found(&fixture.dir("sub/rtl"));

    assert_eq!(package.root, fixture.path().join("sub"));
}

#[test]
fn a_manifest_above_the_repository_is_not_its_own() {
    let fixture = Fixture::new("above-git");
    fixture.file("astli.toml", "[package]\nname = \"outer\"\n");
    fixture.dir("repo/.git");

    let package = found(&fixture.dir("repo/rtl"));

    assert_eq!(package.root, fixture.path().join("repo"));
    assert_eq!(package.name, None);
}

#[test]
fn a_parent_component_is_followed_before_the_walk() {
    let fixture = Fixture::new("parent-component");
    fixture.file("a/astli.toml", "");
    fixture.file("a/b/astli.toml", "[package]\nname = \"b\"\n");

    let package = found(&fixture.path().join("a/b/.."));

    assert_eq!(package.root, fixture.path().join("a"));
}

#[test]
fn the_package_section_is_read_relative_to_the_root() {
    let fixture = Fixture::new("section");
    fixture.dir("hw/include");
    fixture.file("hw/top.sv", "");
    let manifest = fixture.file(
        "astli.toml",
        "[package]\n\
         name = \"soc\"\n\
         sources = [\"hw\", \"./hw/top.sv\"]\n\
         include-dirs = [\"hw/include\"]\n\
         exclude = [\"hw/scripts/**\"]\n\
         \n\
         [fmt]\nwidth = 80\n\
         \n\
         [lint]\nwarn = [\"restriction\"]\n",
    );

    let package = Package::read(&manifest).expect("a valid manifest");

    let root = fixture.path();
    assert_eq!(package.root, root);
    assert_eq!(package.name.as_deref(), Some("soc"));
    assert_eq!(package.sources, [root.join("hw"), root.join("hw/top.sv")]);
    assert_eq!(package.include_dirs, [root.join("hw/include")]);
    assert_eq!(package.exclude, ["hw/scripts/**"]);
    assert_eq!(package.fmt["width"].as_integer(), Some(80));
    assert!(package.lint.contains_key("warn"));
}

#[test]
fn an_empty_manifest_takes_the_defaults() {
    let fixture = Fixture::new("empty");
    let manifest = fixture.file("astli.toml", "");

    let package = Package::read(&manifest).expect("a valid manifest");

    assert_eq!(package.sources, [fixture.path()]);
    assert!(package.include_dirs.is_empty() && package.exclude.is_empty());
    assert!(package.fmt.is_empty() && package.lint.is_empty());
}

#[test]
fn a_source_outside_the_package_is_refused() {
    let fixture = Fixture::new("outside");
    fixture.dir("hw");

    let parent = refusal(&fixture, "[package]\nsources = [\"hw/../..\"]\n");
    let absolute = refusal(&fixture, "[package]\ninclude-dirs = [\"/usr/include\"]\n");

    assert!(
        parent.contains("`sources`: `hw/../..` is outside the package"),
        "{parent}"
    );
    assert!(
        absolute.contains("`include-dirs`: `/usr/include` is outside"),
        "{absolute}"
    );
}

#[test]
fn a_path_that_does_not_exist_is_refused() {
    let fixture = Fixture::new("missing");

    let said = refusal(&fixture, "[package]\nsources = [\"rtl\"]\n");

    assert!(
        said.contains("astli.toml: `sources`: `rtl` does not exist"),
        "{said}"
    );
}

#[test]
fn a_malformed_pattern_is_refused() {
    let fixture = Fixture::new("pattern");

    let said = refusal(&fixture, "[package]\nexclude = [\"hw/[ab\"]\n");

    assert!(said.contains("`exclude`: `hw/[ab`"), "{said}");
}

#[test]
fn an_unknown_key_or_table_is_refused() {
    let fixture = Fixture::new("unknown");

    let key = refusal(&fixture, "[package]\nsource = [\"hw\"]\n");
    let table = refusal(&fixture, "[dependencies]\naxi = { path = \"../axi\" }\n");

    assert!(key.contains("unknown field `source`"), "{key}");
    assert!(table.contains("unknown field `dependencies`"), "{table}");
}

#[test]
fn a_missing_manifest_names_its_path() {
    let fixture = Fixture::new("no-manifest");
    let path = fixture.path().join("astli.toml");

    let err = Package::read(&path).expect_err("no manifest to read");

    assert!(matches!(&err, Error::Io(at, _) if *at == path), "{err}");
}

/// The files a walk of the package at `root` yields, relative to it.
fn walked(root: &Path) -> Vec<String> {
    let package = found(root);
    let files = package.walk().expect("a walk");
    let relative = |path: &PathBuf| {
        let path = path
            .strip_prefix(&package.root)
            .expect("a file inside the root");
        path.display().to_string()
    };
    files.iter().map(relative).collect()
}

#[test]
fn a_walk_yields_the_systemverilog_files_sorted() {
    let fixture = Fixture::new("walk");
    fixture.file("astli.toml", "");
    fixture.file("rtl/b.sv", "");
    fixture.file("rtl/a.v", "");
    fixture.file("include/defs.svh", "");
    fixture.file("include/old.vh", "");
    fixture.file("rtl/notes.md", "");
    fixture.file("rtl/regs.hjson", "");

    assert_eq!(
        walked(fixture.path()),
        ["include/defs.svh", "include/old.vh", "rtl/a.v", "rtl/b.sv"]
    );
}

#[test]
fn a_walk_honours_gitignore_without_a_repository() {
    let fixture = Fixture::new("walk-gitignore");
    fixture.file("astli.toml", "");
    fixture.file(".gitignore", "build/\n*_gen.sv\n");
    fixture.file("rtl/.gitignore", "!keep_gen.sv\n");
    fixture.file("build/out.sv", "");
    fixture.file("rtl/top.sv", "");
    fixture.file("rtl/regs_gen.sv", "");
    fixture.file("rtl/keep_gen.sv", "");

    assert_eq!(walked(fixture.path()), ["rtl/keep_gen.sv", "rtl/top.sv"]);
}

#[test]
fn a_gitignore_above_the_root_does_not_apply() {
    // As for a private package checked out inside the public one, which
    // ignores it.
    let fixture = Fixture::new("walk-above");
    fixture.dir(".git");
    fixture.file(".gitignore", "pd\n");
    fixture.file("pd/astli.toml", "");
    fixture.dir("pd/.git");
    fixture.file("pd/hw/chip.sv", "");

    assert_eq!(walked(&fixture.path().join("pd")), ["hw/chip.sv"]);
}

#[test]
fn a_walk_skips_git_but_not_other_hidden_directories() {
    let fixture = Fixture::new("walk-hidden");
    fixture.file("astli.toml", "");
    fixture.file(".git/hooks/odd.sv", "");
    fixture.file(".generated/hw/pkg.sv", "");

    assert_eq!(walked(fixture.path()), [".generated/hw/pkg.sv"]);
}

#[test]
fn a_walk_stops_at_a_nested_package() {
    let fixture = Fixture::new("walk-nested");
    fixture.file("astli.toml", "");
    fixture.file("hw/top.sv", "");
    fixture.file("vendor/ip/astli.toml", "");
    fixture.file("vendor/ip/rtl/ip.sv", "");
    fixture.file("vendor/loose.sv", "");

    assert_eq!(walked(fixture.path()), ["hw/top.sv", "vendor/loose.sv"]);
}

#[test]
fn sources_narrow_the_walk_and_exclude_removes_from_it() {
    let fixture = Fixture::new("walk-sources");
    fixture.file(
        "astli.toml",
        "[package]\nsources = [\"hw\", \"util/one.sv\"]\nexclude = [\"hw/scripts/**\", \"**/*_tb.sv\"]\n",
    );
    fixture.file("hw/top.sv", "");
    fixture.file("hw/top_tb.sv", "");
    fixture.file("hw/sub/core_tb.sv", "");
    fixture.file("hw/scripts/gen/tmpl.sv", "");
    fixture.file("util/one.sv", "");
    fixture.file("util/two.sv", "");
    fixture.file("sw/model.sv", "");

    assert_eq!(walked(fixture.path()), ["hw/top.sv", "util/one.sv"]);
}

#[test]
fn an_excluded_directory_is_not_entered() {
    let fixture = Fixture::new("walk-exclude-dir");
    fixture.file("astli.toml", "[package]\nexclude = [\"third_party\"]\n");
    fixture.file("third_party/x/y.sv", "");
    fixture.file("top.sv", "");

    assert_eq!(walked(fixture.path()), ["top.sv"]);
}

/// The package's units and headers, relative to its root.
fn told_apart(root: &Path) -> (Vec<String>, Vec<String>) {
    let package = found(root);
    let files = package.files().expect("the package's files");
    let relative = |paths: &[PathBuf]| -> Vec<String> {
        let relative = |path: &PathBuf| {
            let path = path.strip_prefix(&package.root).expect("inside the root");
            path.display().to_string()
        };
        paths.iter().map(relative).collect()
    };
    (relative(&files.units), relative(&files.headers))
}

#[test]
fn a_header_is_any_file_another_includes() {
    let fixture = Fixture::new("headers");
    fixture.file("astli.toml", "");
    fixture.file(
        "rtl/top.sv",
        "`include \"core_body.sv\"\nmodule top; endmodule\n",
    );
    fixture.file("rtl/core_body.sv", "`include \"defs.svh\"\n");
    fixture.file("rtl/defs.svh", "");
    fixture.file("rtl/unused.svh", "");
    fixture.file("rtl/old.v", "module old; endmodule\n");

    let (units, headers) = told_apart(fixture.path());

    assert_eq!(units, ["rtl/old.v", "rtl/top.sv"]);
    assert_eq!(headers, ["rtl/core_body.sv", "rtl/defs.svh"]);
}

#[test]
fn an_include_in_any_branch_makes_a_header() {
    let fixture = Fixture::new("headers-branch");
    fixture.file("astli.toml", "");
    fixture.file(
        "top.sv",
        "`ifdef VERILATOR\n`include \"sim_only.sv\"\n`else\n`include \"synth_only.sv\"\n`endif\n",
    );
    fixture.file("sim_only.sv", "");
    fixture.file("synth_only.sv", "");

    let (units, headers) = told_apart(fixture.path());

    assert_eq!(units, ["top.sv"]);
    assert_eq!(headers, ["sim_only.sv", "synth_only.sv"]);
}

#[test]
fn an_include_resolves_through_the_include_dirs_and_by_suffix() {
    let fixture = Fixture::new("headers-resolve");
    fixture.file(
        "astli.toml",
        "[package]\nsources = [\"src\"]\ninclude-dirs = [\"include\"]\n",
    );
    fixture.file(
        "src/xbar.sv",
        "`include \"axi/typedef.svh\"\n`include \"regs/defs.svh\"\n",
    );
    // Outside `sources`, so found only through the include directory, and
    // walked for its own includes.
    fixture.file("include/axi/typedef.svh", "`include \"axi/assign.inc\"\n");
    fixture.file("include/axi/assign.inc", "");
    fixture.file("src/gen/regs/defs.svh", "");

    let (units, headers) = told_apart(fixture.path());

    assert_eq!(units, ["src/xbar.sv"]);
    assert_eq!(
        headers,
        [
            "include/axi/assign.inc",
            "include/axi/typedef.svh",
            "src/gen/regs/defs.svh"
        ]
    );
}

#[test]
fn every_file_an_ambiguous_include_could_be_is_a_header() {
    let fixture = Fixture::new("headers-ambiguous");
    fixture.file("astli.toml", "");
    fixture.file("top.sv", "`include \"defs.sv\"\n");
    fixture.file("a/defs.sv", "");
    fixture.file("b/defs.sv", "");

    let (units, headers) = told_apart(fixture.path());

    assert_eq!(units, ["top.sv"]);
    assert_eq!(headers, ["a/defs.sv", "b/defs.sv"]);
}

#[test]
fn the_build_resolves_as_the_classification_does() {
    let fixture = Fixture::new("headers-build");
    fixture.file("astli.toml", "");
    fixture.file("rtl/top.sv", "`include \"pkg/defs.svh\"\n");
    fixture.file("include/pkg/defs.svh", "");
    let package = found(fixture.path());

    let files = package.files().expect("the package's files");
    let resolved = files.build.resolve(
        "pkg/defs.svh",
        Some(&package.root.join("rtl/top.sv")),
        false,
        Path::is_file,
    );

    assert_eq!(
        resolved,
        astli_preproc::Resolved::Found(package.root.join("include/pkg/defs.svh"))
    );
}

#[test]
fn a_walked_file_only_a_header_outside_the_walk_includes_is_a_header() {
    let fixture = Fixture::new("headers-through-unwalked");
    fixture.file(
        "astli.toml",
        "[package]\nsources = [\"src\"]\ninclude-dirs = [\"include\"]\n",
    );
    fixture.file("src/top.sv", "`include \"wrap.svh\"\n");
    fixture.file("include/wrap.svh", "`include \"body.sv\"\n");
    fixture.file("src/body.sv", "");

    let (units, headers) = told_apart(fixture.path());

    assert_eq!(units, ["src/top.sv"]);
    assert_eq!(headers, ["include/wrap.svh", "src/body.sv"]);
}
