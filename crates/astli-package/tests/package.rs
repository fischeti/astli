//! Finding the package a directory is in, and what its manifest may say.

use std::path::{Path, PathBuf};

use astli_package::{
    At, Declared, Dependencies, Dependency, Error, Package, Patches, Selection, Source,
};

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
         dev = [\"hw/tb/**\"]\n\
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
    assert_eq!(package.dev, ["hw/tb/**"]);
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

    let exclude = refusal(&fixture, "[package]\nexclude = [\"hw/[ab\"]\n");
    let dev = refusal(&fixture, "[package]\ndev = [\"tb/{a\"]\n");

    assert!(exclude.contains("`exclude`: `hw/[ab`"), "{exclude}");
    assert!(dev.contains("`dev`: `tb/{a`"), "{dev}");
}

#[test]
fn an_unknown_key_or_table_is_refused() {
    let fixture = Fixture::new("unknown");

    let key = refusal(&fixture, "[package]\nsource = [\"hw\"]\n");
    let table = refusal(&fixture, "[dev_dependencies]\naxi = {}\n");
    let entry = refusal(&fixture, "[dependencies]\naxi = { branch = \"main\" }\n");

    assert!(key.contains("unknown field `source`"), "{key}");
    assert!(
        table.contains("unknown field `dev_dependencies`"),
        "{table}"
    );
    assert!(entry.contains("unknown field `branch`"), "{entry}");
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
        slashed(
            path.strip_prefix(&package.root)
                .expect("a file inside the root"),
        )
    };
    files.iter().map(relative).collect()
}

