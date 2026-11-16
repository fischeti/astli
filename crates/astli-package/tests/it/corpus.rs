//! Reading every `Bender.yml` in the pinned corpus.
//!
//! `corpus/` is gitignored and fetched by `scripts/fetch-corpus.sh`; without
//! it the test says so and passes.

use astli_package::bender_dependencies;
use astli_testing::corpus::{self, Links};

#[test]
fn corpus_bender_manifests_read() {
    let Some(root) = corpus::root() else {
        return;
    };
    // The checkouts bender links into `deps/` are read too, which are what
    // its manifests name.
    let manifests = corpus::walk(&root, Links::Follow, |path| {
        path.file_name().is_some_and(|name| name == "Bender.yml")
    });

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
