//! The package manager: `add`, `remove`, `update`, `fetch`, `patch`,
//! `snapshot`, `tree`, `path`, `init` and the cache.

use std::path::{Path, PathBuf};

use crate::support::*;

#[test]
fn update_writes_the_lock_and_moves_only_what_it_names() {
    let fixture = Fixture::new("update");
    fixture.file("gitconfig", "");
    for name in ["cells", "axi"] {
        let dir = fixture.path().join(name);
        std::fs::create_dir_all(&dir).expect("a repository");
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(
            &dir,
            &["commit", "--quiet", "--allow-empty", "--message", "one"],
        );
        git(&dir, &["tag", "v1.0.0"]);
    }
    let url = |name: &str| fixture.path().join(name).display().to_string();
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\naxi = {{ git = '{}', version = '1' }}\ncells = {{ git = '{}', version = '1' }}\n",
            url("axi"),
            url("cells")
        ),
    );
    let root = fixture.path().join("root");
    let update = |args: &[&str]| {
        let output = driver(&fixture)
            .dir(&root)
            .cache()
            .args(["update"].iter().chain(args));
        assert!(output.status.success(), "{}", stderr(&output));
        said_in(&fixture, &output)
    };

    let first = update(&[]);
    for name in ["cells", "axi"] {
        let dir = fixture.path().join(name);
        git(
            &dir,
            &["commit", "--quiet", "--allow-empty", "--message", "two"],
        );
        git(&dir, &["tag", "v1.1.0"]);
    }
    let named = update(&["cells"]);
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");
    let all = update(&[]);

    assert_eq!(
        first,
        "    Fetching axi (<dir>/axi)\n    Fetching cells (<dir>/cells)\n      \
         Adding axi 1.0.0\n      Adding cells 1.0.0\n"
    );
    assert_eq!(
        named,
        "    Fetching cells (<dir>/cells)\n    Updating cells 1.0.0 -> 1.1.0\n"
    );
    assert!(
        lock.contains("version = \"1.0.0\"") && lock.contains("version = \"1.1.0\""),
        "{lock}"
    );
    assert_eq!(
        all,
        "    Fetching axi (<dir>/axi)\n    Fetching cells (<dir>/cells)\n    Updating axi 1.0.0 -> 1.1.0\n"
    );
}

#[test]
fn update_refuses_a_name_the_lock_does_not_hold() {
    let fixture = Fixture::new("update-unknown");
    fixture.file("astli.toml", "");
    fixture.file("astli.lock", "version = 1\n");

    let output = driver(&fixture).args(["update", "axi"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("`axi` is not in"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_package_command_reads_its_dependencies_and_writes_the_lock() {
    let fixture = Fixture::new("package-lock");
    let url = cells_repository(&fixture);
    let manifest = |required: &str| {
        format!(
            "[dependencies]\ncells = {{ git = '{url}', version = '{required}' }}\n\
             ip = {{ path = 'vendor/ip' }}\n"
        )
    };
    fixture.file("root/astli.toml", &manifest("1"));
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\n  ip u_ip ();\nendmodule\n",
    );
    fixture.file("root/vendor/ip/ip.sv", "module ip;\nendmodule\n");
    let root = fixture.path().join("root");

    let first = driver(&fixture).dir(&root).cache().args(["files"]);
    let locked = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--locked"]);
    fixture.file("root/astli.toml", &manifest("2"));
    let refused = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--locked"]);

    assert!(first.status.success(), "{}", stderr(&first));
    // Compared with `/`, which Windows prints as `\`.
    let listed = stdout(&first).replace('\\', "/");
    assert!(
        listed.contains("cells.sv") && listed.contains("vendor/ip/ip.sv"),
        "{listed}"
    );
    // Once, from the dependency, not again as the root's own.
    assert_eq!(listed.matches("ip.sv").count(), 1, "{listed}");
    assert_eq!(
        said_in(&fixture, &first),
        "    Fetching cells (<dir>/cells)\n      Adding cells 1.0.0\nChecking out cells 1.0.0\n"
    );
    assert!(root.join("astli.lock").is_file());
    assert!(locked.status.success(), "{}", stderr(&locked));
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("version"), "{}", stderr(&refused));
}

#[test]
fn a_package_only_bender_manages_gets_no_lock_unless_asked() {
    let fixture = Fixture::new("bender-lock");
    let url = cells_repository(&fixture);
    fixture.file(
        "root/Bender.yml",
        &format!(
            "package:\n  name: root\ndependencies:\n  cells: {{ git: '{url}', version: 1.0.0 }}\n"
        ),
    );
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\nendmodule\n",
    );
    let root = fixture.path().join("root");
    git(&root, &["init", "--quiet", "--initial-branch=main"]);

    let run = driver(&fixture).dir(&root).cache().args(["files"]);
    let refused = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--locked"]);

    assert!(run.status.success(), "{}", stderr(&run));
    assert!(stdout(&run).contains("cells.sv"), "{}", stdout(&run));
    assert!(!root.join("astli.lock").exists());
    assert!(
        stderr(&refused).contains("is missing"),
        "{}",
        stderr(&refused)
    );
}

#[test]
fn a_feature_brings_in_an_optional_dependency_and_dev_dependencies_are_the_roots() {
    let fixture = Fixture::new("optional-dependency");
    fixture.file(
        "root/astli.toml",
        "[features]\ndefault = ['asic']\nasic = { dependencies = ['tech'] }\n\
         [dependencies]\ntech = { path = '../tech' }\n\
         [dev-dependencies]\nverif = { path = '../verif' }\n",
    );
    fixture.file("root/top.sv", "module top;\nendmodule\n");
    fixture.file("tech/tech.sv", "module tech;\nendmodule\n");
    fixture.file("verif/verif.sv", "module verif;\nendmodule\n");
    let root = fixture.path().join("root");

    let all = driver(&fixture).dir(&root).cache().args(["files"]);
    let none = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--no-features"]);

    assert!(all.status.success(), "{}", stderr(&all));
    assert!(none.status.success(), "{}", stderr(&none));
    let (all, none) = (stdout(&all), stdout(&none));
    assert!(all.contains("tech.sv") && all.contains("verif.sv"), "{all}");
    assert!(
        !none.contains("tech.sv") && none.contains("verif.sv"),
        "{none}"
    );
}

#[test]
fn after_fetch_a_run_needs_no_upstream() {
    let fixture = Fixture::new("fetch");
    let url = cells_repository(&fixture);
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{url}', version = '1' }}\n"),
    );
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\nendmodule\n",
    );
    let root = fixture.path().join("root");

    let fetched = driver(&fixture).dir(&root).cache().args(["fetch"]);
    // Locked elsewhere at a release this cache never fetched.
    let upstream = fixture.path().join("cells");
    git(
        &upstream,
        &["commit", "--quiet", "--allow-empty", "--message", "two"],
    );
    git(&upstream, &["tag", "v1.1.0"]);
    let newer = git(&upstream, &["rev-parse", "HEAD"]);
    let lock = root.join("astli.lock");
    let fetched_lock = std::fs::read_to_string(&lock).expect("a lock");
    let run = |cache: &str, args: &[&str], env: &[(&str, &str)]| {
        (env.iter())
            .fold(
                driver(&fixture)
                    .dir(&root)
                    .env("ASTLI_CACHE_DIR", fixture.path().join(cache)),
                |driver, (name, value)| driver.env(name, value),
            )
            .args(args)
    };
    let elsewhere = run("elsewhere", &["update"], &[]);
    let newer_lock = std::fs::read_to_string(&lock).expect("a lock");
    std::fs::remove_dir_all(&upstream).expect("upstream removed");
    let uncached = run("cache", &["files", "--locked", "--offline"], &[]);
    let required = format!("[dependencies]\ncells = {{ git = '{url}', version = '1.1' }}\n");
    fixture.file("root/astli.toml", &required);
    let unmet = run("cache", &["files", "--offline"], &[]);
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{url}', version = '1' }}\n"),
    );
    let moved = run("cache", &["files"], &[("ASTLI_OFFLINE", "1")]);
    let moved_lock = std::fs::read_to_string(&lock).expect("a lock");
    let offline = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--locked", "--offline"]);
    let unmirrored = [
        run("empty", &["files", "--locked", "--offline"], &[]),
        run("empty", &["files", "--offline"], &[]),
    ];

    assert!(fetched.status.success(), "{}", stderr(&fetched));
    assert!(
        stderr(&fetched).contains("Fetched 1 package into"),
        "{}",
        stderr(&fetched)
    );
    assert!(offline.status.success(), "{}", stderr(&offline));
    assert!(
        stdout(&offline).contains("cells.sv"),
        "{}",
        stdout(&offline)
    );
    assert!(elsewhere.status.success(), "{}", stderr(&elsewhere));
    assert_ne!(newer_lock, fetched_lock);
    // The run that fails names the commit the cache lacks, and where from.
    let lacked = format!("`cells` is locked at `{newer}` of `{url}`, which is not in the cache");
    assert!(!uncached.status.success());
    let said = stderr(&uncached);
    assert!(
        said.contains("would change") && said.contains(&lacked),
        "{said}"
    );
    assert!(!unmet.status.success());
    let said = stderr(&unmet);
    assert!(
        said.contains("no version") && said.contains(&lacked),
        "{said}"
    );
    // Without `--locked`, it resolves over what the cache holds.
    assert!(moved.status.success(), "{}", stderr(&moved));
    assert!(stderr(&moved).contains(&lacked), "{}", stderr(&moved));
    assert_eq!(moved_lock, fetched_lock);
    let lacked = format!("`cells` comes from `{url}`, which is not in the cache");
    for unmirrored in unmirrored {
        assert!(!unmirrored.status.success());
        let said = stderr(&unmirrored);
        assert!(said.contains(&lacked), "{said}");
    }
}