/// `path` with `/` between its components, as the cases spell it on every
/// platform.
fn slashed(path: &Path) -> String {
    let parts: Vec<_> = (path.components())
        .map(|part| part.as_os_str().to_string_lossy())
        .collect();
    parts.join("/")
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
fn a_walk_skips_what_a_package_depends_on_by_path() {
    let fixture = Fixture::new("walk-path-dependency");
    fixture.file(
        "astli.toml",
        "[dependencies]\nip = { path = 'vendor/./ip' }\n",
    );
    fixture.file("hw/top.sv", "");
    fixture.file("vendor/ip/Bender.yml", "package:\n  name: ip\n");
    fixture.file("vendor/ip/rtl/ip.sv", "");

    assert_eq!(walked(fixture.path()), ["hw/top.sv"]);
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

#[test]
fn dev_files_are_walked_but_not_by_a_dependent() {
    let fixture = Fixture::new("walk-dev");
    fixture.file(
        "astli.toml",
        "[package]\ndev = [\"test\", \"**/*_tb.sv\"]\n",
    );
    fixture.file("rtl/fifo.sv", "");
    fixture.file("rtl/fifo_tb.sv", "");
    fixture.file("test/tb_fifo.sv", "");

    let package = found(fixture.path());
    let as_dependency = package.clone().as_dependency();

    assert_eq!(package.walk().expect("a walk").len(), 3);
    assert_eq!(
        as_dependency.walk().expect("a walk"),
        [fixture.path().join("rtl/fifo.sv")]
    );
}

#[test]
fn a_dependents_include_does_not_find_a_dev_file_by_suffix() {
    let fixture = Fixture::new("dev-suffix");
    fixture.file("astli.toml", "[package]\ndev = [\"test/**\"]\n");
    fixture.file("rtl/top.sv", "`include \"tb_macros.svh\"\n");
    fixture.file("test/tb_macros.svh", "");

    let package = found(fixture.path());
    let own = package.files(&[]).expect("the package's files");
    let seen = package
        .as_dependency()
        .files(&[])
        .expect("the package's files");

    assert_eq!(own.headers, [fixture.path().join("test/tb_macros.svh")]);
    assert!(seen.headers.is_empty(), "{:?}", seen.headers);
}

/// The package's units and headers, relative to its root.
fn told_apart(root: &Path) -> (Vec<String>, Vec<String>) {
    let package = found(root);
    let files = package.files(&[]).expect("the package's files");
    let relative = |paths: &[PathBuf]| -> Vec<String> {
        let relative =
            |path: &PathBuf| slashed(path.strip_prefix(&package.root).expect("inside the root"));
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

    let files = package.files(&[]).expect("the package's files");
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

#[test]
fn a_dependency_header_resolves_but_is_not_the_packages() {
    let fixture = Fixture::new("headers-dependency");
    fixture.file("soc/astli.toml", "");
    fixture.file("soc/top.sv", "`include \"cells/registers.svh\"\n");
    fixture.file("cells/include/cells/registers.svh", "");
    let package = found(&fixture.path().join("soc"));
    let dependencies = [fixture.path().join("cells/include")];

    let files = package.files(&dependencies).expect("the package's files");
    let resolved = files.build.resolve(
        "cells/registers.svh",
        Some(&package.root.join("top.sv")),
        false,
        Path::is_file,
    );

    assert_eq!(files.units, [package.root.join("top.sv")]);
    assert!(files.headers.is_empty());
    assert_eq!(
        resolved,
        astli_preproc::Resolved::Found(fixture.path().join("cells/include/cells/registers.svh"))
    );
}

#[test]
fn include_is_the_default_include_dir() {
    let fixture = Fixture::new("include-default");
    fixture.file("ip/include/ip/defs.svh", "");
    fixture.file("ip/rtl/ip.sv", "");
    fixture.file("bare/include/bare.svh", "");
    fixture.file("bare/rtl/x.sv", "");
    fixture.file("none/astli.toml", "[package]\ninclude-dirs = []\n");
    fixture.dir("none/include");

    // With a manifest that sets nothing, and with none at all.
    fixture.file("ip/astli.toml", "");
    let ip = Package::at(&fixture.path().join("ip")).unwrap();
    let bare = Package::at(&fixture.path().join("bare")).unwrap();
    let none = Package::at(&fixture.path().join("none")).unwrap();

    assert_eq!(ip.include_dirs, [fixture.path().join("ip/include")]);
    assert_eq!(bare.include_dirs, [fixture.path().join("bare/include")]);
    assert_eq!(bare.manifest, None);
    // Written empty, it exports nothing.
    assert!(none.include_dirs.is_empty());
}

#[test]
fn a_package_at_a_directory_is_not_looked_for_above_it() {
    // As a dependency checked out without a repository of its own.
    let fixture = Fixture::new("at");
    fixture.file("astli.toml", "[package]\nname = \"outer\"\n");
    let dir = fixture.dir("deps/cells");

    let package = Package::at(&dir).unwrap();

    assert_eq!(package.root, dir);
    assert_eq!(package.name, None);
}

#[test]
fn an_include_found_by_suffix_gives_the_directory_a_tool_needs() {
    let fixture = Fixture::new("suffix-dirs");
    fixture.file("astli.toml", "[package]\ninclude-dirs = [\"include\"]\n");
    fixture.file(
        "src/xbar.sv",
        "`include \"axi/typedef.svh\"\n`include \"regs/defs.svh\"\n`include \"local.svh\"\n",
    );
    fixture.file("include/axi/typedef.svh", "");
    fixture.file("src/gen/regs/defs.svh", "");
    fixture.file("src/local.svh", "");
    let package = found(fixture.path());

    let files = package.files(&[]).expect("the package's files");

    // Neither the include directory nor the includer's own is needed again.
    assert_eq!(files.suffix_dirs, [package.root.join("src/gen")]);
}

const FEATURES: &str = "\
    [package]\n\
    name = \"axi\"\n\
    \n\
    [features]\n\
    default = [\"vip\"]\n\
    vip = { sources = [\"vip\"], include-dirs = [\"vip/include\"], dependencies = [\"cells/sim\"] }\n\
    sim = { sources = [\"vip/rand.sv\", \"sim/**\"] }\n\
    \n\
    [dependencies]\n\
    cells = { features = [\"asic\"] }\n";

#[test]
fn features_are_read_and_all_enabled() {
    let fixture = Fixture::new("features-read");
    fixture.dir("vip/include");
    let manifest = fixture.file("astli.toml", FEATURES);

    let package = Package::read(&manifest).expect("a valid manifest");

    let vip = &package.features["vip"];
    assert_eq!(vip.sources, ["vip"]);
    assert_eq!(vip.include_dirs, [fixture.path().join("vip/include")]);
    assert_eq!(
        vip.dependencies,
        [("cells".to_string(), Some("sim".to_string()))]
    );
    assert_eq!(package.default_features, ["vip"]);
    assert_eq!(package.dependencies["cells"].features, ["asic"]);
    // The package's own runs enable every feature, a dependent its default.
    assert!(package.enabled.iter().eq(["sim", "vip"]));
    assert!(package.clone().as_dependency().enabled.iter().eq(["vip"]));
    assert_eq!(package.search_dirs(), [fixture.path().join("vip/include")]);
}

#[test]
fn a_malformed_feature_is_refused() {
    let fixture = Fixture::new("features-refused");

    let default = refusal(&fixture, "[features]\ndefault = [\"vip\"]\n");
    let bare = refusal(
        &fixture,
        "[features]\nvip = { dependencies = [\"cells\"] }\n",
    );
    let slash = refusal(&fixture, "[features.\"a/b\"]\n");
    let key = refusal(&fixture, "[features]\nvip = { source = [\"vip\"] }\n");
    let outside = refusal(&fixture, "[features]\nvip = { include-dirs = [\"..\"] }\n");
    let pattern = refusal(&fixture, "[features]\nvip = { sources = [\"vip/[a\"] }\n");

    assert!(
        default.contains("`features.default`: `vip` is not a feature"),
        "{default}"
    );
    assert!(
        bare.contains("`cells` is not one of `dependencies`"),
        "{bare}"
    );
    assert!(slash.contains("`a/b` is not a feature's name"), "{slash}");
    assert!(
        key.contains("`features.vip`: unknown field `source`"),
        "{key}"
    );
    assert!(
        outside.contains("`features.vip.include-dirs`: `..` is outside"),
        "{outside}"
    );
    assert!(
        pattern.contains("`features.vip.sources`: `vip/[a`"),
        "{pattern}"
    );
}

/// The walk of `package`, relative to its root.
fn walked_of(package: &Package) -> Vec<String> {
    let files = package.walk().expect("a walk");
    let relative = |path: &PathBuf| slashed(path.strip_prefix(&package.root).expect("inside"));
    files.iter().map(relative).collect()
}

#[test]
fn a_file_only_disabled_features_name_is_not_walked() {
    let fixture = Fixture::new("features-walk");
    fixture.dir("vip/include");
    fixture.file("astli.toml", FEATURES);
    fixture.file("rtl/xbar.sv", "");
    fixture.file("vip/test.sv", "");
    fixture.file("vip/rand.sv", "");
    fixture.file("sim/tb.sv", "");
    let mut package = found(fixture.path());

    let all = walked_of(&package);
    package.enabled = ["sim".to_string()].into();
    let sim = walked_of(&package);
    package.enabled.clear();
    let none = walked_of(&package);

    assert_eq!(
        all,
        ["rtl/xbar.sv", "sim/tb.sv", "vip/rand.sv", "vip/test.sv"]
    );
    // `vip/rand.sv` is named by both, so either keeps it.
    assert_eq!(sim, ["rtl/xbar.sv", "sim/tb.sv", "vip/rand.sv"]);
    assert_eq!(none, ["rtl/xbar.sv"]);
    assert!(package.search_dirs().is_empty());
}

/// A root that depends on `axi`, which depends on `cells`, each with the
/// manifest given.
fn design(fixture: &Fixture, root: &str, cells: &str) -> (Package, Dependencies) {
    let axi = "[dependencies]\naxi = { path = '../axi' }\n";
    let root = match root.contains("[dependencies]\n") {
        true => root.replace("[dependencies]\n", axi),
        false => format!("{root}{axi}"),
    };
    fixture.file("root/astli.toml", &root);
    fixture.dir("axi/vip/include");
    fixture.file("axi/astli.toml", FEATURES);
    fixture.file("cells/astli.toml", cells);
    let at = |dir: &str| Package::at(&fixture.path().join(dir)).expect("a package");
    let dependency = |name: &str, needs: &[&str]| Dependency {
        name: name.to_string(),
        package: at(name).as_dependency(),
        needs: needs.iter().map(|it| it.to_string()).collect(),
    };
    let dependencies = Dependencies {
        packages: vec![dependency("cells", &[]), dependency("axi", &["cells"])],
    };
    (at("root"), dependencies)
}

const CELLS: &str = "[package]\nname = \"cells\"\n[features]\nasic = {}\nsim = {}\nfpga = {}\n";

fn enabled(package: &Package) -> Vec<&str> {
    package.enabled.iter().map(String::as_str).collect()
}

#[test]
fn a_dependency_gets_its_defaults_and_what_anything_asks() {
    let fixture = Fixture::new("select");
    let (mut root, mut dependencies) = design(&fixture, "[features]\nown = {}\n", CELLS);

    dependencies
        .select(&mut root, &Selection::default())
        .expect("a selection");

    let [cells, axi] = &dependencies.packages[..] else {
        panic!("two dependencies");
    };
    assert_eq!(enabled(&root), ["own"]);
    // Its default, `vip`, which asks for `cells/sim`; and `asic`, which axi's
    // `[dependencies]` asks for.
    assert_eq!(enabled(&axi.package), ["vip"]);
    assert_eq!(enabled(&cells.package), ["asic", "sim"]);
}

#[test]
fn the_command_line_selects_only_the_roots_own() {
    let fixture = Fixture::new("select-flags");
    let root = "[features]\nown = {}\nmore = {}\nfpga = { dependencies = [\"cells/fpga\"] }\n\
                [dependencies]\ncells = { path = '../cells' }\n";
    let selection = |features: &[&str]| Selection {
        no_features: true,
        features: features.iter().map(|it| it.to_string()).collect(),
    };

    let (mut root, mut dependencies) = design(&fixture, root, CELLS);
    dependencies
        .select(&mut root, &selection(&["more", "fpga"]))
        .expect("a selection");
    let refused = dependencies
        .select(&mut root, &selection(&["cells/fpga"]))
        .expect_err("a dependency's");

    assert_eq!(enabled(&root), ["fpga", "more"]);
    assert_eq!(
        enabled(&dependencies.packages[0].package),
        ["asic", "fpga", "sim"]
    );
    assert!(
        refused
            .to_string()
            .contains("`cells/fpga` is a dependency's"),
        "{refused}"
    );
}

#[test]
fn a_feature_nobody_declares_or_a_package_not_in_the_design_is_refused() {
    let fixture = Fixture::new("select-refused");
    let refused = |root: &str, cells: &str, features: &[&str]| {
        let (mut root, mut dependencies) = design(&fixture, root, cells);
        let selection = Selection {
            no_features: false,
            features: features.iter().map(|it| it.to_string()).collect(),
        };
        let err = dependencies
            .select(&mut root, &selection)
            .expect_err("refused");
        err.to_string()
    };

    let missing = refused(
        "",
        "[package]\nname = \"cells\"\n[features]\nasic = {}\n",
        &[],
    );
    let flag = refused("", CELLS, &["nope"]);
    let absent = refused("[dependencies]\nmem = { features = [\"x\"] }\n", CELLS, &[]);

    assert!(
        missing.contains("`axi` enables `sim` of `cells`, which declares no such feature"),
        "{missing}"
    );
    assert!(
        flag.contains("`--features` enables `nope` of the root package"),
        "{flag}"
    );
    assert!(
        absent.contains("the root package asks for features of `mem`, which is not in the design"),
        "{absent}"
    );
}

/// A root and two dependencies, `cells` and `soc`, with the manifests given.
fn overriding(fixture: &Fixture, root: &str, soc: &str) -> (Package, Dependencies) {
    fixture.file("root/astli.toml", root);
    fixture.file("cells/astli.toml", "[package]\nname = \"cells\"\n");
    fixture.file("soc/astli.toml", soc);
    let at = |dir: &str| Package::at(&fixture.path().join(dir)).expect("a package");
    let dependency = |name: &str| Dependency {
        name: name.to_string(),
        package: at(name).as_dependency(),
        needs: Vec::new(),
    };
    let dependencies = Dependencies {
        packages: vec![dependency("cells"), dependency("soc")],
    };
    (at("root"), dependencies)
}

/// Units of the root (0), `cells` (1) and `soc` (2), each declaring the names
/// given.
fn units<'a>(declared: &'a [(usize, &'a str, &'a [&'a str])]) -> Vec<Declared<'a>> {
    (declared.iter())
        .map(|(package, path, names)| Declared {
            package: *package,
            path: Path::new(path),
            names: names.to_vec(),
        })
        .collect()
}

const CLOCKS: &[(usize, &str, &[&str])] = &[
    (0, "root/tc_clk.sv", &["tc_clk_gating", "tc_clk_mux2"]),
    (1, "cells/tc_clk.sv", &["tc_clk_gating", "tc_clk_mux2"]),
    (1, "cells/tc_sram.sv", &["tc_sram"]),
    (1, "cells/tc_pwr.sv", &["tc_pwr"]),
];

#[test]
fn an_override_drops_the_dependency_files_declaring_what_it_replaces() {
    let fixture = Fixture::new("override");
    let (root, dependencies) = overriding(
        &fixture,
        "[overrides]\ncells = [\"tc_clk_*\", \"tc_sram\"]\n",
        "",
    );

    let dropped = dependencies
        .replaced(&root, &units(CLOCKS))
        .expect("replaced");

    // `tc_sram` may be replaced, but nothing of the root's declares it.
    assert_eq!(dropped, [false, true, false, false]);
}

#[test]
fn without_an_override_nothing_is_dropped() {
    let fixture = Fixture::new("override-none");
    let (root, dependencies) = overriding(&fixture, "", "");

    let dropped = dependencies
        .replaced(&root, &units(CLOCKS))
        .expect("replaced");

    assert!(dropped.iter().all(|dropped| !dropped));
}

#[test]
fn a_file_declaring_a_name_nothing_replaces_is_refused() {
    let fixture = Fixture::new("override-whole");
    let (root, dependencies) = overriding(&fixture, "[overrides]\ncells = [\"tc_clk_*\"]\n", "");
    let declared = [
        (0, "root/tc_clk.sv", &["tc_clk_gating"][..]),
        (1, "cells/tc_clk.sv", &["tc_clk_gating", "tc_clk_or2"][..]),
    ];

    let err = dependencies
        .replaced(&root, &units(&declared))
        .expect_err("refused");

    assert!(
        err.to_string().contains(
            "cells/tc_clk.sv declares `tc_clk_gating`, which the root package replaces, and \
             `tc_clk_or2`, which nothing does"
        ),
        "{err}"
    );
}

#[test]
fn two_packages_replacing_one_name_are_refused() {
    let fixture = Fixture::new("override-twice");
    let (root, dependencies) = overriding(
        &fixture,
        "[overrides]\ncells = [\"tc_sram\"]\n",
        "[package]\nname = \"soc\"\n[overrides]\ncells = [\"tc_*\"]\n",
    );
    let declared = [
        (0, "root/sram.sv", &["tc_sram"][..]),
        (2, "soc/sram.sv", &["tc_sram"][..]),
        (1, "cells/tc_sram.sv", &["tc_sram"][..]),
    ];

    let err = dependencies
        .replaced(&root, &units(&declared))
        .expect_err("refused");

    assert!(
        err.to_string().contains(
            "`tc_sram` of `cells` is replaced twice: by the root package in root/sram.sv and \
             by `soc` in soc/sram.sv"
        ),
        "{err}"
    );
}

#[test]
fn an_override_of_a_package_not_in_the_design_is_refused() {
    let fixture = Fixture::new("override-absent");
    let (root, dependencies) = overriding(&fixture, "[overrides]\nmem = [\"x\"]\n", "");
    let malformed = refusal(&fixture, "[overrides]\ncells = [\"tc_[a\"]\n");

    let err = dependencies.replaced(&root, &[]).expect_err("refused");

    assert!(
        err.to_string().contains("overrides names of `mem`"),
        "{err}"
    );
    assert!(
        malformed.contains("`overrides.cells`: `tc_[a`"),
        "{malformed}"
    );
}

#[test]
fn a_dependency_says_where_it_comes_from() {
    let fixture = Fixture::new("sources");
    fixture.dir("vendor/cells");
    let manifest = fixture.file(
        "astli.toml",
        "[dependencies]\n\
         axi = { git = \"https://github.com/pulp-platform/axi.git\", version = \"0.39\" }\n\
         idma = { git = \"https://github.com/pulp-platform/iDMA.git\", rev = \"main\" }\n\
         cells = { path = \"vendor/cells\", features = [\"asic\"] }\n\
         tech = {}\n",
    );

    let package = Package::read(&manifest).expect("a valid manifest");

    let source = |name: &str| package.dependencies[name].source.clone();
    let Some(Source::Git {
        url,
        at: At::Version(required),
    }) = source("axi")
    else {
        panic!("a repository at a version: {:?}", source("axi"));
    };
    assert_eq!(url, "https://github.com/pulp-platform/axi.git");
    assert_eq!(required.to_string(), "^0.39");
    assert!(matches!(source("idma"), Some(Source::Git { at: At::Rev(rev), .. }) if rev == "main"));
    assert_eq!(
        source("cells"),
        Some(Source::Path(fixture.path().join("vendor/cells")))
    );
    // Read, and refused once resolved.
    assert_eq!(source("tech"), None);
}

#[test]
fn a_malformed_dependency_is_refused() {
    let fixture = Fixture::new("sources-refused");

    let both = refusal(
        &fixture,
        "[dependencies]\naxi = { git = \"x\", path = \"y\" }\n",
    );
    let neither = refusal(&fixture, "[dependencies]\naxi = { git = \"x\" }\n");
    let loose = refusal(&fixture, "[dependencies]\naxi = { version = \"1\" }\n");
    let missing = refusal(&fixture, "[dependencies]\naxi = { path = \"axi\" }\n");

    assert!(
        both.contains("`dependencies.axi`: both `git` and `path`"),
        "{both}"
    );
    assert!(neither.contains("needs `version` or `rev`"), "{neither}");
    assert!(loose.contains("`version` and `rev` need `git`"), "{loose}");
    assert!(
        missing.contains("`dependencies.axi.path`: `axi` is not a directory"),
        "{missing}"
    );
}

const BENDER: &str = "\
package:
  name: soc
dependencies:
  axi: { git: \"https://github.com/pulp-platform/axi.git\", version: 0.39.8 }
  local: { path: ./local }
";

#[test]
fn without_an_astli_toml_the_bender_manifest_lists_the_dependencies() {
    let fixture = Fixture::new("bender-listed");
    fixture.dir("ip/local");
    fixture.file("ip/Bender.yml", BENDER);

    let package = Package::at(&fixture.path().join("ip")).expect("a package");

    assert!(package.dependencies.keys().eq(["axi", "local"]));
    assert_eq!(
        package.dependencies["local"].source,
        Some(Source::Path(fixture.path().join("ip/local")))
    );
}

#[test]
fn a_table_of_dependencies_replaces_the_bender_manifest() {
    let fixture = Fixture::new("bender-unread");
    fixture.dir("ip/local");
    fixture.file("ip/Bender.yml", BENDER);
    fixture.file("ip/astli.toml", "[fmt]\n");
    let without = Package::at(&fixture.path().join("ip")).expect("a package");
    fixture.file("ip/astli.toml", "[dependencies]\n");
    let with = Package::at(&fixture.path().join("ip")).expect("a package");
    fixture.file("bad/Bender.yml", "dependencies:\n  axi: { git: \"x\" }\n");
    let bad = Package::at(&fixture.path().join("bad")).expect_err("refused");

    // An `astli.toml` for other reasons keeps what the `Bender.yml` lists.
    assert!(without.dependencies.keys().eq(["axi", "local"]));
    assert!(with.dependencies.is_empty());
    assert!(
        matches!(&bad, Error::Manifest(path, _) if path.ends_with("Bender.yml")),
        "{bad}"
    );
}

#[test]
fn the_packages_the_root_pins_are_read() {
    let fixture = Fixture::new("pinned");
    let manifest = fixture.file("astli.toml", "[resolve]\npinned = [\"axi\", \"cells\"]\n");
    let package = Package::read(&manifest).expect("a valid manifest");
    let unknown = refusal(&fixture, "[resolve]\npin = [\"axi\"]\n");

    assert_eq!(package.pinned, ["axi", "cells"]);
    assert!(unknown.contains("unknown field `pin`"), "{unknown}");
}

fn names(dependencies: &Dependencies) -> Vec<&str> {
    (dependencies.packages.iter())
        .map(|dependency| dependency.name.as_str())
        .collect()
}

/// The root at `dir` under `fixture`, with `found`, each a directory beside
/// it named for its package, as resolution would hand them over, selected.
fn selected(
    fixture: &Fixture,
    dir: &str,
    found: &[&str],
    selection: &Selection,
) -> (Package, Dependencies) {
    let mut root = Package::at(&fixture.path().join(dir)).expect("a package");
    let found = (found.iter())
        .map(|name| {
            let package = Package::at(&fixture.path().join(name)).expect("a package");
            let needs = package.dependencies.keys().cloned().collect();
            (name.to_string(), package.as_dependency(), needs)
        })
        .collect();
    let mut dependencies = Dependencies::ordered(&root, found).expect("ordered");
    dependencies
        .select(&mut root, selection)
        .expect("a selection");
    (root, dependencies)
}

#[test]
fn a_dependency_a_feature_names_alone_is_in_the_design_only_with_it() {
    let fixture = Fixture::new("optional");
    fixture.file(
        "root/astli.toml",
        "[features]\nsim = { dependencies = [\"models\"] }\n\
         [dependencies]\nmodels = { path = \"../models\" }\ncells = { path = \"../cells\" }\n\
         [dev-dependencies]\nverif = { path = \"../verif\" }\n",
    );
    for name in ["models", "cells", "verif"] {
        fixture.file(&format!("{name}/astli.toml"), "");
    }
    let without = Selection {
        no_features: true,
        features: Vec::new(),
    };
    let found = ["models", "cells", "verif"];

    let (_, all) = selected(&fixture, "root", &found, &Selection::default());
    let (_, none) = selected(&fixture, "root", &found, &without);

    assert_eq!(names(&all), ["cells", "models", "verif"]);
    assert_eq!(names(&none), ["cells", "verif"]);
}

#[test]
fn defaults_are_off_only_where_every_requirement_turns_them_off() {
    let fixture = Fixture::new("defaults");
    fixture.file(
        "cells/astli.toml",
        "[features]\ndefault = [\"asic\"]\nasic = {}\nfpga = {}\n",
    );
    let design = |axi: &str| {
        fixture.file(
            "root/astli.toml",
            "[dependencies]\naxi = { path = \"../axi\" }\n\
             cells = { path = \"../cells\", default-features = false, features = [\"fpga\"] }\n",
        );
        fixture.file(
            "axi/astli.toml",
            &format!("[dependencies]\ncells = {axi}\n"),
        );
        let (_, dependencies) =
            selected(&fixture, "root", &["axi", "cells"], &Selection::default());
        let cells = &dependencies.packages[0].package;
        cells.enabled.iter().cloned().collect::<Vec<_>>()
    };

    let off = design("{ path = \"../cells\", default-features = false }");
    let on = design("{ path = \"../cells\" }");

    assert_eq!(off, ["fpga"]);
    assert_eq!(on, ["asic", "fpga"]);
}

#[test]
fn a_dependency_in_both_tables_is_refused() {
    let fixture = Fixture::new("both-tables");
    fixture.dir("axi");

    let err = refusal(
        &fixture,
        "[dependencies]\naxi = { path = \"axi\" }\n[dev-dependencies]\naxi = { path = \"axi\" }\n",
    );

    assert!(
        err.contains("`axi` is in both `dependencies` and `dev-dependencies`"),
        "{err}"
    );
}

#[test]
fn the_root_reads_its_patches_and_its_walk_stops_at_them() {
    let fixture = Fixture::new("patches");
    fixture.file("astli.toml", "");
    fixture.file("top.sv", "module top;\nendmodule\n");
    fixture.file("work/cells/cells.sv", "module cells;\nendmodule\n");
    let mut patches = Patches::default();
    (patches.0).insert("cells".to_string(), fixture.path().join("work/cells"));

    patches.write(fixture.path()).expect("written");
    let package = found(fixture.path());
    fixture.file(
        ".astli/patch.toml",
        "[patch]\ncells = { path = 'work/cells' }\n",
    );
    let relative = Patches::read(fixture.path()).expect("read");
    fixture.file(
        ".astli/patch.toml",
        "[patch]\ncells = { path = 'nowhere' }\n",
    );
    let missing = Patches::read(fixture.path()).expect_err("refused");

    assert_eq!(package.patches, patches);
    assert_eq!(relative, patches);
    let walked = package.walk().expect("walked");
    assert_eq!(walked, [fixture.path().join("top.sv")]);
    assert!(
        missing.to_string().contains("`nowhere` is not a directory"),
        "{missing}"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.path().join(".astli/.gitignore")).expect("written"),
        "*\n"
    );
}
