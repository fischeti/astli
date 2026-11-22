//! The buffers, as a load reads them back: each change in the negotiated
//! encoding, open and close, and the disk behind them.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::support::{Client, loaded, path, uri};

#[test]
fn changes_edit_the_buffer_in_the_negotiated_encoding() {
    // Each row opens `text`, sends one change of each edit, a range of
    // `[line, character]` pairs and what replaces it, and finds `edited`.
    type Edit = ([u32; 4], &'static str);
    let utf16: &[(&str, &[Edit], &str)] = &[
        ("wire é;\n", &[([0, 5, 0, 6], "q")], "wire q;\n"),
        // U+1D11E is two UTF-16 code units, and four bytes.
        ("// 𝄞 x\n", &[([0, 6, 0, 7], "y")], "// 𝄞 y\n"),
        ("// 𝄞 x\n", &[([0, 3, 0, 5], "")], "//  x\n"),
        // A position inside a character is that character's start.
        ("𝄞", &[([0, 1, 0, 1], "x")], "x𝄞"),
        ("a\nb\nc\n", &[([0, 1, 2, 0], "")], "ac\n"),
        // Past the end of a line is its end, before a `\r\n` too, and past
        // the last line the end of the text.
        ("ab\ncd\n", &[([0, 10, 0, 10], "!")], "ab!\ncd\n"),
        ("ab\r\ncd\r\n", &[([0, 5, 0, 5], "!")], "ab!\r\ncd\r\n"),
        ("ab\ncd\n", &[([9, 0, 9, 4], "x")], "ab\ncd\nx"),
        ("ab\ncd", &[([1, 9, 1, 9], "\n")], "ab\ncd\n"),
        // A range whose end is before its start is empty.
        ("abc", &[([0, 2, 0, 1], "x")], "abxc"),
        // Each change reads the text the one before it left.
        ("ab", &[([0, 0, 0, 0], "x"), ([0, 1, 0, 1], "y")], "xyab"),
        ("é", &[([0, 0, 0, 1], "𝄞"), ([0, 2, 0, 2], "!")], "𝄞!"),
        ("", &[([0, 0, 0, 0], "a\n"), ([1, 0, 1, 0], "b")], "a\nb"),
    ];
    let utf8: &[(&str, &[Edit], &str)] = &[
        ("wire é;\n", &[([0, 5, 0, 7], "q")], "wire q;\n"),
        ("// 𝄞 x\n", &[([0, 3, 0, 7], "")], "//  x\n"),
        ("// 𝄞 x\n", &[([0, 8, 0, 9], "y")], "// 𝄞 y\n"),
        ("é", &[([0, 1, 0, 1], "x")], "xé"),
    ];
    for (utf8, rows) in [(false, utf16), (true, utf8)] {
        // One server for every row, each row a design of its own.
        let mut client = Client::start(&["a.sv"], utf8);
        let mut wanted = HashMap::new();
        for (row, &(text, edits, edited)) in rows.iter().enumerate() {
            let name = format!("row{row}/a.sv");
            let uri = uri(&name);
            client.open(&uri, text);
            let changes: Vec<Value> = (edits.iter())
                .map(|([l0, c0, l1, c1], text)| {
                    let start = json!({ "line": l0, "character": c0 });
                    let end = json!({ "line": l1, "character": c1 });
                    json!({ "range": { "start": start, "end": end }, "text": text })
                })
                .collect();
            let document = json!({ "uri": uri, "version": 7 });
            let params = json!({ "textDocument": document, "contentChanges": changes });
            client.notify("textDocument/didChange", params);
            let root = path(&name).parent().unwrap().to_path_buf();
            wanted.insert(root, (row, text, edited));
        }
        // A design opened and changed at once may be loaded once or twice,
        // the last time with the change.
        let mut found: HashMap<PathBuf, String> = HashMap::new();
        while found.len() < wanted.len()
            || (wanted.iter()).any(|(root, (.., edited))| found[root] != *edited)
        {
            let load = client.load();
            found.insert(load.root.clone(), load.text("a.sv").unwrap().to_string());
        }
        for (root, (row, text, edited)) in &wanted {
            assert_eq!(found[root], *edited, "row {row}: {text:?}");
        }
        let request = client.request("astli/unknown", Value::Null);
        assert!(request.is_err());
        client.logs.retain(|it| !loaded(it));
        assert!(client.logs.is_empty(), "utf8 {utf8}: {:?}", client.logs);
        client.finish();
    }
}

#[test]
fn documents_are_held_from_open_to_close() {
    let mut client = Client::start(&["a b.sv", "b.sv"], false);

    // A space in a path is escaped in its URI.
    let uri = &uri("a b.sv");
    assert!(uri.ends_with("a%20b.sv"));
    client.open(uri, "module a;\n");
    let load = client.load();
    assert_eq!(load.root, path("a b.sv").parent().unwrap());
    assert_eq!(
        load.texts,
        [("a b.sv".to_string(), "module a;\n".to_string())]
    );
    client.replace(uri, "module b;\n");
    let load = client.load_until(|it| it.text("a b.sv") == Some("module b;\n"));
    assert_eq!(load.texts.len(), 1);
    client.close(uri);

    // A buffer no file is behind is not held, and loads nothing.
    client.open("untitled:Untitled-1", "module c;\n");

    // Closed, the file is read from the disk, which does not hold it.
    let other = &crate::support::uri("b.sv");
    client.open(other, "module c;\n");
    let load = client.load_until(|it| it.text("a b.sv").is_none());
    assert_eq!(
        load.texts,
        [("b.sv".to_string(), "module c;\n".to_string())]
    );

    // A change to a file that is not open is told, and changes nothing.
    let change = json!({ "textDocument": { "uri": uri, "version": 3 }, "contentChanges": [{ "text": "x" }] });
    client.notify("textDocument/didChange", change);
    let said = client.logged(|it| !loaded(it));
    assert_eq!(said, format!("{uri} changed, and is not open"));

    // So are params that are not a notification's.
    client.notify("textDocument/didOpen", json!({ "textDocument": 1 }));
    let said = client.logged(|it| !loaded(it));
    assert!(
        said.starts_with("`textDocument/didOpen`: invalid type"),
        "{said}"
    );
    client.finish();
}

#[test]
fn buffers_are_read_before_the_disk() {
    let names = ["Cargo.toml", "missing.sv", "src/../missing.sv"];
    let mut client = Client::start(&names, false);
    let on_disk = std::fs::read_to_string(path("Cargo.toml")).unwrap();

    client.open(&uri("missing.sv"), "unsaved");
    let load = client.load();
    // Spelled another way, the path is the same file.
    let want = [
        (names[0], on_disk.as_str()),
        (names[1], "unsaved"),
        (names[2], "unsaved"),
    ];
    let texts: Vec<(&str, &str)> = (load.texts.iter())
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    assert_eq!(texts, want);

    client.open(&uri("Cargo.toml"), "edited");
    let load = client.load_until(|it| it.text("Cargo.toml") == Some("edited"));
    assert_eq!(load.text("missing.sv"), Some("unsaved"));

    client.close(&uri("Cargo.toml"));
    let load = client.load_until(|it| it.text("Cargo.toml") != Some("edited"));
    assert_eq!(load.text("Cargo.toml"), Some(on_disk.as_str()));
    client.logs.retain(|it| !loaded(it));
    assert!(client.logs.is_empty(), "{:?}", client.logs);
    client.finish();
}