/// What a commit holds in place of a file stored with Git LFS.
const LFS_POINTER: &str = "version https://git-lfs.github.com/spec/v1\n\
     oid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\n\
     size 6\n";

#[test]
fn a_dependency_asking_for_lfs_files_is_refused_without_git_lfs() {
    let fixture = Fixture::new("lfs-program");
    let url = cells_repository(&fixture);
    let manifest =
        |lfs: &str| format!("[dependencies]\ncells = {{ git = '{url}', version = '1'{lfs} }}\n");
    fixture.file("root/astli.toml", &manifest(""));
    fixture.file("root/top.sv", "module top;\nendmodule\n");
    let root = fixture.path().join("root");
    let fetched = driver(&fixture).dir(&root).cache().args(["fetch"]);
    fixture.file("root/astli.toml", &manifest(", lfs = true"));
    // A `PATH` that finds no program, `git-lfs` among them.
    let nowhere = fixture.path().join("nowhere");
    std::fs::create_dir_all(&nowhere).expect("a directory");

    let refused = driver(&fixture)
        .dir(&root)
        .cache()
        .env("PATH", &nowhere)
        .args(["files", "--offline"]);

    assert!(fetched.status.success(), "{}", stderr(&fetched));
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains(
            "`cells` asks for the files its repository stores with Git LFS, which takes \
             `git-lfs`, and it is not installed"
        ),
        "{}",
        stderr(&refused)
    );
}

#[test]
fn a_git_lfs_pointer_is_refused_saying_what_fetches_it() {
    let fixture = Fixture::new("lfs-pointer");
    fixture.file("cells/cells.sv", LFS_POINTER);
    let url = repository(&fixture, "cells");
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{url}', version = '1' }}\n"),
    );
    let root = fixture.path().join("root");
    git(&root, &["init", "--quiet", "--initial-branch=main"]);

    let dependency = driver(&fixture).dir(&root).cache().args(["files"]);
    fixture.file("root/big.sv", LFS_POINTER);
    let own = driver(&fixture).dir(&root).cache().args(["lint", "big.sv"]);
    // Discovering the package's files reads past one it cannot read, which
    // the check then reports, checking the rest without it. A header is
    // refused as a unit is, by the `include that names it.
    fixture.file("root/big.svh", LFS_POINTER);
    fixture.file(
        "root/top.sv",
        "`include \"big.svh\"\nmodule top;\n  lost u_lost ();\nendmodule\n",
    );
    let checked = driver(&fixture).dir(&root).cache().args(["check"]);

    let pointer = "a Git LFS pointer, not the file it stands for, which was never fetched";
    assert!(!dependency.status.success());
    assert!(
        stderr(&dependency).contains(&format!(
            "cells.sv: {pointer}: to fetch a dependency's, set `lfs = true` on its requirement"
        )),
        "{}",
        stderr(&dependency)
    );
    assert!(!own.status.success());
    assert!(
        stderr(&own).contains(&format!("big.sv: {pointer}: `git lfs pull` fetches it")),
        "{}",
        stderr(&own)
    );
    let said = stderr(&checked);
    assert!(said.contains(&format!("big.sv: {pointer}")), "{said}");
    assert!(said.contains("cannot read `big.svh`"), "{said}");
    assert!(
        said.contains(&format!("{pointer}: `git lfs pull` fetches it")),
        "{said}"
    );
    assert!(said.contains("`lost`"), "{said}");
    assert!(said.contains("files failed, and 1 has errors"), "{said}");
}

