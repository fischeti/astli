//! Admitting the file an `` `include `` names: which candidate is read, and
//! when following one would not terminate.
//!
//! The files are a map rather than the filesystem, which is the point of
//! [`Reader`] being a trait. `svirig-syntax`'s own tests cover the same ground
//! with real `` `include ``s behind it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use svirig_text::{FileId, Included, Origins, Reader, Span, clean};

#[derive(Default)]
struct Tree(HashMap<PathBuf, String>);

impl Reader for Tree {
    fn read(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }
}

impl Tree {
    fn with(mut self, path: &str, text: &str) -> Tree {
        self.0.insert(path.into(), text.to_string());
        self
    }
}

/// The file a driver named, which it hands over rather than the store going
/// to look: only an `` `include `` reaches through [`Reader`].
fn root(origins: &mut Origins, tree: &Tree, path: &str) -> FileId {
    let text = tree.read(Path::new(path)).expect("the tree has it");
    origins.add_file(path, text)
}

/// A span standing for the `` `include `` that pulled a file in. Which bytes
/// it covers does not matter here; which file it is in does.
fn site(origins: &Origins, file: FileId) -> Span {
    Span::new(file, 0, origins.text(file).len() as u32)
}

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

/// The file that was opened, for the tests that expect one.
fn opened(included: Included) -> FileId {
    match included {
        Included::Opened(file) => file,
        other => panic!("expected a file to be opened, got {other:?}"),
    }
}

#[test]
fn the_first_candidate_that_reads_wins() {
    let tree = Tree::default()
        .with("top.sv", "")
        .with("vendor/f.svh", "vendor\n")
        .with("local/f.svh", "local\n");
    let mut origins = Origins::new();
    let top = root(&mut origins, &tree, "top.sv");

    let found = opened(origins.load_included(
        &tree,
        &paths(&["nowhere/f.svh", "local/f.svh", "vendor/f.svh"]),
        site(&origins, top),
    ));

    assert_eq!(origins.text(found), "local\n");
    assert_eq!(origins.include_depth(found), 1);
    assert_eq!(origins.included_from(found).map(|at| at.file), Some(top));
}

#[test]
fn nothing_on_the_list_reads() {
    let tree = Tree::default().with("top.sv", "");
    let mut origins = Origins::new();
    let top = root(&mut origins, &tree, "top.sv");

    let before = origins.files().count();
    assert_eq!(
        origins.load_included(&tree, &paths(&["a.svh", "b.svh"]), site(&origins, top)),
        Included::NotFound
    );
    // A candidate that does not read leaves nothing behind.
    assert_eq!(origins.files().count(), before);
}

#[test]
fn a_file_already_open_above_is_refused() {
    let tree = Tree::default().with("top.sv", "").with("f.svh", "");
    let mut origins = Origins::new();
    let top = root(&mut origins, &tree, "top.sv");

    let once = opened(origins.load_included(&tree, &paths(&["f.svh"]), site(&origins, top)));
    // A second inclusion from elsewhere is ordinary: two buffers, two sites.
    let twice = opened(origins.load_included(&tree, &paths(&["f.svh"]), site(&origins, top)));
    assert_ne!(once, twice);

    // Reaching it from inside itself is the cycle, and does not terminate.
    // Told apart from a name that reads nowhere, because they are different
    // mistakes to be told about.
    assert_eq!(
        origins.load_included(&tree, &paths(&["f.svh"]), site(&origins, once)),
        Included::Cycle
    );
    // So is reaching the file that started the chain.
    assert_eq!(
        origins.load_included(&tree, &paths(&["top.sv"]), site(&origins, once)),
        Included::Cycle
    );
}

#[test]
fn a_path_is_compared_after_the_dots_are_resolved() {
    assert_eq!(clean(Path::new("dir/../f.svh")), PathBuf::from("f.svh"));
    assert_eq!(clean(Path::new("./a/./b")), PathBuf::from("a/b"));
    // Only a real segment cancels: these have nothing to climb out of.
    assert_eq!(clean(Path::new("../../f")), PathBuf::from("../../f"));
    assert_eq!(clean(Path::new("/../f")), PathBuf::from("/f"));

    let tree = Tree::default().with("top.sv", "").with("f.svh", "");
    let mut origins = Origins::new();
    let top = root(&mut origins, &tree, "top.sv");
    let f = opened(origins.load_included(&tree, &paths(&["f.svh"]), site(&origins, top)));

    // The store compares the paths it is handed, so cleaning a candidate
    // before it gets here is what makes `dir/../f.svh` the same file as
    // `f.svh` and the cycle visible.
    let candidate = clean(Path::new("dir/../f.svh"));
    assert_eq!(
        origins.load_included(&tree, std::slice::from_ref(&candidate), site(&origins, f)),
        Included::Cycle
    );
}
