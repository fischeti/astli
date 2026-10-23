//! Resolving designs whose dependencies are repositories the tests make.

mod common;

use std::collections::BTreeSet;

use astli_package::Package;
use astli_resolve::{Origin, Resolution, resolve};
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
    let manifest = fixture.file("root/astli.toml", &format!("[dependencies]\n{entries}"));
    let root = Package::read(&manifest).expect("a valid manifest");
    let pinned: BTreeSet<String> = pinned.iter().map(|it| it.to_string()).collect();
    resolve(&fixture.store(), &root, &pinned).map_err(|err| err.to_string())
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
    let cells = &resolution.packages[1];
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
    assert!(pd.dir.join("pd.sv").is_file(), "{}", pd.dir.display());
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
    assert_eq!(resolution.packages[1].dir, fixture.path().join("local"));
}