#[test]
fn a_path_of_a_dependency_in_a_submodule_not_checked_out_names_it() {
    let fixture = Fixture::new("submodule-path");
    fixture.file("gitconfig", "");
    fixture.file(
        "core/astli.toml",
        "[package]\nname = 'core'\ninclude-dirs = ['vendor/fpu/include']\n",
    );
    fixture.file(
        "core/.gitmodules",
        "[submodule \"fpu\"]\n\tpath = vendor/fpu\n\turl = ../fpu\n",
    );
    let core = fixture.path().join("core");
    git(&core, &["init", "--quiet", "--initial-branch=main"]);
    git(&core, &["add", "--all"]);
    git(&core, &["commit", "--quiet", "--message", "one"]);
    // A gitlink, as `git submodule add` writes one, to any commit.
    let gitlink = format!("160000,{},vendor/fpu", git(&core, &["rev-parse", "HEAD"]));
    git(&core, &["update-index", "--add", "--cacheinfo", &gitlink]);
    git(&core, &["commit", "--quiet", "--message", "two"]);
    git(&core, &["tag", "v1.0.0"]);
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\ncore = {{ git = '{}', version = '1' }}\n",
            core.display()
        ),
    );
    let root = fixture.path().join("root");

    let refused = driver(&fixture).dir(&root).cache().args(["files"]);

    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains(
            "`include-dirs`: `vendor/fpu/include` is in the submodule `vendor/fpu`, which is \
             not checked out: a requirement of the package with `submodules = [\"vendor/fpu\"]` \
             checks it out"
        ),
        "{}",
        stderr(&refused)
    );
}

/// A root under `fixture` depending on a `cells` repository.
fn depends_on_cells(fixture: &Fixture) -> PathBuf {
    let url = cells_repository(fixture);
    fixture.file(
        "root/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{url}', version = '1' }}\n"),
    );
    fixture.file(
        "root/top.sv",
        "module top;\n  cells u_cells ();\nendmodule\n",
    );
    fixture.path().join("root")
}

#[test]
fn path_prints_where_a_dependency_is_checked_out() {
    let fixture = Fixture::new("path");
    let root = depends_on_cells(&fixture);

    let output = driver(&fixture).dir(&root).cache().args(["path", "cells"]);
    let unknown = driver(&fixture).dir(&root).cache().args(["path", "axi"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let dir = PathBuf::from(stdout(&output).trim_end());
    assert!(
        dir.starts_with(fixture.path().join("cache")),
        "{}",
        dir.display()
    );
    assert!(dir.join("cells.sv").is_file(), "{}", dir.display());
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("`axi` is not a dependency"),
        "{}",
        stderr(&unknown)
    );
}

#[test]
fn tree_lists_what_a_shared_dependency_needs_once() {
    let fixture = Fixture::new("tree");
    cells_repository(&fixture);
    let url = |name: &str| fixture.path().join(name).display().to_string();
    for (name, needs) in [("axi", "cells"), ("soc", "axi")] {
        fixture.file(
            &format!("{name}/astli.toml"),
            &format!(
                "[dependencies]\n{needs} = {{ git = '{}', version = '1' }}\n",
                url(needs)
            ),
        );
        let dir = fixture.path().join(name);
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(&dir, &["add", "--all"]);
        git(&dir, &["commit", "--quiet", "--message", "one"]);
        git(&dir, &["tag", "v1.0.0"]);
    }
    fixture.file(
        "root/astli.toml",
        &format!(
            "[package]\nname = 'top'\n[dependencies]\n\
             soc = {{ git = '{}', version = '1' }}\naxi = {{ git = '{}', version = '1' }}\n",
            url("soc"),
            url("axi")
        ),
    );
    let root = fixture.path().join("root");

    let output = driver(&fixture).dir(&root).cache().args(["tree", "-q"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "top\n\
         ├── axi 1.0.0\n\
         │   └── cells 1.0.0\n\
         └── soc 1.0.0\n\
         \x20   └── axi 1.0.0 (*)\n"
    );
}

#[test]
fn a_dependency_without_a_manifest_is_read_as_the_root_describes_it() {
    let fixture = Fixture::new("described");
    fixture.file("gitconfig", "");
    fixture.file("tcells/src/rtl/cell.sv", "module tc_cell;\nendmodule\n");
    fixture.file("tcells/src/fpga/cell.sv", "module tc_cell;\nendmodule\n");
    fixture.file("tcells/src/include/tcells/defs.svh", "`define TC_W 8\n");
    let upstream = fixture.path().join("tcells");
    git(&upstream, &["init", "--quiet", "--initial-branch=main"]);
    git(&upstream, &["add", "--all"]);
    git(&upstream, &["commit", "--quiet", "--message", "one"]);
    git(&upstream, &["tag", "v1.0.0"]);
    let description = "exclude = [\"src/fpga/**\"]\ninclude-dirs = [\"src/include\"]\n";
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\ntcells = {{ git = '{}', version = '1' }}\n\
             [described.tcells]\n{description}[described.ghost]\n",
            upstream.display()
        ),
    );
    fixture.file(
        "root/top.sv",
        "`include \"tcells/defs.svh\"\nmodule top;\n  logic [`TC_W-1:0] w;\n  tc_cell u_cell ();\nendmodule\n",
    );
    let root = fixture.path().join("root");

    let files = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--emit", "files"]);
    let quiet = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--emit", "files", "-q"]);
    let tree = driver(&fixture).dir(&root).cache().args(["tree", "-q"]);
    let check = driver(&fixture).dir(&root).cache().args(["check"]);
    // Upstream takes the description as its own manifest.
    fixture.file("tcells/astli.toml", &format!("[package]\n{description}"));
    git(&upstream, &["add", "--all"]);
    git(&upstream, &["commit", "--quiet", "--message", "two"]);
    git(&upstream, &["tag", "v1.1.0"]);
    let updated = driver(&fixture).dir(&root).cache().args(["update", "-q"]);
    let after = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--emit", "files"]);

    assert!(files.status.success(), "{}", stderr(&files));
    let listed = stdout(&files).replace('\\', "/");
    assert!(
        listed.contains("src/rtl/cell.sv") && !listed.contains("src/fpga"),
        "{listed}"
    );
    let said = stderr(&files);
    assert!(
        said.contains(
            "note: dependency `tcells` has no astli.toml, so astli uses `[described.tcells]` \
             from this one instead\n  \
             = help: the same keys under `[package]` in a dependency's own astli.toml make its \
             description unnecessary\n"
        ),
        "{said}"
    );
    let ghost = "warning: `[described.ghost]` describes a package the design does not hold\n";
    assert!(said.contains(ghost), "{said}");
    // A note is left out under `-q`, and a warning is not.
    let quiet = stderr(&quiet);
    assert!(
        quiet.contains(ghost) && !quiet.contains("has no astli.toml"),
        "{quiet}"
    );
    // Only the commands about dependencies note the description.
    let checked = stderr(&check);
    assert!(
        checked.contains(ghost) && !checked.contains("has no astli.toml"),
        "{checked}"
    );
    assert!(
        stdout(&tree).contains("tcells 1.0.0 (described)"),
        "{}",
        stdout(&tree)
    );
    assert!(updated.status.success(), "{}", stderr(&updated));
    assert!(after.status.success(), "{}", stderr(&after));
    assert!(
        stderr(&after).contains("`tcells` has its own astli.toml, which is read instead"),
        "{}",
        stderr(&after)
    );
    assert!(!stdout(&after).contains("src/fpga"), "{}", stdout(&after));
}

