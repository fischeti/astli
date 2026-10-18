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
