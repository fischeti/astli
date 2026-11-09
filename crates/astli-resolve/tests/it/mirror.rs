//! Mirroring repositories the tests make with `git`, and reading them.

use crate::common;

use std::path::Path;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use astli_resolve::Store;
use common::Fixture;

fn hex(id: impl ToString) -> String {
    id.to_string()
}

#[test]
fn versions_are_the_tags_after_a_v() {
    let fixture = Fixture::new("versions");
    let upstream = fixture.repo("upstream");
    let first = upstream.commit(&[("a.sv", "")]);
    upstream.git(&["tag", "v0.1.0"]);
    upstream.git(&["tag", "0.3.0"]);
    upstream.git(&["tag", "release"]);
    let second = upstream.commit(&[("b.sv", "")]);
    upstream.git(&["tag", "--annotate", "--message", "a release", "v0.2.0"]);
    upstream.git(&["tag", "v1.0.0-rc.1"]);

    let mirror = fixture
        .store()
        .mirror("upstream", &upstream.url())
        .expect("a mirror");
    let versions = mirror.versions().expect("its versions");

    let read: Vec<(String, String)> = (versions.iter())
        .map(|(version, commit)| (version.to_string(), hex(commit)))
        .collect();
    // An annotated tag is read as the commit it tags.
    assert_eq!(
        read,
        [
            ("0.1.0".to_string(), first),
            ("0.2.0".to_string(), second.clone()),
            ("1.0.0-rc.1".to_string(), second),
        ]
    );
    assert!(mirror.fresh);
}

#[test]
fn a_rev_names_a_branch_a_tag_or_a_commit() {
    let fixture = Fixture::new("revs");
    let upstream = fixture.repo("upstream");
    let first = upstream.commit(&[("a.sv", "")]);
    upstream.git(&["tag", "pulp-v2.0.0"]);
    upstream.git(&["switch", "--quiet", "--create", "common-cells-v2"]);
    let branch = upstream.commit(&[("b.sv", "")]);
    upstream.git(&["switch", "--quiet", "main"]);

    let mirror = fixture
        .store()
        .mirror("upstream", &upstream.url())
        .expect("a mirror");
    let rev = |rev: &str| mirror.rev(rev).map(hex);

    assert_eq!(rev("main"), Some(first.clone()));
    assert_eq!(rev("pulp-v2.0.0"), Some(first.clone()));
    assert_eq!(rev("common-cells-v2"), Some(branch.clone()));
    assert_eq!(rev(&branch), Some(branch.clone()));
    assert_eq!(rev(&first[..10]), Some(first));
    assert_eq!(rev("nowhere"), None);
}

#[test]
fn a_file_is_read_at_a_commit_without_a_checkout() {
    let fixture = Fixture::new("files");
    let upstream = fixture.repo("upstream");
    let old = upstream.commit(&[("astli.toml", "[package]\nname = \"old\"\n")]);
    let new = upstream.commit(&[
        ("astli.toml", "[package]\nname = \"new\"\n"),
        ("src/core.sv", "module core;\nendmodule\n"),
    ]);

    let mirror = fixture
        .store()
        .mirror("upstream", &upstream.url())
        .expect("a mirror");
    let file = |commit: &str, path: &str| {
        let commit = mirror.rev(commit).expect("a commit");
        let read = mirror.file(commit, path).expect("readable");
        read.map(|bytes| String::from_utf8(bytes).expect("UTF-8"))
    };

    assert_eq!(
        file(&old, "astli.toml").as_deref(),
        Some("[package]\nname = \"old\"\n")
    );
    assert_eq!(
        file(&new, "src/core.sv").as_deref(),
        Some("module core;\nendmodule\n")
    );
    assert_eq!(file(&old, "src/core.sv"), None);
    // A directory is not a file.
    assert_eq!(file(&new, "src"), None);
}

#[test]
fn a_fetch_brings_what_was_pushed_since() {
    let fixture = Fixture::new("fetch");
    let upstream = fixture.repo("upstream");
    upstream.commit(&[("a.sv", "")]);
    upstream.git(&["tag", "v0.1.0"]);
    drop(
        fixture
            .store()
            .mirror("upstream", &upstream.url())
            .expect("a mirror"),
    );
    let later = upstream.commit(&[("b.sv", "")]);
    upstream.git(&["tag", "v0.2.0"]);

    let mut mirror = fixture
        .store()
        .mirror("upstream", &upstream.url())
        .expect("the mirror");
    let before = mirror.versions().expect("its versions").len();
    mirror.fetch().expect("fetched");
    let after = mirror.versions().expect("its versions");

    // The mirror was there already, and a run fetches only when it must.
    assert!(!mirror.fresh);
    assert_eq!(before, 1);
    assert_eq!(after.len(), 2);
    assert_eq!(hex(after[1].1), later);
}

/// The files under `dir`, relative to it, sorted.
fn listed(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut open = vec![dir.to_path_buf()];
    while let Some(at) = open.pop() {
        for entry in std::fs::read_dir(&at).expect("a directory") {
            let path = entry.expect("an entry").path();
            match path.is_dir() {
                true => open.push(path),
                false => files.push(
                    (path.strip_prefix(dir).expect("inside"))
                        .to_string_lossy()
                        .replace('\\', "/"),
                ),
            }
        }
    }
    files.sort();
    files
}