#[test]
fn a_cache_dir_named_through_dot_dot_reads_its_checkouts() {
    let fixture = Fixture::new("cache-dot-dot");
    fixture.file("gitconfig", "");
    fixture.file("cells/cells.sv", "module cells;\nendmodule\n");
    fixture.file("cells/fpga/cells.sv", "module cells;\nendmodule\n");
    let upstream = fixture.path().join("cells");
    git(&upstream, &["init", "--quiet", "--initial-branch=main"]);
    git(&upstream, &["add", "--all"]);
    git(&upstream, &["commit", "--quiet", "--message", "one"]);
    git(&upstream, &["tag", "v1.0.0"]);
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\ncells = {{ git = '{}', version = '1' }}\n\
             [described.cells]\nexclude = [\"fpga\"]\n",
            upstream.display()
        ),
    );
    let root = fixture.path().join("root");
    let files = |cache: &Path| {
        driver(&fixture)
            .dir(&root)
            .env("ASTLI_CACHE_DIR", cache)
            .args(["files", "--emit", "files", "-q"])
    };

    // Checked out the first time, and read from the store the second, the
    // second time through a relative path.
    let cold = files(&root.join("../cache"));
    let warm = files(Path::new("../cache"));

    for output in [cold, warm] {
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(stderr(&output), "");
        let listed = stdout(&output).replace('\\', "/");
        assert!(
            listed.contains("/cells.sv") && !listed.contains("fpga"),
            "{listed}"
        );
    }
}

#[test]
fn init_names_the_package_after_its_directory_and_writes_once() {
    let fixture = Fixture::new("init");
    fixture.file("ip/ip.sv", "module ip;\nendmodule\n");
    let dir = fixture.path().join("ip");

    let first = driver(&fixture).dir(&dir).args(["init"]);
    let again = driver(&fixture).dir(&dir).args(["init"]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert_eq!(stderr(&first), "     Created astli.toml for `ip`\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("astli.toml")).expect("a manifest"),
        "[package]\nname = \"ip\"\n"
    );
    assert!(!again.status.success());
    assert!(
        stderr(&again).contains("already exists"),
        "{}",
        stderr(&again)
    );
}

#[test]
fn a_file_put_into_a_checkout_by_hand_is_refused() {
    let fixture = Fixture::new("stray");
    let root = depends_on_cells(&fixture);
    let path = driver(&fixture).dir(&root).cache().args(["path", "cells"]);
    let dir = PathBuf::from(stdout(&path).trim_end());

    std::fs::write(dir.join("generated.sv"), "module generated;\nendmodule\n")
        .expect("a directory of a checkout stays writable");
    let output = driver(&fixture).dir(&root).cache().args(["files"]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("generated.sv: not a file of the commit"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn cache_clean_deletes_the_checkouts_and_a_run_fetches_again() {
    let fixture = Fixture::new("cache-clean");
    let root = depends_on_cells(&fixture);
    let cache = fixture.path().join("cache");

    let dir = driver(&fixture).dir(&root).cache().args(["cache", "dir"]);
    let fetched = driver(&fixture).dir(&root).cache().args(["fetch"]);
    let cleaned = driver(&fixture).dir(&root).cache().args(["cache", "clean"]);
    let gone = !cache.exists();
    let again = driver(&fixture).dir(&root).cache().args(["cache", "clean"]);
    let refetched = driver(&fixture).dir(&root).cache().args(["files"]);

    assert_eq!(stdout(&dir).trim_end(), cache.display().to_string());
    assert!(fetched.status.success(), "{}", stderr(&fetched));
    assert!(cleaned.status.success(), "{}", stderr(&cleaned));
    assert!(gone, "the cache is still there");
    assert!(
        stderr(&again).starts_with("     Removed nothing, since "),
        "{}",
        stderr(&again)
    );
    assert!(refetched.status.success(), "{}", stderr(&refetched));
    assert!(
        stdout(&refetched).contains("cells.sv"),
        "{}",
        stdout(&refetched)
    );
}

#[test]
fn a_patch_reads_a_dependency_from_a_clone_and_leaves_the_lock_alone() {
    let fixture = Fixture::new("patch");
    let root = depends_on_cells(&fixture);
    // A repository a hook runs the driver from, which neither the mirror
    // nor the clone may write to.
    let hook = fixture.path().join("hook");
    std::fs::create_dir_all(&hook).expect("a directory");
    git(&hook, &["init", "--quiet", "--initial-branch=main"]);
    git(
        &hook,
        &["remote", "add", "origin", "https://example.com/hook"],
    );
    git(
        &hook,
        &["commit", "--quiet", "--allow-empty", "--message", "one"],
    );
    let hooked = || {
        let config = std::fs::read_to_string(hook.join(".git/config")).expect("a config");
        (
            config,
            git(&hook, &["for-each-ref"]),
            git(&hook, &["status", "--porcelain"]),
        )
    };
    let before = hooked();
    let first = (driver(&fixture).dir(&root).cache())
        .hook_in(&hook)
        .args(["files"]);
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");

    // Configuration the environment carries reaches the `git` it starts,
    // here a template its clone copies.
    let template = fixture.path().join("template");
    fixture.file("template/from-the-environment", "");
    let patched = (driver(&fixture).dir(&root).cache())
        .hook_in(&hook)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "init.templateDir")
        .env("GIT_CONFIG_VALUE_0", &template)
        .args(["patch", "cells"]);
    let clone = root.join(".astli/patch/cells");
    std::fs::write(clone.join("fifo.sv"), "module fifo;\nendmodule\n").expect("a clone to edit");
    let run = driver(&fixture).dir(&root).cache().args(["files"]);
    let locked = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--locked"]);
    let update = driver(&fixture).dir(&root).cache().args(["update"]);
    let removed = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["patch", "--remove", "cells"]);
    let after = driver(&fixture).dir(&root).cache().args(["files"]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert!(patched.status.success(), "{}", stderr(&patched));
    assert!(
        stderr(&patched).contains("Cloning cells at"),
        "{}",
        stderr(&patched)
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".astli/.gitignore")).expect("an ignore file"),
        "*\n"
    );
    assert!(clone.join(".git/from-the-environment").is_file());
    assert_eq!(
        git(&clone, &["remote", "get-url", "origin"]),
        fixture.path().join("cells").display().to_string()
    );
    assert!(run.status.success(), "{}", stderr(&run));
    // Once, as the patched dependency's, not again as the root's own.
    assert_eq!(
        stdout(&run).matches("fifo.sv").count(),
        1,
        "{}",
        stdout(&run)
    );
    assert!(
        stderr(&run).contains("`cells` is patched: read from"),
        "{}",
        stderr(&run)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("astli.lock")).expect("a lock"),
        lock
    );
    assert!(!locked.status.success());
    assert!(
        stderr(&locked).contains("`--locked` needs a design without patches"),
        "{}",
        stderr(&locked)
    );
    assert!(!update.status.success());
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(clone.join("fifo.sv").is_file());
    assert!(after.status.success(), "{}", stderr(&after));
    assert!(!stdout(&after).contains("fifo.sv"), "{}", stdout(&after));
    assert_eq!(hooked(), before);
}

