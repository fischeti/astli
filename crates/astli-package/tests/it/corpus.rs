//! Reading every `Bender.yml` in the pinned corpus.
//!
//! `corpus/` is gitignored and fetched by `scripts/fetch-corpus.sh`; without
//! it the test says so and passes.

use std::path::PathBuf;

use astli_package::bender_dependencies;

#[test]
fn corpus_bender_manifests_read() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.is_dir() {
        eprintln!("skipping: run scripts/fetch-corpus.sh to populate corpus/");
        return;
    }
    let mut manifests = Vec::new();
    let mut open = vec![root];
    while let Some(dir) = open.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            // bender's own checkouts are not part of the pinned corpus.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                open.push(path);
            } else if entry.file_name() == "Bender.yml" {
                manifests.push(path);
            }
        }
    }

    let refused: Vec<String> = (manifests.iter())
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            let err = bender_dependencies(&text).err()?;
            Some(format!("{}: {err}", path.display()))
        })
        .collect();

    assert!(!manifests.is_empty());
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}
