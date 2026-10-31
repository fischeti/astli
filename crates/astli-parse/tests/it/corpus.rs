//! Finding the corpus, for the tests named `corpus_*`.
//!
//! `corpus/` is gitignored and fetched by `scripts/fetch-corpus.sh`, so every
//! test over it has to cope with not having it. None of them fail in that
//! case; they say so and pass, because a machine that has not fetched the
//! corpus is not a machine with a bug.

use std::path::PathBuf;

/// Every SystemVerilog file in the corpus, or `None` if it was never fetched.
pub fn files() -> Option<Vec<PathBuf>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.is_dir() {
        eprintln!("skipping: run scripts/fetch-corpus.sh to populate corpus/");
        return None;
    }

    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // A tool's own copies of sources, such as `.git` or `.bender`, are
            // not part of the pinned corpus, and neither is a link into one, as
            // bender's from `deps/`.
            if entry.file_name().to_string_lossy().starts_with('.') || into_hidden(&entry.path()) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("sv" | "svh")
            ) {
                files.push(path);
            }
        }
    }

    assert!(
        !files.is_empty(),
        "corpus/ exists but holds no SystemVerilog"
    );
    files.sort();
    Some(files)
}

/// Which corpus repository a file came from, for a per-repo number.
pub fn repo(path: &std::path::Path) -> String {
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
fn into_hidden(path: &std::path::Path) -> bool {
    std::fs::read_link(path).is_ok_and(|target| {
        target.components().any(|part| {
            matches!(part, std::path::Component::Normal(name)
                if name.to_string_lossy().starts_with('.'))
        })
    })
}