#[test]
fn a_patch_points_at_a_directory_given() {
    let fixture = Fixture::new("patch-dir");
    let root = depends_on_cells(&fixture);
    fixture.file(
        "work/cells/astli.toml",
        "[dependencies]\nvip = { path = 'vip' }\n",
    );
    fixture.file(
        "work/cells/cells.sv",
        "module cells;\n  gone u ();\nendmodule\n",
    );
    fixture.file("work/cells/cdc.sv", "module cdc;\nendmodule\n");
    fixture.file(
        "work/cells/vip/vip.sv",
        "module vip;\n  gone u ();\nendmodule\n",
    );

    let patched = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["clone", "cells", "../work/cells"]);
    let run = driver(&fixture).dir(&root).cache().args(["files"]);
    let check = driver(&fixture).dir(&root).cache().args(["check"]);
    let unknown = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["patch", "axi", "../work/cells"]);

    assert!(patched.status.success(), "{}", stderr(&patched));
    assert!(stdout(&run).contains("cdc.sv"), "{}", stdout(&run));
    // The patched package's findings are its authors', and so are those of
    // a directory its own manifest names.
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("`axi` is not a dependency"),
        "{}",
        stderr(&unknown)
    );
}

#[test]
fn a_snapshot_requires_the_pushed_commit_of_a_clone() {
    let fixture = Fixture::new("snapshot");
    let cells = cells_repository(&fixture);
    let axi = fixture.path().join("axi");
    fixture.file(
        "axi/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{cells}', version = '1' }}\n"),
    );
    git(&axi, &["init", "--quiet", "--initial-branch=main"]);
    git(&axi, &["add", "--all"]);
    git(&axi, &["commit", "--quiet", "--message", "one"]);
    git(&axi, &["tag", "v1.0.0"]);
    fixture.file(
        "root/astli.toml",
        &format!(
            "# The design.\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n\
             cells = {{ git = '{cells}', version = '1' }}\n",
            axi.display()
        ),
    );
    fixture.file("root/top.sv", "module top;\nendmodule\n");
    let root = fixture.path().join("root");
    let clone = root.join(".astli/patch/cells");
    let first = driver(&fixture).dir(&root).cache().args(["patch", "cells"]);

    fixture.file(
        "root/.astli/patch/cells/fifo.sv",
        "module fifo;\nendmodule\n",
    );
    let dirty = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["snapshot", "cells"]);
    git(&clone, &["add", "--all"]);
    git(&clone, &["commit", "--quiet", "--message", "fifo"]);
    let unpushed = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["snapshot", "cells"]);
    git(
        &clone,
        &["push", "--quiet", "origin", "HEAD:refs/heads/fifo"],
    );
    // Resolving it then fails, so the manifest and the patches are put back.
    let written = |name: &str| std::fs::read_to_string(root.join(name)).expect("a file");
    let before = ["astli.toml", "astli.lock", ".astli/patch.toml"].map(written);
    let lock = root.join("astli.lock");
    let readonly = |yes: bool| {
        let mut permissions = std::fs::metadata(&lock).expect("a lock").permissions();
        permissions.set_readonly(yes);
        std::fs::set_permissions(&lock, permissions).expect("permissions to change");
    };
    readonly(true);
    let unwritable = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["snapshot", "cells"]);
    readonly(false);
    let kept = ["astli.toml", "astli.lock", ".astli/patch.toml"].map(written);
    let snapshot = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["snapshot", "cells"]);
    let after = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["files", "--locked"]);

    assert!(first.status.success(), "{}", stderr(&first));
    assert!(
        stderr(&dirty).contains("has changes not committed"),
        "{}",
        stderr(&dirty)
    );
    assert!(
        stderr(&unpushed).contains("push it first"),
        "{}",
        stderr(&unpushed)
    );
    assert!(!unwritable.status.success());
    assert_eq!(kept, before);
    assert!(snapshot.status.success(), "{}", stderr(&snapshot));
    let commit = git(&clone, &["rev-parse", "HEAD"]);
    let manifest = std::fs::read_to_string(root.join("astli.toml")).expect("a manifest");
    assert_eq!(
        manifest,
        format!(
            "# The design.\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n\
             cells = {{ git = '{cells}', rev = \"{commit}\" }}\n\n[resolve]\npinned = [\"cells\"]\n",
            axi.display()
        )
    );
    assert!(
        stderr(&snapshot).contains("`axi` depends on `cells` `^1` of"),
        "{}",
        stderr(&snapshot)
    );
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");
    assert!(lock.contains(&commit), "{lock}");
    assert_eq!(
        std::fs::read_to_string(root.join(".astli/patch.toml")).expect("the patches"),
        "# What this machine reads each dependency named from, written by `astli patch`.\n\
         [patch]\n"
    );
    assert!(after.status.success(), "{}", stderr(&after));
    assert!(stdout(&after).contains("fifo.sv"), "{}", stdout(&after));
    assert!(clone.join("fifo.sv").is_file());
}

