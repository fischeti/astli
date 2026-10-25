//! Resolving designs whose dependencies are repositories the tests make.

mod common;

use std::collections::BTreeSet;

use astli_package::Package;
use astli_resolve::{Lock, Origin, Resolution, resolve};
use common::{Fixture, Repo};

/// `cells`, released as 1.0.0, 1.1.0 and 2.0.0, and `axi` 0.2.0, which
/// requires `cells` as `axi_cells` says, from `cells_url` or `cells`'s own.
fn upstream(fixture: &Fixture, axi_cells: &str) -> (Repo, Repo) {
    let cells = fixture.repo("cells");
    cells.release("v1.0.0", &[("cells.sv", "module cells; endmodule\n")]);
    cells.release("v1.1.0", &[("fifo.sv", "module fifo; endmodule\n")]);
    cells.release("v2.0.0", &[("cdc.sv", "module cdc; endmodule\n")]);
    let axi = fixture.repo("axi");
    let manifest = format!(
        "[dependencies]\ncells = {{ git = '{}', {axi_cells} }}\n",
        cells.url()
    );
    axi.release("v0.2.0", &[("astli.toml", &manifest), ("axi.sv", "")]);
    (cells, axi)
}

/// The root at `root/` under `fixture`, whose `[dependencies]` are
/// `entries`, resolved with `pinned`.
fn resolved(fixture: &Fixture, entries: &str, pinned: &[&str]) -> Result<Resolution, String> {
    locked(fixture, entries, pinned, &Lock::default())
}

/// As [`resolved`], preferring what `lock` holds.
fn locked(
    fixture: &Fixture,
    entries: &str,
    pinned: &[&str],
    lock: &Lock,
) -> Result<Resolution, String> {
    let manifest = fixture.file("root/astli.toml", &format!("[dependencies]\n{entries}"));
    let root = Package::read(&manifest).expect("a valid manifest");
    let pinned: BTreeSet<String> = pinned.iter().map(|it| it.to_string()).collect();
    resolve(&fixture.store(), &root, &pinned, lock).map_err(|err| err.to_string())
}

/// Each package and the version or commit chosen for it.
fn chosen(resolution: &Resolution) -> Vec<String> {
    (resolution.packages.iter())
        .map(|it| match (&it.version, &it.commit) {
            (Some(version), _) => format!("{} {version}", it.name),
            (None, Some(commit)) => format!("{} {}", it.name, &commit.to_string()[..8]),
            (None, None) => format!("{} on disk", it.name),
        })
        .collect()
}

#[test]
fn the_highest_version_every_requirement_admits_is_chosen() {
    let fixture = Fixture::new("highest");
    let (cells, axi) = upstream(&fixture, "version = '1.0'");
    let entries = format!(
        "axi = {{ git = '{}', version = '0.2' }}\ncells = {{ git = '{}', version = '1' }}\n",
        axi.url(),
        cells.url()
    );

    let resolution = resolved(&fixture, &entries, &[]).expect("resolved");

    assert_eq!(chosen(&resolution), ["axi 0.2.0", "cells 1.1.0"]);
    let cells = resolution.packages[1]
        .checkout(&fixture.store())
        .expect("checked out");
    assert!(cells.dir.join("fifo.sv").is_file() && !cells.dir.join("cdc.sv").exists());
    assert_eq!(resolution.packages[0].needs, ["cells"]);
    assert!(resolution.warnings.is_empty());
}

#[test]
fn requirements_that_cannot_all_hold_are_explained() {
    let fixture = Fixture::new("conflict");
    let (cells, axi) = upstream(&fixture, "version = '1.0'");
    let entries = format!(
        "axi = {{ git = '{}', version = '0.2' }}\ncells = {{ git = '{}', version = '2' }}\n",
        axi.url(),
        cells.url()
    );

    let err = resolved(&fixture, &entries, &[]).expect_err("no solution");

    assert!(err.contains("`axi`") && err.contains("`cells`"), "{err}");
    assert!(err.contains("[resolve] pinned"), "{err}");
}

