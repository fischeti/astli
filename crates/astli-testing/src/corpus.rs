//! Finding the files of `corpus/`, which is gitignored and fetched by
//! `scripts/fetch-corpus.sh`.
//!
//! Every test over it has to cope with not having it. None of them fail in
//! that case; they say so and pass, because a machine that has not fetched
//! the corpus is not a machine with a bug.

use std::path::{Component, Path, PathBuf};

/// The extensions of SystemVerilog sources and headers.
pub const SYSTEMVERILOG: &[&str] = &["sv", "svh"];

/// How [`walk`] treats a link into a hidden directory, as bender's `deps/` has.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Links {
    /// Leave it out: its target is a tool's own copy of a source the corpus
    /// holds already.
    SkipHidden,
    /// Follow it like any other link.
    Follow,
}

/// The corpus, found from where this crate is, or `None` if it was never
/// fetched.
pub fn root() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if root.is_dir() {
        return Some(root);
    }
    eprintln!("skipping: run scripts/fetch-corpus.sh to populate corpus/");
    None
}

/// Every SystemVerilog file in the corpus, sorted, or `None` if it was never
/// fetched.
pub fn files() -> Option<Vec<PathBuf>> {
    let files = listed(&root()?, SYSTEMVERILOG);
    assert!(
        !files.is_empty(),
        "corpus/ exists but holds no SystemVerilog"
    );
    Some(files)
}

/// The files under `dir` with one of `extensions`, sorted, but none a tool
/// keeps its own copies in, such as `.git` or `.bender`, nor through a link
/// into one.
pub fn listed(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    walk(dir, Links::SkipHidden, |path| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
    })
}

/// The files under `dir` that `keep` accepts, sorted. A hidden entry is never
/// entered, and nor is a link into one unless `links` says to follow it.
pub fn walk(dir: &Path, links: Links, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.')
                || (links == Links::SkipHidden && into_hidden(&path))
            {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if keep(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Which corpus repository a file came from, for a per-repo number.
pub fn repo(path: &Path) -> String {
    let mut after = false;
    for part in path.components() {
        let part = part.as_os_str().to_string_lossy().to_string();
        if after {
            return part;
        }
        after = part == "corpus";
    }
    "?".to_string()
}

/// Whether `path` is a link whose target passes through a hidden directory.
fn into_hidden(path: &Path) -> bool {
    std::fs::read_link(path).is_ok_and(|target| {
        target.components().any(|part| {
            matches!(part, Component::Normal(name) if name.to_string_lossy().starts_with('.'))
        })
    })
}
