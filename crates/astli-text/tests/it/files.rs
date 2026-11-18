//! Admitting the file an `` `include `` names, and refusing one whose
//! following would not terminate.
//!
//! Which file a name finds is the preprocessor's search; the store is handed
//! the path and text it settled on. `astli-preproc`'s own tests cover the
//! same ground with real `` `include ``s behind it.

use std::path::{Path, PathBuf};

use astli_text::{Origins, SourceId, Span, clean};

/// A span standing for the `` `include `` that pulled a file in. Which bytes
/// it covers does not matter here; which file it is in does.
fn site(origins: &Origins, file: SourceId) -> Span {
    Span::new(file, 0, origins.text(file).len() as u32)
}

/// `path`, empty, as the file the `` `include `` in `from` names.
fn include(origins: &mut Origins, path: &str, from: SourceId) -> Option<SourceId> {
    origins.add_included(path.into(), String::new(), site(origins, from), None)
}

#[test]
fn an_included_file_records_where_it_was_included() {
    let mut origins = Origins::new();
    let top = origins.add_file("top.sv", String::new());

    let found = origins
        .add_included(
            "f.svh".into(),
            "f\n".into(),
            site(&origins, top),
            Some("f.svh"),
        )
        .expect("nothing is open but `top.sv`");

    assert_eq!(origins.text(found), "f\n");
    assert_eq!(origins.include_depth(found), 1);
    assert_eq!(origins.included_from(found).map(|at| at.src_id), Some(top));
    assert_eq!(origins.included_as(found), Some("f.svh"));
}

#[test]
fn a_file_already_open_above_is_refused() {
    let mut origins = Origins::new();
    let top = origins.add_file("top.sv", String::new());

    let once = include(&mut origins, "f.svh", top).expect("not open yet");
    // A second inclusion from elsewhere is ordinary: two buffers, two sites.
    let twice = include(&mut origins, "f.svh", top).expect("not open above `top.sv`");
    assert_ne!(once, twice);

    // Reaching it from inside itself is the cycle, and does not terminate.
    let before = origins.files().count();
    assert_eq!(include(&mut origins, "f.svh", once), None);
    // So is reaching the file that started the chain.
    assert_eq!(include(&mut origins, "top.sv", once), None);
    // A refused file leaves nothing behind.
    assert_eq!(origins.files().count(), before);
}

#[test]
fn a_path_is_compared_after_the_dots_are_resolved() {
    assert_eq!(clean(Path::new("dir/../f.svh")), PathBuf::from("f.svh"));
    assert_eq!(clean(Path::new("./a/./b")), PathBuf::from("a/b"));
    // Only a real segment cancels: these have nothing to climb out of.
    assert_eq!(clean(Path::new("../../f")), PathBuf::from("../../f"));
    assert_eq!(clean(Path::new("/../f")), PathBuf::from("/f"));

    let mut origins = Origins::new();
    let top = origins.add_file("top.sv", String::new());
    let f = include(&mut origins, "f.svh", top).expect("not open yet");

    // The store compares the paths it is handed, so cleaning a candidate
    // before it gets here is what makes `dir/../f.svh` the same file as
    // `f.svh` and the cycle visible.
    let candidate = clean(Path::new("dir/../f.svh"));
    let at = site(&origins, f);
    assert_eq!(
        origins.add_included(candidate, String::new(), at, None),
        None
    );
}
