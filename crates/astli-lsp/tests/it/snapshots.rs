//! Each design an open file is in, loaded once its edits settle, and
//! swapped in unless an edit overtook its load.

use crate::support::{Client, loaded, path, uri};

#[test]
fn a_design_is_loaded_on_open_and_again_after_an_edit() {
    let mut client = Client::start(&["a.sv"], false);
    let uri = &uri("a.sv");
    let root = path("a.sv").parent().unwrap().to_path_buf();
    client.open(uri, "module a;");
    let load = client.load();
    assert_eq!((&load.root, load.discover), (&root, true));
    assert_eq!(load.text("a.sv"), Some("module a;"));
    let said = client.logged(loaded);
    let prefix = format!("loaded {}: 0 units, 0 lowered, in ", root.display());
    assert!(said.starts_with(&prefix) && said.ends_with(" ms"), "{said}");

    // An edit loads the design again, from what was discovered.
    client.replace(uri, "module b;");
    let load = client.load();
    assert_eq!((&load.root, load.discover), (&root, false));
    assert_eq!(load.text("a.sv"), Some("module b;"));
    client.logged(loaded);
    assert!(client.logs.is_empty(), "{:?}", client.logs);
    client.finish();
}

#[test]
fn an_edit_overtakes_the_load_it_makes_stale() {
    let mut client = Client::start(&["a.sv"], false);
    let uri = &uri("a.sv");
    client.open(uri, "wait");
    let first = client.load();
    assert_eq!(first.text("a.sv"), Some("wait"));
    // The load waits until this edit overtakes it, and makes nothing.
    client.replace(uri, "module b;");
    let second = client.load();
    assert_eq!(second.text("a.sv"), Some("module b;"));
    // What the first discovered was not taken, so the second discovers.
    assert!(second.discover);
    client.logged(loaded);

    // Only the second load was swapped in.
    let _ = client.request("astli/unknown", serde_json::Value::Null);
    assert!(client.logs.is_empty(), "{:?}", client.logs);
    client.finish();
}

#[test]
fn files_of_two_designs_are_loaded_apart() {
    let mut client = Client::start(&["a.sv", "b.sv"], false);
    client.open(&uri("one/a.sv"), "module a;");
    client.open(&uri("two/b.sv"), "module b;");
    let mut loads = [client.load(), client.load()];
    loads.sort_by(|a, b| a.root.cmp(&b.root));
    assert_eq!(loads[0].root, path("one"));
    assert_eq!(loads[0].texts, [("a.sv".into(), "module a;".into())]);
    assert_eq!(loads[1].root, path("two"));
    assert_eq!(loads[1].texts, [("b.sv".into(), "module b;".into())]);
    client.logged(loaded);
    client.logged(loaded);

    // An edit to one loads that one alone.
    client.replace(&uri("two/b.sv"), "module c;");
    let load = client.load();
    assert_eq!(load.root, path("two"));
    client.logged(loaded);
    let _ = client.request("astli/unknown", serde_json::Value::Null);
    assert!(client.logs.is_empty(), "{:?}", client.logs);
    client.finish();
}

#[test]
fn an_edit_to_a_file_another_design_read_loads_that_one_too() {
    // Each design reads `../shared.sv`, as one that includes a header
    // outside its root would.
    let mut client = Client::start(&["a.sv", "../shared.sv"], false);
    client.open(&uri("one/a.sv"), "module a;");
    client.open(&uri("two/a.sv"), "module a;");
    client.load();
    client.load();
    client.logged(loaded);
    client.logged(loaded);

    // Opened, the shared file is a design of its own, and each design that
    // read it is loaded again.
    client.open(&uri("shared.sv"), "module shared;");
    let mut roots: Vec<_> = (0..3).map(|_| client.load().root).collect();
    roots.sort();
    let root = path("shared.sv").parent().unwrap().to_path_buf();
    assert_eq!(roots, [root, path("one"), path("two")]);
    client.finish();
}

#[test]
fn a_load_that_waits_on_the_pool_leaves_it_to_the_jobs() {
    // Fewer threads than designs loading at once, each load waiting for
    // jobs it spawned: a load that ran on the pool would hold the thread
    // they need. The pool is built once a process, so under `cargo test`
    // another test may have built it already.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global();
    let mut client = Client::start(&["a.sv"], false);
    client.open(&uri("one/a.sv"), "scoped");
    client.open(&uri("two/a.sv"), "scoped");
    client.logged(loaded);
    client.logged(loaded);
    client.finish();
}

#[test]
fn a_load_of_a_design_closed_is_not_taken_by_one_reopened() {
    let mut client = Client::start(&["a.sv"], false);
    let uri = &uri("one/a.sv");
    client.open(uri, "hold");
    client.held();
    // Closed, the design is forgotten; reopened, it is a new one.
    client.close(uri);
    assert_eq!(client.forgotten(), path("one"));
    client.open(uri, "module a;");
    let _ = client.request("astli/unknown", serde_json::Value::Null);
    client.release();
    assert_eq!(client.load().text("a.sv"), Some("hold"));
    assert_eq!(client.load().text("a.sv"), Some("module a;"));
    client.logged(loaded);

    // Only the load of the design reopened was swapped in.
    let _ = client.request("astli/unknown", serde_json::Value::Null);
    assert!(client.logs.is_empty(), "{:?}", client.logs);
    client.finish();
}

#[test]
fn a_file_opened_before_a_load_reads_it_loads_the_design_again() {
    let mut client = Client::start(&["a.sv", "../shared.sv"], false);
    client.open(&uri("one/a.sv"), "hold");
    client.held();
    // The load has not read the shared file yet, so opening it overtakes
    // nothing; the load reads it as it was when the load started.
    client.open(&uri("shared.sv"), "module shared;");
    let _ = client.request("astli/unknown", serde_json::Value::Null);
    client.release();
    let first = client.load_until(|load| load.root == path("one"));
    assert_eq!(first.text("../shared.sv"), None);
    // Its snapshot is taken, the nearest there is, and the design loaded
    // again with what is open.
    let one = format!("loaded {}:", path("one").display());
    client.logged(|said| said.starts_with(&one));
    client.held();
    client.release();
    let again = client.load_until(|load| load.root == path("one"));
    assert_eq!(again.text("../shared.sv"), Some("module shared;"));
    client.logged(|said| said.starts_with(&one));
    client.finish();
}