/// Commits a file in the clone of `cells` under `ws/.astli/patch/cells`
/// and pushes it, returning the commit.
fn commit_in_clone(fixture: &Fixture, ws: &Path) -> String {
    let clone = ws.join(".astli/patch/cells");
    fixture.file("ws/.astli/patch/cells/fifo.sv", "module fifo;\nendmodule\n");
    git(&clone, &["add", "--all"]);
    git(&clone, &["commit", "--quiet", "--message", "fifo"]);
    git(
        &clone,
        &["push", "--quiet", "origin", "HEAD:refs/heads/fifo"],
    );
    git(&clone, &["rev-parse", "HEAD"])
}

#[test]
fn a_snapshot_in_a_workspace_requires_the_commit_in_each_member_and_pins_at_its_root() {
    let fixture = Fixture::new("snapshot-workspace");
    let ws = workspace(&fixture);
    let cells = fixture.path().join("cells").display().to_string();
    let axi = axi_requiring_cells(&fixture, &cells);
    let other = format!(
        "[package]\nname = 'other'\n\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n\
         cells = {{ git = '{cells}', version = '1' }}\n",
        axi.display()
    );
    fixture.file("ws/ip/other/astli.toml", &other);
    fixture.file(
        "ws/ip/legacy/Bender.yml",
        &format!("package:\n  name: legacy\ndependencies:\n  cells: {{ git: '{cells}', version: 1.0.0 }}\n"),
    );
    fixture.file("ws/ip/legacy/astli.toml", "[package]\nname = 'legacy'\n");
    // Without a `[dependencies]` table or a `Bender.yml`, it has none.
    fixture.file(
        "ws/ip/dev/astli.toml",
        &format!("[package]\nname = 'dev'\n\n[dev-dependencies]\ncells = {{ git = '{cells}', version = '1' }}\n"),
    );
    let member = ws.join("ip/other");
    let patched = driver(&fixture)
        .dir(&member)
        .cache()
        .args(["patch", "cells"]);
    let commit = commit_in_clone(&fixture, &ws);

    let bender = driver(&fixture)
        .dir(&member)
        .cache()
        .args(["snapshot", "cells"]);
    let other_after_bender =
        std::fs::read_to_string(member.join("astli.toml")).expect("a manifest");
    std::fs::remove_dir_all(ws.join("ip/legacy")).expect("removed");
    let snapshot = driver(&fixture)
        .dir(&member)
        .cache()
        .args(["snapshot", "cells"]);
    let after = driver(&fixture)
        .dir(&ws)
        .cache()
        .args(["files", "--locked"]);

    assert!(patched.status.success(), "{}", stderr(&patched));
    assert!(!bender.status.success());
    assert_eq!(
        stderr(&bender),
        format!(
            "{}  = help: or require `cells` at `{commit}` in the Bender.yml by hand, and drop the \
             patch with `astli patch --remove cells`\n",
            FROM_BENDER.replace("{who}", "`legacy`")
        )
    );
    assert_eq!(other_after_bender, other);
    assert!(snapshot.status.success(), "{}", stderr(&snapshot));
    assert_eq!(
        std::fs::read_to_string(member.join("astli.toml")).expect("a manifest"),
        format!(
            "[package]\nname = 'other'\n\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n\
             cells = {{ git = '{cells}', rev = \"{commit}\" }}\n",
            axi.display()
        )
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("ip/dma/astli.toml")).expect("a manifest"),
        format!(
            "[package]\nname = 'dma'\n\n[dependencies]\ncells = {{ git = '{cells}', rev = \"{commit}\" }}\n"
        )
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("ip/dev/astli.toml")).expect("a manifest"),
        format!(
            "[package]\nname = 'dev'\n\n[dev-dependencies]\ncells = {{ git = '{cells}', rev = \"{commit}\" }}\n"
        )
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("astli.toml")).expect("a manifest"),
        "[package]\nname = 'soc'\n\n[workspace]\n\n[dependencies]\ndma = { path = 'ip/dma' }\n\n\
         [resolve]\npinned = [\"cells\"]\n"
    );
    let said = said_in(&fixture, &snapshot);
    assert!(
        said.contains("in ip/dma/astli.toml")
            && said.contains("in ip/dev/astli.toml")
            && said.contains("in ip/other/astli.toml"),
        "{said}"
    );
    let lock = std::fs::read_to_string(ws.join("astli.lock")).expect("a lock");
    assert!(lock.contains(&commit), "{lock}");
    assert!(after.status.success(), "{}", stderr(&after));
    assert!(stdout(&after).contains("fifo.sv"), "{}", stdout(&after));
}