#[test]
fn a_pin_lets_the_root_decide_and_says_what_it_replaced() {
    let fixture = Fixture::new("pinned");
    let (cells, axi) = upstream(&fixture, "version = '1.0'");
    let entries = format!(
        "axi = {{ git = '{}', version = '0.2' }}\ncells = {{ git = '{}', version = '2' }}\n",
        axi.url(),
        cells.url()
    );

    let resolution = resolved(&fixture, &entries, &["cells"]).expect("resolved");

    assert_eq!(chosen(&resolution), ["axi 0.2.0", "cells 2.0.0"]);
    let [warning] = &resolution.warnings[..] else {
        panic!("one warning: {:?}", resolution.warnings);
    };
    assert!(
        warning.starts_with("`axi` depends on `cells` ^1.0 of") && warning.contains("2.0.0"),
        "{warning}"
    );
}

#[test]
fn a_rev_meets_a_version_only_where_a_tag_says_so() {
    let fixture = Fixture::new("rev");
    let (cells, axi) = upstream(&fixture, "version = '1.0'");
    let tagged = cells.git(&["rev-parse", "v1.1.0"]);
    let untagged = cells.commit(&[("later.sv", "")]);
    let entries = |rev: &str| {
        format!(
            "axi = {{ git = '{}', version = '0.2' }}\ncells = {{ git = '{}', rev = '{rev}' }}\n",
            axi.url(),
            cells.url()
        )
    };

    let agreed = resolved(&fixture, &entries(&tagged), &[]).expect("resolved");
    let refused = resolved(&fixture, &entries(&untagged), &[]);

    assert_eq!(chosen(&agreed), ["axi 0.2.0", "cells 1.1.0"]);
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_fork_is_another_package_unless_the_root_pins_it() {
    let fixture = Fixture::new("fork");
    let fork = fixture.repo("fork");
    fork.release("v1.0.0", &[("cells.sv", "")]);
    let (cells, axi) = upstream(&fixture, "version = '1.0'");
    let entries = format!(
        "axi = {{ git = '{}', version = '0.2' }}\ncells = {{ git = '{}', version = '1' }}\n",
        axi.url(),
        fork.url()
    );

    let refused = resolved(&fixture, &entries, &[]).expect_err("two sources");
    let pinned = resolved(&fixture, &entries, &["cells"]).expect("resolved");

    assert!(refused.contains("two packages of one name"), "{refused}");
    let cells_of = &pinned.packages[1];
    assert!(matches!(&cells_of.origin, Origin::Git { url } if *url == fork.url()));
    assert_eq!(pinned.warnings.len(), 1, "{:?}", pinned.warnings);
    assert!(
        pinned.warnings[0].contains(&cells.url()),
        "{:?}",
        pinned.warnings
    );
}

#[test]
fn a_path_in_a_repository_is_a_directory_of_it_at_the_same_commit() {
    let fixture = Fixture::new("within");
    let soc = fixture.repo("soc");
    let commit = soc.release(
        "v1.0.0",
        &[
            ("astli.toml", "[dependencies]\npd = { path = 'hw/../pd' }\n"),
            ("pd/astli.toml", ""),
            ("pd/pd.sv", "module pd; endmodule\n"),
        ],
    );
    let outside = fixture.repo("outside");
    outside.release(
        "v1.0.0",
        &[(
            "astli.toml",
            "[dependencies]\nother = { path = '../other' }\n",
        )],
    );

    let resolution = resolved(
        &fixture,
        &format!("soc = {{ git = '{}', version = '1' }}\n", soc.url()),
        &[],
    )
    .expect("resolved");
    let refused = resolved(
        &fixture,
        &format!("outside = {{ git = '{}', version = '1' }}\n", outside.url()),
        &[],
    )
    .expect_err("refused");

    let pd = &resolution.packages[0];
    assert_eq!(pd.name, "pd");
    assert_eq!(pd.commit.map(|it| it.to_string()), Some(commit));
    let at = pd.checkout(&fixture.store()).expect("checked out");
    assert!(at.dir.join("pd.sv").is_file(), "{}", at.dir.display());
    // The whole repository's files, the directory's among them.
    assert!(
        at.held
            .is_some_and(|held| held.contains(&at.dir.join("pd.sv")))
    );
    assert!(matches!(&pd.origin, Origin::Within { subdir, .. } if subdir.ends_with("pd")));
    assert!(refused.contains("outside its repository"), "{refused}");
}

#[test]
fn a_path_on_disk_brings_what_it_requires() {
    let fixture = Fixture::new("dir");
    let (cells, _) = upstream(&fixture, "version = '1.0'");
    fixture.file(
        "local/astli.toml",
        &format!(
            "[dependencies]\ncells = {{ git = '{}', version = '1' }}\n",
            cells.url()
        ),
    );

    let resolution = resolved(&fixture, "local = { path = '../local' }\n", &[]).expect("resolved");

    assert_eq!(chosen(&resolution), ["cells 1.1.0", "local on disk"]);
    let local = resolution.packages[1]
        .checkout(&fixture.store())
        .expect("located");
    assert_eq!(local.dir, fixture.path().join("local"));
    assert!(local.held.is_none());
}

#[test]
fn a_locked_commit_stays_chosen_after_a_newer_release() {
    let fixture = Fixture::new("locked");
    let (cells, _) = upstream(&fixture, "version = '1.0'");
    let entries = format!("cells = {{ git = '{}', version = '1' }}\n", cells.url());
    let lock = Lock::of(&resolved(&fixture, &entries, &[]).expect("resolved"));
    cells.release("v1.2.0", &[("later.sv", "")]);

    let kept = locked(&fixture, &entries, &[], &lock).expect("resolved");
    let fetched = fixture
        .store()
        .mirror("cells", &cells.url())
        .expect("a mirror");
    let known: Vec<String> = (fetched.versions().expect("tags").into_iter())
        .map(|(version, _)| version.to_string())
        .collect();
    drop(fetched);
    let updated = resolved(&fixture, &entries, &[]).expect("resolved");

    assert_eq!(chosen(&kept), ["cells 1.1.0"]);
    assert_eq!(known, ["1.0.0", "1.1.0", "2.0.0"], "fetched though locked");
    assert_eq!(chosen(&updated), ["cells 1.2.0"]);
    assert_eq!(
        lock.changes(&Lock::of(&updated)),
        [("Updating", "cells 1.1.0 -> 1.2.0".to_string())]
    );
}

#[test]
fn a_lock_the_manifest_rules_out_resolves_again_with_every_mirror_fetched() {
    let fixture = Fixture::new("stale");
    let (cells, _) = upstream(&fixture, "version = '1.0'");
    let entries = |required: &str| {
        format!(
            "cells = {{ git = '{}', version = '{required}' }}\n",
            cells.url()
        )
    };
    let lock = Lock::of(&resolved(&fixture, &entries("1"), &[]).expect("resolved"));
    cells.release("v3.0.0", &[("later.sv", "")]);

    let moved = locked(&fixture, &entries("3"), &[], &lock).expect("resolved");

    assert_eq!(chosen(&moved), ["cells 3.0.0"]);
}

#[test]
fn a_locked_branch_stays_at_its_commit_until_the_rev_changes() {
    let fixture = Fixture::new("branch");
    let cells = fixture.repo("cells");
    let first = cells.commit(&[("cells.sv", "")]);
    cells.git(&["branch", "next"]);
    let entries = |rev: &str| format!("cells = {{ git = '{}', rev = '{rev}' }}\n", cells.url());
    let lock = Lock::of(&resolved(&fixture, &entries("main"), &[]).expect("resolved"));
    let second = cells.commit(&[("fifo.sv", "")]);

    let kept = locked(&fixture, &entries("main"), &[], &lock).expect("resolved");
    cells.git(&["switch", "--quiet", "next"]);
    let third = cells.commit(&[("cdc.sv", "")]);
    let other = locked(&fixture, &entries("next"), &[], &lock).expect("resolved");
    let updated = resolved(&fixture, &entries("main"), &[]).expect("resolved");

    assert_eq!(lock.packages[0].revs, ["main"]);
    assert_eq!(
        kept.packages[0].commit.map(|it| it.to_string()),
        Some(first)
    );
    assert_eq!(
        other.packages[0].commit.map(|it| it.to_string()),
        Some(third)
    );
    assert_eq!(
        updated.packages[0].commit.map(|it| it.to_string()),
        Some(second)
    );
}

#[test]
fn only_a_package_from_its_own_repository_is_locked() {
    let fixture = Fixture::new("unlocked");
    let soc = fixture.repo("soc");
    soc.release(
        "v1.0.0",
        &[
            ("astli.toml", "[dependencies]\npd = { path = 'pd' }\n"),
            ("pd/astli.toml", ""),
        ],
    );
    fixture.file("local/astli.toml", "");
    let entries = format!(
        "local = {{ path = '../local' }}\nsoc = {{ git = '{}', version = '1' }}\n",
        soc.url()
    );

    let lock = Lock::of(&resolved(&fixture, &entries, &[]).expect("resolved"));

    let names: Vec<&str> = lock.packages.iter().map(|it| it.name.as_str()).collect();
    assert_eq!(names, ["soc"]);
    assert_eq!(lock.packages[0].dependencies, ["pd"]);
    assert_eq!(Lock::parse(&lock.to_string()), Ok(lock));
}

/// `tech`, released as 1.0.0, and `cells` 1.0.0, which brings `tech` in only
/// with its `asic` feature, its default unless `default` says otherwise.
fn optional(fixture: &Fixture, default: &str) -> (Repo, Repo) {
    let tech = fixture.repo("tech");
    tech.release("v1.0.0", &[("tech.sv", "")]);
    let cells = fixture.repo("cells");
    let manifest = format!(
        "[features]\ndefault = [{default}]\nasic = {{ dependencies = ['tech'] }}\n\
         [dependencies]\ntech = {{ git = '{}', version = '1' }}\n",
        tech.url()
    );
    cells.release("v1.0.0", &[("astli.toml", &manifest)]);
    (tech, cells)
}

/// The names of the packages chosen.
fn names(resolution: &Resolution) -> Vec<&str> {
    (resolution.packages.iter())
        .map(|it| it.name.as_str())
        .collect()
}

#[test]
fn an_optional_dependency_is_resolved_only_where_a_feature_brings_it_in() {
    let fixture = Fixture::new("optional");
    let (_, cells) = optional(&fixture, "");
    let entry = |features: &str| {
        format!(
            "cells = {{ git = '{}', version = '1', features = [{features}] }}\n",
            cells.url()
        )
    };

    let without = resolved(&fixture, &entry(""), &[]).expect("resolved");
    let with = resolved(&fixture, &entry("'asic'"), &[]).expect("resolved");

    assert_eq!(names(&without), ["cells"]);
    assert!(without.packages[0].needs.is_empty());
    assert_eq!(names(&with), ["cells", "tech"]);
    assert_eq!(with.packages[0].needs, ["tech"]);
}

#[test]
fn defaults_are_off_only_where_every_requirement_turns_them_off() {
    let fixture = Fixture::new("defaults");
    let (_, cells) = optional(&fixture, "'asic'");
    let soc = fixture.repo("soc");
    let manifest = format!(
        "[dependencies]\ncells = {{ git = '{}', version = '1' }}\n",
        cells.url()
    );
    soc.release("v1.0.0", &[("astli.toml", &manifest)]);
    let cells_entry = format!(
        "cells = {{ git = '{}', version = '1', default-features = false }}\n",
        cells.url()
    );
    let soc_entry = format!("soc = {{ git = '{}', version = '1' }}\n", soc.url());

    let off = resolved(&fixture, &cells_entry, &[]).expect("resolved");
    let on = resolved(&fixture, &format!("{cells_entry}{soc_entry}"), &[]).expect("resolved");

    assert_eq!(names(&off), ["cells"]);
    assert_eq!(names(&on), ["cells", "soc", "tech"]);
}

#[test]
fn every_feature_of_the_root_is_resolved() {
    let fixture = Fixture::new("root-features");
    let (_, cells) = optional(&fixture, "");
    let manifest = fixture.file(
        "root/astli.toml",
        &format!(
            "[features]\nasic = {{ dependencies = ['cells/asic'] }}\n\
             [dependencies]\ncells = {{ git = '{}', version = '1' }}\n",
            cells.url()
        ),
    );
    let root = Package::read(&manifest).expect("a valid manifest");

    let resolution =
        resolve(&fixture.store(), &root, &BTreeSet::new(), &Lock::default()).expect("resolved");

    assert_eq!(names(&resolution), ["cells", "tech"]);
}

#[test]
fn a_version_without_the_feature_asked_is_passed_over() {
    let fixture = Fixture::new("feature-missing");
    let cells = fixture.repo("cells");
    cells.release("v1.0.0", &[("astli.toml", "[features]\nasic = {}\n")]);
    cells.release("v1.1.0", &[("astli.toml", "")]);
    let entries = format!(
        "cells = {{ git = '{}', version = '1', features = ['asic'] }}\n",
        cells.url()
    );

    let resolution = resolved(&fixture, &entries, &[]).expect("resolved");

    assert_eq!(chosen(&resolution), ["cells 1.0.0"]);
}

#[test]
fn only_the_roots_dev_dependencies_are_resolved() {
    let fixture = Fixture::new("dev");
    let (tech, _) = optional(&fixture, "");
    let verif = fixture.repo("verif");
    verif.release("v1.0.0", &[("verif.sv", "")]);
    let axi = fixture.repo("axi");
    let manifest = format!(
        "[dev-dependencies]\nverif = {{ git = '{}', version = '1' }}\n",
        verif.url()
    );
    axi.release("v1.0.0", &[("astli.toml", &manifest)]);
    let manifest = fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\naxi = {{ git = '{}', version = '1' }}\n\
             [dev-dependencies]\ntech = {{ git = '{}', version = '1' }}\n",
            axi.url(),
            tech.url()
        ),
    );
    let root = Package::read(&manifest).expect("a valid manifest");

    let resolution =
        resolve(&fixture.store(), &root, &BTreeSet::new(), &Lock::default()).expect("resolved");

    assert_eq!(names(&resolution), ["axi", "tech"]);
}

