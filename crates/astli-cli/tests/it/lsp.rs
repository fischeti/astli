//! `astli lsp` as a client starts it, over stdin and stdout: what it reads
//! of the package a file is in.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::support::{Fixture, driver, stderr, stdout};

/// `messages`, framed as the protocol frames them.
fn framed(messages: &[Value]) -> String {
    let mut out = String::new();
    for message in messages {
        let body = message.to_string();
        out.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    }
    out
}

/// The messages in what the server wrote.
fn unframed(text: &str) -> Vec<Value> {
    (text.split("Content-Length: ").skip(1))
        .map(|it| {
            let body = &it[it.find("\r\n\r\n").expect("a header's end") + 4..];
            serde_json::from_str(body).expect("a message")
        })
        .collect()
}

/// The `file:` URI of `path`, which is absolute.
fn uri(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    match path.starts_with('/') {
        true => format!("file://{path}"),
        false => format!("file:///{path}"),
    }
}

/// Opens each file with its text in a server run in `fixture`, and asks to
/// format it. Returns the edits for each, and what the server showed.
fn format(fixture: &Fixture, files: &[(&Path, &str)]) -> (Vec<Value>, Vec<String>) {
    let mut messages = vec![
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize",
                "params": { "processId": null, "capabilities": {} } }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    ];
    for (at, &(path, text)) in files.iter().enumerate() {
        let document = json!({ "uri": uri(path), "languageId": "systemverilog", "version": 1,
                               "text": text });
        messages.push(json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen",
                              "params": { "textDocument": document } }));
        let options = json!({ "tabSize": 8, "insertSpaces": false });
        messages.push(
            json!({ "jsonrpc": "2.0", "id": at + 1, "method": "textDocument/formatting",
                              "params": { "textDocument": { "uri": uri(path) },
                                          "options": options } }),
        );
    }
    let shutdown = files.len() + 1;
    messages.push(json!({ "jsonrpc": "2.0", "id": shutdown, "method": "shutdown" }));
    messages.push(json!({ "jsonrpc": "2.0", "method": "exit" }));

    let output = driver(fixture).stdin(&framed(&messages)).args(["lsp"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let mut edits = Vec::new();
    let mut shown = Vec::new();
    for message in unframed(&stdout(&output)) {
        match message["id"].as_u64() {
            Some(id) if (1..shutdown as u64).contains(&id) => edits.push(message["result"].clone()),
            Some(_) => {}
            None if message["method"] == "window/showMessage" => {
                shown.push(message["params"]["message"].as_str().unwrap().to_string());
            }
            None => {}
        }
    }
    (edits, shown)
}

#[test]
fn the_packages_fmt_lays_out_what_the_server_formats() {
    let fixture = Fixture::new("lsp-fmt");
    fixture.file("astli.toml", "[fmt]\nindent = 4\nexclude = [\"vendor\"]\n");
    let unformatted = "module m;\nlogic q;\nendmodule\n";
    let own = fixture.file("rtl/m.sv", unformatted);
    let vendor = fixture.file("vendor/v.sv", unformatted);

    let (edits, shown) = format(&fixture, &[(&own, unformatted), (&vendor, unformatted)]);

    let indented = json!([{
        "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 2, "character": 0 } },
        "newText": "    logic q;\n",
    }]);
    assert_eq!(edits, [indented, json!([])]);
    assert!(shown.is_empty(), "{shown:?}");
}

#[test]
fn a_manifest_that_does_not_read_is_shown_and_formats_nothing() {
    let fixture = Fixture::new("lsp-fmt-broken");
    fixture.file("astli.toml", "[fmt]\ncolumns = 80\n");
    let text = "module m;\nlogic q;\nendmodule\n";
    let file = fixture.file("m.sv", text);

    let (edits, shown) = format(&fixture, &[(&file, text)]);

    assert_eq!(edits, [json!([])]);
    let manifest = fixture.path().join("astli.toml");
    let shown: Vec<String> = (shown.iter())
        .map(|it| it.replace(&*manifest.to_string_lossy(), "<manifest>"))
        .collect();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(
        shown[0].starts_with("<manifest>: [fmt]: unknown field `columns`"),
        "{shown:?}"
    );
}

#[test]
fn a_package_loads_on_one_thread() {
    let fixture = Fixture::new("lsp-one-thread");
    fixture.file("astli.toml", "");
    let top = "module top;\n  leaf u_leaf ();\nendmodule\n";
    let top = fixture.file("top.sv", top);
    fixture.file("leaf.sv", "module leaf;\nendmodule\n");
    let mut server = driver(&fixture).spawn(["-j1", "lsp"]);
    let mut stdin = server.stdin.take().expect("a pipe to stdin");
    let mut stdout = BufReader::new(server.stdout.take().expect("a pipe from stdout"));
    let (sender, messages) = mpsc::channel();
    std::thread::spawn(move || {
        while let Some(message) = read_framed(&mut stdout) {
            if sender.send(message).is_err() {
                break;
            }
        }
    });
    let document = json!({ "uri": uri(&top), "languageId": "systemverilog", "version": 1,
                           "text": std::fs::read_to_string(&top).unwrap() });
    let opening = framed(&[
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize",
                "params": { "processId": null, "capabilities": {} } }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen",
                "params": { "textDocument": document } }),
    ]);
    stdin.write_all(opening.as_bytes()).expect("written");

    let said = loop {
        let Ok(message) = messages.recv_timeout(Duration::from_secs(30)) else {
            let _ = server.kill();
            panic!("the server loaded nothing");
        };
        let said = message["params"]["message"].as_str().unwrap_or_default();
        if message["method"] == "window/logMessage" && said.starts_with("loaded ") {
            break said.to_string();
        }
    };
    assert!(said.contains(": 2 units, 2 lowered, "), "{said}");

    let closing = framed(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "shutdown" }),
        json!({ "jsonrpc": "2.0", "method": "exit" }),
    ]);
    stdin.write_all(closing.as_bytes()).expect("written");
    drop(stdin);
    assert!(server.wait().expect("the server ends").success());
}

/// The next message the server writes, or `None` once it closes stdout.
fn read_framed(from: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if from.read_line(&mut line).ok()? == 0 {
            return None;
        }
        match line.trim_end() {
            "" => break,
            header => {
                let value = header.strip_prefix("Content-Length: ");
                length = length.or(value.map(|it| it.parse::<usize>().expect("a length")));
            }
        }
    }
    let mut body = vec![0; length.expect("a length")];
    from.read_exact(&mut body).ok()?;
    Some(serde_json::from_slice(&body).expect("a message"))
}