#[test]
fn a_snapshot_of_what_only_a_dependency_requires_goes_to_the_member_whose_design_holds_it() {
    let fixture = Fixture::new("snapshot-held");
    let ws = workspace(&fixture);
    let cells = fixture.path().join("cells").display().to_string();
    let axi = axi_requiring_cells(&fixture, &cells);
    let dma = format!(
        "[package]\nname = 'dma'\n\n[dependencies]\naxi = {{ git = '{}', version = '1' }}\n",
        axi.display()
    );
    fixture.file("ws/ip/dma/astli.toml", &dma);
    let patched = driver(&fixture).dir(&ws).cache().args(["patch", "cells"]);
    let commit = commit_in_clone(&fixture, &ws);

    let elsewhere = driver(&fixture)
        .dir(ws.join("ip/other"))
        .cache()
        .args(["snapshot", "cells"]);
    let snapshot = driver(&fixture)
        .dir(ws.join("ip/dma"))
        .cache()
        .args(["snapshot", "cells"]);

    assert!(patched.status.success(), "{}", stderr(&patched));
    assert!(!elsewhere.status.success());
    assert!(
        stderr(&elsewhere).contains(
            "no package of the workspace requires `cells`, and the design of `other` does not \
             hold it"
        ),
        "{}",
        stderr(&elsewhere)
    );
    assert!(snapshot.status.success(), "{}", stderr(&snapshot));
    assert_eq!(
        std::fs::read_to_string(ws.join("ip/dma/astli.toml")).expect("a manifest"),
        format!(
            "{dma}cells = {{ git = {}, rev = \"{commit}\" }}\n",
            quoted(&cells)
        )
    );
    assert!(
        std::fs::read_to_string(ws.join("astli.toml"))
            .expect("a manifest")
            .ends_with("[resolve]\npinned = [\"cells\"]\n")
    );
}

/// `text` as a TOML string, as astli writes it: a literal one where it
/// holds a backslash, as a Windows path does.
fn quoted(text: &str) -> String {
    toml_edit::Value::from(text).to_string()
}

#[test]
fn add_requires_the_highest_version_a_repository_tags_and_locks_it() {
    let fixture = Fixture::new("add");
    let url = cells_repository(&fixture);
    let cells = fixture.path().join("cells");
    git(
        &cells,
        &["commit", "--quiet", "--allow-empty", "--message", "two"],
    );
    git(&cells, &["tag", "v1.1.0"]);
    git(
        &cells,
        &["commit", "--quiet", "--allow-empty", "--message", "three"],
    );
    git(&cells, &["tag", "v2.0.0-rc.1"]);
    fixture.file("untagged/.keep", "");
    let untagged = fixture.path().join("untagged");
    git(&untagged, &["init", "--quiet", "--initial-branch=main"]);
    git(
        &untagged,
        &["commit", "--quiet", "--allow-empty", "--message", "one"],
    );
    fixture.file(
        "root/astli.toml",
        "[package]\nname = \"root\"\n\n[dependencies]\naxi = { path = \"../axi\" }\n",
    );
    fixture.file("axi/axi.sv", "module axi;\nendmodule\n");
    let root = fixture.path().join("root");

    let added = driver(&fixture).dir(&root).cache().args(["add", &url]);
    let manifest = std::fs::read_to_string(root.join("astli.toml")).expect("a manifest");
    let refused = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["add", &untagged.display().to_string()]);
    let tracked = driver(&fixture).dir(&root).cache().args([
        "add",
        &untagged.display().to_string(),
        "--rev",
        "main",
        "--name",
        "other",
    ]);
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");

    assert!(added.status.success(), "{}", stderr(&added));
    assert_eq!(
        said_in(&fixture, &added),
        "    Fetching cells (<dir>/cells)\n      Adding cells 1.1.0\n    \
         Required cells 1.1.0 in [dependencies]\n"
    );
    assert_eq!(
        manifest,
        format!(
            "[package]\nname = \"root\"\n\n[dependencies]\naxi = {{ path = \"../axi\" }}\n\
             cells = {{ git = {}, version = \"1.1.0\" }}\n",
            quoted(&url)
        )
    );
    assert!(!refused.status.success());
    assert!(
        said_in(&fixture, &refused).contains("`<dir>/untagged` tags no version `vX.Y.Z`"),
        "{}",
        said_in(&fixture, &refused)
    );
    assert!(tracked.status.success(), "{}", stderr(&tracked));
    assert!(
        lock.contains("name = \"other\"") && lock.contains("revs = [\"main\"]"),
        "{lock}"
    );
}

#[test]
fn add_names_a_repository_as_its_manifest_does() {
    let fixture = Fixture::new("add-named");
    fixture.file("gitconfig", "");
    fixture.file("repo/Bender.yml", "package:\n  name: Common_Cells\n");
    let repo = fixture.path().join("repo");
    git(&repo, &["init", "--quiet", "--initial-branch=main"]);
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "--message", "one"]);
    git(&repo, &["tag", "v0.3.1"]);
    fixture.file("root/astli.toml", "[dependencies]\n");
    let root = fixture.path().join("root");

    let added = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["add", &repo.display().to_string()]);

    assert!(added.status.success(), "{}", stderr(&added));
    assert_eq!(
        std::fs::read_to_string(root.join("astli.toml")).expect("a manifest"),
        format!(
            "[dependencies]\ncommon_cells = {{ git = {}, version = \"0.3.1\" }}\n",
            quoted(&repo.display().to_string())
        )
    );
}

#[test]
fn add_changes_an_entry_in_place_and_requires_a_package_the_design_holds() {
    let fixture = Fixture::new("add-held");
    let cells = cells_repository(&fixture);
    let axi = fixture.path().join("axi");
    fixture.file(
        "axi/astli.toml",
        &format!("[dependencies]\ncells = {{ git = '{cells}', version = '1' }}\n"),
    );
    git(&axi, &["init", "--quiet", "--initial-branch=main"]);
    git(&axi, &["add", "--all"]);
    git(&axi, &["commit", "--quiet", "--message", "one"]);
    git(&axi, &["tag", "v1.0.0"]);
    fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\naxi = {{ git = '{}', version = '1', default-features = false }}\n",
            axi.display()
        ),
    );
    let root = fixture.path().join("root");

    let changed = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["add", "axi", "--version", "1.0"]);
    let held = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["add", "cells", "--dev"]);
    let unknown = driver(&fixture).dir(&root).cache().args(["add", "apb"]);
    let twice = driver(&fixture).dir(&root).cache().args(["add", "cells"]);

    assert!(changed.status.success(), "{}", stderr(&changed));
    assert!(held.status.success(), "{}", stderr(&held));
    assert_eq!(
        std::fs::read_to_string(root.join("astli.toml")).expect("a manifest"),
        format!(
            "[dependencies]\naxi = {{ git = '{}', version = \"1.0\", default-features = false }}\n\n\
             [dev-dependencies]\ncells = {{ git = {}, version = \"1.0.0\" }}\n",
            axi.display(),
            quoted(&cells)
        )
    );
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("`apb` is not in the design"),
        "{}",
        stderr(&unknown)
    );
    assert!(!twice.status.success());
    assert!(
        stderr(&twice).contains("`cells` is already in `[dev-dependencies]`"),
        "{}",
        stderr(&twice)
    );
}