#[test]
fn a_patch_meets_every_requirement_and_brings_its_own() {
    let fixture = Fixture::new("patch");
    let (cells, axi) = upstream(&fixture, "version = '1.0'");
    // The clone requires a `cells` that `axi` 0.2.0 does not, and is
    // required at a version it has no tag for.
    fixture.file(
        "work/axi/astli.toml",
        &format!(
            "[dependencies]\ncells = {{ git = '{}', version = '2' }}\n",
            cells.url()
        ),
    );
    let manifest = fixture.file(
        "root/astli.toml",
        &format!(
            "[dependencies]\naxi = {{ git = '{}', version = '0.2' }}\n",
            axi.url()
        ),
    );
    let mut root = Package::read(&manifest).expect("a valid manifest");
    (root.patches.0).insert("axi".to_string(), fixture.path().join("work/axi"));
    (root.patches.0).insert("dma".to_string(), fixture.path().join("work/axi"));

    let resolution =
        resolve(&fixture.store(), &root, &BTreeSet::new(), &Lock::default()).expect("resolved");

    assert_eq!(chosen(&resolution), ["axi on disk", "cells 2.0.0"]);
    assert_eq!(
        resolution.packages[0].origin,
        Origin::Patched(fixture.path().join("work/axi"))
    );
    assert_eq!(resolution.warnings.len(), 2, "{:?}", resolution.warnings);
    assert!(resolution.warnings[0].starts_with("`axi` is patched: read from"));
    assert_eq!(
        resolution.warnings[1],
        "`dma` is patched, but the design does not depend on it"
    );
    assert!(Lock::of(&resolution).get("axi").is_none());
}