#[test]
fn a_checkout_is_a_commits_files_and_nothing_else() {
    let fixture = Fixture::new("checkout");
    let upstream = fixture.repo("upstream");
    let old = upstream.commit(&[("src/core.sv", "module core;\nendmodule\n")]);
    let new = upstream.commit(&[("include/core/defs.svh", "`define W 4\n")]);

    let mirror = fixture
        .store()
        .mirror("upstream", &upstream.url())
        .expect("a mirror");
    let at = |commit: &str| {
        let commit = mirror.rev(commit).expect("a commit");
        mirror.checkout(commit).expect("checked out")
    };
    let old_out = at(&old);
    let new_out = at(&new);
    let new_dir = &new_out.dir;

    assert_eq!(listed(&old_out.dir), ["src/core.sv"]);
    assert_eq!(listed(new_dir), ["include/core/defs.svh", "src/core.sv"]);
    assert_eq!(
        std::fs::read_to_string(new_dir.join("src/core.sv")).expect("written"),
        "module core;\nendmodule\n"
    );
    let files: Vec<_> = new_out.files.iter().cloned().collect();
    assert_eq!(
        files,
        [
            new_dir.join("include/core/defs.svh"),
            new_dir.join("src/core.sv")
        ]
    );
    // Written once, and found again after.
    assert_eq!(&at(&new).dir, new_dir);
    assert!(new_dir.ends_with(&new));
}

#[test]
fn a_checkout_cannot_be_written_but_can_be_deleted() {
    let fixture = Fixture::new("sealed");
    let upstream = fixture.repo("upstream");
    let commit = upstream.commit(&[("src/core.sv", "module core;\nendmodule\n")]);

    let mirror = fixture
        .store()
        .mirror("upstream", &upstream.url())
        .expect("a mirror");
    let checkout = mirror
        .checkout(mirror.rev(&commit).expect("a commit"))
        .expect("checked out");
    let file = checkout.dir.join("src/core.sv");
    let written = std::fs::write(&file, "edited");
    let added = std::fs::write(checkout.dir.join("src/generated.sv"), "");
    let deleted = std::fs::remove_dir_all(&checkout.dir);

    assert!(written.is_err(), "a file of the commit was edited");
    assert!(added.is_ok(), "a directory stays writable");
    assert!(deleted.is_ok(), "{deleted:?}");
}

#[test]
fn a_commit_no_branch_reaches_is_fetched_by_its_id() {
    let fixture = Fixture::new("orphan");
    let upstream = fixture.repo("upstream");
    upstream.commit(&[("a.sv", "")]);
    upstream.git(&["switch", "--quiet", "--create", "gone"]);
    let orphan = upstream.commit(&[("b.sv", "")]);
    upstream.git(&["switch", "--quiet", "main"]);
    upstream.git(&["branch", "--quiet", "--delete", "--force", "gone"]);
    // As a forge serves any commit it holds; `git` itself needs telling.
    upstream.git(&["config", "uploadpack.allowAnySHA1InWant", "true"]);

    let mut mirror = fixture
        .store()
        .mirror("upstream", &upstream.url())
        .expect("a mirror");
    let before = mirror.rev(&orphan);
    mirror
        .fetch_commit(orphan.parse().expect("a commit id"))
        .expect("fetched");

    assert_eq!(before, None);
    assert_eq!(mirror.rev(&orphan).map(hex), Some(orphan));
}

#[test]
fn runs_opening_mirrors_in_opposite_orders_both_finish() {
    let fixture = Fixture::new("crossed");
    let (a, b) = (fixture.repo("a"), fixture.repo("b"));
    let (in_a, in_b) = (a.commit(&[("a.sv", "")]), b.commit(&[("b.sv", "")]));
    let (a, b) = (a.url(), b.url());
    // Each run opens one mirror and keeps it, as resolution keeps every
    // mirror it reads, then, once both have, fetches and checks out the
    // other.
    let both = Arc::new(Barrier::new(2));
    let run = |store: Store, kept: String, other: String, commit: String, both: Arc<Barrier>| {
        let _kept = store.mirror("kept", &kept).expect("a mirror");
        both.wait();
        let mut other = store.mirror("other", &other).expect("a mirror");
        other.fetch().expect("fetched");
        let id = other.rev(&commit).expect("a commit");
        other.checkout(id).expect("checked out");
    };

    // Runs that waited for one lock while holding another would wait for
    // ever, so each runs on a thread of its own, and is given up on.
    let (done, finished) = std::sync::mpsc::channel();
    for (kept, other, commit) in [(a.clone(), b.clone(), in_b), (b, a, in_a)] {
        let (store, done, both) = (fixture.store(), done.clone(), both.clone());
        std::thread::spawn(move || {
            run(store, kept, other, commit, both);
            let _ = done.send(());
        });
    }
    for _ in 0..2 {
        let waited = finished.recv_timeout(Duration::from_secs(60));
        assert!(waited.is_ok(), "the runs waited on each other");
    }
}

#[test]
fn cleaning_deletes_mirrors_checkouts_and_locks() {
    let fixture = Fixture::new("clean");
    let upstream = fixture.repo("upstream");
    let commit = upstream.commit(&[("src/core.sv", "")]);
    let store = fixture.store();
    {
        let mirror = store.mirror("upstream", &upstream.url()).expect("a mirror");
        mirror
            .checkout(mirror.rev(&commit).expect("a commit"))
            .expect("checked out");
    }

    let cleaned = store.clean().expect("cleaned");
    let again = store.clean().expect("nothing to clean");

    assert!(cleaned && !again);
    assert!(!store.root.exists());
}