#[test]
fn add_requires_a_directory_by_its_path_from_the_manifest() {
    let fixture = Fixture::new("add-path");
    fixture.file("tech/tech.sv", "module tech;\nendmodule\n");
    fixture.file("tech/astli.toml", "[features]\nasic = {}\n");
    fixture.file(
        "root/astli.toml",
        "[dependencies]\nb = { path = \"../b\" }\n",
    );
    fixture.file("b/b.sv", "module b;\nendmodule\n");
    fixture.file("root/hw/top.sv", "module top;\nendmodule\n");
    let hw = fixture.path().join("root/hw");

    let added =
        driver(&fixture)
            .dir(&hw)
            .args(["add", "--path", "../../tech", "--features", "asic"]);
    let missing = driver(&fixture)
        .dir(&hw)
        .args(["add", "--path", "../nowhere"]);
    let versioned =
        driver(&fixture)
            .dir(&hw)
            .args(["add", "--path", "../../tech", "--rev", "main"]);
    // Resolving it then fails, so the manifest is put back.
    fixture.file(
        "broken/astli.toml",
        "[dependencies]\nnope = { path = \"nope\" }\n",
    );
    let unresolved = driver(&fixture)
        .dir(&hw)
        .args(["add", "--path", "../../broken"]);

    assert!(added.status.success(), "{}", stderr(&added));
    assert_eq!(
        said_in(&fixture, &added),
        "    Required tech from ../tech in [dependencies]\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.path().join("root/astli.toml")).expect("a manifest"),
        "[dependencies]\nb = { path = \"../b\" }\ntech = { path = \"../tech\", features = [\"asic\"] }\n"
    );
    assert!(!missing.status.success());
    assert!(
        stderr(&missing).contains("is not a directory"),
        "{}",
        stderr(&missing)
    );
    assert!(!versioned.status.success());
    assert!(!unresolved.status.success());
    assert!(
        stderr(&unresolved).contains("`nope` is not a directory"),
        "{}",
        stderr(&unresolved)
    );
    assert_eq!(
        std::fs::read_to_string(fixture.path().join("root/astli.toml")).expect("a manifest"),
        "[dependencies]\nb = { path = \"../b\" }\ntech = { path = \"../tech\", features = [\"asic\"] }\n"
    );
}

#[test]
fn add_and_remove_refuse_a_root_whose_dependencies_come_from_its_bender_yml() {
    let fixture = Fixture::new("add-bender");
    fixture.file("tech/tech.sv", "module tech;\nendmodule\n");
    fixture.file(
        "root/Bender.yml",
        "package:\n  name: root\ndependencies:\n  tech: { path: ../tech }\n",
    );
    fixture.file("root/astli.toml", "[fmt]\n");
    let root = fixture.path().join("root");

    let added = driver(&fixture)
        .dir(&root)
        .args(["add", "--path", "../tech", "--name", "other"]);
    let removed = driver(&fixture).dir(&root).args(["remove", "tech"]);

    for output in [&added, &removed] {
        assert!(!output.status.success());
        assert_eq!(stderr(output), FROM_BENDER.replace("{who}", "the root"));
    }
    assert_eq!(
        std::fs::read_to_string(root.join("astli.toml")).expect("a manifest"),
        "[fmt]\n"
    );
}

#[test]
fn remove_drops_a_dependency_and_what_mentions_it() {
    let fixture = Fixture::new("remove");
    let root = depends_on_cells(&fixture);
    fixture.file("tech/tech.sv", "module tech;\nendmodule\n");
    fixture.file("tech/astli.toml", "[features]\nasic = {}\n");
    fixture.file("tb/tb.sv", "module tb;\nendmodule\n");
    fixture.file("sim/sim.sv", "module sim;\nendmodule\n");
    let manifest = std::fs::read_to_string(root.join("astli.toml")).expect("a manifest");
    fixture.file(
        "root/astli.toml",
        &format!(
            "{manifest}tb = {{ path = \"../tb\" }}\ntech = {{ path = \"../tech\" }}\n\n\
             [dev-dependencies]\nsim = {{ path = \"../sim\" }}\n\n\
             [features]\nvip = {{ dependencies = [\"tech/asic\", \"cells\", \"tb\"] }}\n\n\
             [resolve]\npinned = [\"cells\"]\n"
        ),
    );
    let locked = driver(&fixture).dir(&root).cache().args(["update"]);
    let before = std::fs::read_to_string(root.join("astli.toml")).expect("a manifest");
    let lock_before = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");
    let refused = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["remove", "cells", "--locked"]);
    let kept = std::fs::read_to_string(root.join("astli.toml")).expect("a manifest");
    let lock_kept = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");

    let removed = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["remove", "cells", "tech"]);
    let after = std::fs::read_to_string(root.join("astli.toml")).expect("a manifest");
    let lock = std::fs::read_to_string(root.join("astli.lock")).expect("a lock");
    let dev = driver(&fixture).dir(&root).cache().args(["remove", "sim"]);
    let unknown = driver(&fixture)
        .dir(&root)
        .cache()
        .args(["remove", "--dev", "apb"]);

    assert!(locked.status.success(), "{}", stderr(&locked));
    // Resolving it then fails, so the manifest is put back.
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("would change, and `--locked` keeps it as it is"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(kept, before);
    assert_eq!(lock_kept, lock_before);
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert_eq!(
        said_in(&fixture, &removed),
        "    Removing cells 1.0.0\n     Removed cells from [dependencies]\n     \
         Removed tech from [dependencies]\n"
    );
    assert_eq!(
        after,
        "[dependencies]\ntb = { path = \"../tb\" }\n\n[dev-dependencies]\nsim = { path = \"../sim\" }\n\n\
         [features]\nvip = { dependencies = [\"tb\"] }\n\n[resolve]\npinned = []\n"
    );
    assert!(!lock.contains("cells"), "{lock}");
    assert!(!dev.status.success());
    assert!(
        stderr(&dev).contains("`astli remove --dev sim` removes it"),
        "{}",
        stderr(&dev)
    );
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("`apb` is not in `[dev-dependencies]`"),
        "{}",
        stderr(&unknown)
    );
}
