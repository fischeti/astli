//! The corpus repositories as packages: each that `scripts/corpus/` holds an
//! `astli.toml` for is resolved over its `astli.lock` and checked, and what
//! that prints is compared with the `check.txt` beside the manifest.
//!
//! `corpus/` is gitignored and fetched by `scripts/fetch-corpus.sh`, which
//! copies the manifests and locks in; without it the test says so and
//! passes. Resolving reads the shared cache, and fetches into it what it
//! lacks. A changed `check.txt` is accepted by running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run --release -p astli-cli corpus_packages
//! ```
//!
//! and the diff is the review: an error gone is a fix, one new is a
//! regression, unless the manifest changed with it.

use std::panic;
use std::path::{Path, PathBuf};
use std::process::Command;

use expect_test::expect_file;

#[test]
fn corpus_packages_resolve_and_check_as_expected() {
    // Canonical, as the paths `astli` prints are.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace = workspace.canonicalize().unwrap_or(workspace);
    let corpus = workspace.join("corpus");
    if !corpus.is_dir() {
        eprintln!("skipping: run scripts/fetch-corpus.sh to populate corpus/");
        return;
    }
    let overlays = workspace.join("scripts/corpus");
    let mut repos: Vec<PathBuf> = (std::fs::read_dir(&overlays).expect("scripts/corpus/"))
        .flatten()
        .map(|it| it.path())
        .filter(|it| it.join("astli.toml").is_file())
        .collect();
    repos.sort();
    assert!(
        !repos.is_empty(),
        "no manifests under {}",
        overlays.display()
    );

    let mut failed = Vec::new();
    for overlay in &repos {
        let name = overlay.file_name().unwrap_or_default().to_string_lossy();
        let repo = corpus.join(&*name);
        let read = |path: PathBuf| std::fs::read_to_string(path).ok();
        if let Some(file) = (["astli.toml", "astli.lock"].into_iter())
            .find(|it| read(repo.join(it)) != read(overlay.join(it)))
        {
            failed.push(format!(
                "{name}: {file} is not the one scripts/corpus/ holds; run scripts/fetch-corpus.sh"
            ));
            continue;
        }
        // Fetched first, since what is compared must not depend on what the
        // shared cache already holds: filling an empty one prints progress.
        run(&repo, &["fetch"]);
        let printed = format!(
            "$ astli tree\n{}\n$ astli check\n{}",
            run(&repo, &["tree"]),
            run(&repo, &["check", "--diag", "short"])
        );
        // One repository's mismatch must not hide the rest, so each is
        // caught and the panic message -- the diff -- is left to the
        // default hook to print.
        let compared =
            panic::catch_unwind(|| expect_file![overlay.join("check.txt")].assert_eq(&printed));
        if compared.is_err() {
            failed.push(format!("{name}: check.txt differs"));
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} repositories differ (UPDATE_EXPECT=1 to accept):\n{}",
        failed.len(),
        repos.len(),
        failed.join("\n")
    );
}

/// What `astli` with `args` prints in `repo`, the lock held: stdout, then
/// stderr, with paths as they read on any machine. A pin's notes on what it
/// replaced are left out, since the manifest says what it pins.
fn run(repo: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_astli"))
        .args(args)
        .arg("--locked")
        .current_dir(repo)
        .output()
        .expect("the driver runs");
    let root = format!("{}/", repo.display());
    let mut printed = String::from_utf8_lossy(&output.stdout).into_owned();
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if !line.contains("the root pins it to") {
            printed += &portable(&line.replace(&root, ""));
            printed.push('\n');
        }
    }
    printed
}

/// `line` with each path into a checkout of the shared cache shortened to
/// the package and the path inside it, since where the cache is and how a
/// checkout is named are the machine's.
fn portable(line: &str) -> String {
    const CHECKOUTS: &str = "/git/checkouts/";
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find(CHECKOUTS) {
        // Back to where the path starts.
        let start = rest[..at].rfind([' ', ',', '(']).map_or(0, |it| it + 1);
        out += &rest[..start];
        let inside = &rest[at + CHECKOUTS.len()..];
        // `<name>-<hash>/<commit>/<path>`.
        let mut parts = inside.splitn(3, '/');
        let (Some(checkout), Some(_commit), Some(path)) =
            (parts.next(), parts.next(), parts.next())
        else {
            out += &rest[start..];
            return out;
        };
        let name = checkout.rsplit_once('-').map_or(checkout, |(name, _)| name);
        out += &format!("{name}/");
        rest = path;
    }
    out + rest
}
