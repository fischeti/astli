//! `astli lsp` as a client starts it, over stdin and stdout: what it reads
//! of the package a file is in, and what it publishes of an open file.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin};
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

/// A server run in a fixture, which a test talks to while it runs.
struct Server {
    child: Child,
    stdin: ChildStdin,
    messages: mpsc::Receiver<Value>,
    /// Each publish's params, in order, but what the test took.
    published: Vec<Value>,
    next: u64,
}

impl Server {
    /// Starts `astli` with `args` in `fixture`, and sends `initialize` with
    /// `capabilities`.
    fn start(fixture: &Fixture, args: &[&str], capabilities: Value) -> Server {
        let mut child = driver(fixture).spawn(args);
        let stdin = child.stdin.take().expect("a pipe to stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("a pipe from stdout"));
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            while let Some(message) = read_framed(&mut stdout) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        let mut server = Server {
            child,
            stdin,
            messages,
            published: Vec::new(),
            next: 1,
        };
        server.send(&[
            json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize",
                    "params": { "processId": null, "capabilities": capabilities } }),
            json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        ]);
        server
    }

    fn send(&mut self, messages: &[Value]) {
        self.stdin
            .write_all(framed(messages).as_bytes())
            .expect("written");
    }

    /// Opens the file at `path` as the disk holds it.
    fn open(&mut self, path: &Path) {
        let text = std::fs::read_to_string(path).expect("a file to open");
        let document =
            json!({ "uri": uri(path), "languageId": "systemverilog", "version": 1, "text": text });
        self.send(
            &[json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen",
                            "params": { "textDocument": document } })],
        );
    }

    /// Inserts `text` at the start of the file at `path`, open, which makes
    /// it `version`.
    fn insert(&mut self, path: &Path, version: i32, text: &str) {
        let start = json!({ "line": 0, "character": 0 });
        let change = json!({ "range": { "start": start, "end": start }, "text": text });
        let document = json!({ "uri": uri(path), "version": version });
        self.send(&[
            json!({ "jsonrpc": "2.0", "method": "textDocument/didChange",
                      "params": { "textDocument": document, "contentChanges": [change] } }),
        ]);
    }

    /// Waits for the next message `wanted` takes, keeping each publish
    /// before it.
    fn next(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        loop {
            let Ok(message) = self.messages.recv_timeout(Duration::from_secs(30)) else {
                let _ = self.child.kill();
                panic!(
                    "the server sent nothing wanted; it published {:#?}",
                    self.published
                );
            };
            if wanted(&message) {
                return message;
            }
            if message["method"] == "textDocument/publishDiagnostics" {
                self.published.push(message["params"].clone());
            }
        }
    }

    /// Waits for the server to log what `said` takes.
    fn logged(&mut self, said: impl Fn(&str) -> bool) -> String {
        let message = self.next(|it| {
            it["method"] == "window/logMessage"
                && said(it["params"]["message"].as_str().unwrap_or_default())
        });
        message["params"]["message"].as_str().unwrap().to_string()
    }

    /// The diagnostics last published for the file at `path` before the
    /// server answers a request sent now, if any were.
    fn latest(&mut self, path: &Path) -> Option<Vec<Value>> {
        let id = self.next;
        self.next += 1;
        self.send(&[json!({ "jsonrpc": "2.0", "id": id, "method": "astli/unknown" })]);
        self.next(|it| it["id"] == id && it.get("method").is_none());
        let uri = uri(path);
        let last = self.published.iter().rev().find(|it| it["uri"] == uri);
        last.map(|it| it["diagnostics"].as_array().expect("a list").clone())
    }

    /// Waits for a load of the design to be swapped in, and hands back the
    /// diagnostics last published for the file at `path` by then.
    fn loaded(&mut self, path: &Path) -> Vec<Value> {
        self.logged(|it| it.starts_with("loaded "));
        self.latest(path).expect("a publish for the file")
    }

    /// Shuts the server down and waits for it to exit cleanly.
    fn finish(mut self) {
        let id = self.next;
        self.send(&[
            json!({ "jsonrpc": "2.0", "id": id, "method": "shutdown" }),
            json!({ "jsonrpc": "2.0", "method": "exit" }),
        ]);
        drop(self.stdin);
        assert!(self.child.wait().expect("the server ends").success());
    }
}

/// The diagnostic with `code` among `found`, which has one.
fn with<'a>(found: &'a [Value], code: &str) -> &'a Value {
    let diagnostic = found.iter().find(|it| it["code"] == code);
    diagnostic.unwrap_or_else(|| panic!("no `{code}` in {found:#?}"))
}

/// The codes of `found`, in order.
fn codes(found: &[Value]) -> Vec<&str> {
    found
        .iter()
        .map(|it| it["code"].as_str().unwrap_or_default())
        .collect()
}

/// Where `diagnostic` starts, as a line and a character counted from zero.
fn start(diagnostic: &Value) -> (u64, u64) {
    let start = &diagnostic["range"]["start"];
    (
        start["line"].as_u64().unwrap(),
        start["character"].as_u64().unwrap(),
    )
}

#[test]
fn a_package_loads_on_one_thread() {
    let fixture = Fixture::new("lsp-one-thread");
    fixture.file("astli.toml", "");
    let top = "module top;\n  leaf u_leaf ();\nendmodule\n";
    let top = fixture.file("top.sv", top);
    fixture.file("leaf.sv", "module leaf;\nendmodule\n");
    let mut server = Server::start(&fixture, &["-j1", "lsp"], json!({}));
    server.open(&top);
    let said = server.logged(|it| it.starts_with("loaded "));
    assert!(said.contains(": 2 units, 2 lowered, "), "{said}");
    server.finish();
}

#[test]
fn what_check_and_lint_find_is_published_for_an_open_file() {
    let fixture = Fixture::new("lsp-diagnostics");
    fixture.file("astli.toml", "");
    let text = "module m;\n  reg r;\n  wire p = nowhere;\nendmodule\n";
    let file = fixture.file("m.sv", text);
    let mut server = Server::start(&fixture, &["lsp"], json!({}));
    server.open(&file);
    let found = server.loaded(&file);
    // `check`'s, a tree rule's and a design rule's, each with its code, and
    // a rule's with the page that lists it.
    let undeclared = with(&found, "undeclared-name");
    assert_eq!(
        (start(undeclared), &undeclared["severity"]),
        ((2, 11), &json!(1))
    );
    assert_eq!(undeclared["source"], "astli");
    let reg = with(&found, "forbid-reg");
    assert_eq!((start(reg), &reg["severity"]), ((1, 2), &json!(2)));
    let page = "https://astli.dev/reference/lint-rules/#lowrisc";
    assert_eq!(reg["codeDescription"]["href"], page);
    let unused = with(&found, "unused-signal");
    assert_eq!(start(unused), (1, 6));
    server.finish();
}

#[test]
fn a_finding_a_macro_wrote_is_at_the_outermost_call() {
    let fixture = Fixture::new("lsp-macro");
    fixture.file("astli.toml", "");
    let text = "`define READ(x) (x + nowhere)\n\
                `define TWICE(x) `READ(x)\n\
                module m;\n  wire w = 1'b0;\n  wire p = `TWICE(w);\nendmodule\n";
    let file = fixture.file("m.sv", text);
    let capabilities =
        json!({ "textDocument": { "publishDiagnostics": { "relatedInformation": true } } });
    let mut server = Server::start(&fixture, &["lsp"], capabilities);
    server.open(&file);
    let found = server.loaded(&file);
    let undeclared = with(&found, "undeclared-name");
    assert_eq!(start(undeclared), (4, 11));
    let related: Vec<(&str, (u64, u64))> = (undeclared["relatedInformation"].as_array())
        .expect("related information")
        .iter()
        .map(|it| {
            let start = &it["location"]["range"]["start"];
            let at = (
                start["line"].as_u64().unwrap(),
                start["character"].as_u64().unwrap(),
            );
            assert_eq!(it["location"]["uri"], uri(&file));
            (it["message"].as_str().unwrap(), at)
        })
        .collect();
    assert_eq!(
        related,
        [
            ("this is the text it stands for", (0, 21)),
            ("in this expansion of `READ", (1, 17)),
        ]
    );
    server.finish();
}

#[test]
fn a_place_after_a_character_of_two_utf16_units_counts_them() {
    let fixture = Fixture::new("lsp-utf16");
    fixture.file("astli.toml", "");
    let text = "module m;\n  wire p = /* \u{1D11E}\u{e9} */ nowhere;\nendmodule\n";
    let file = fixture.file("m.sv", text);
    // A client that offers no encoding counts UTF-16 units: the comment's
    // first character is two of them, and its second one.
    let mut server = Server::start(&fixture, &["lsp"], json!({}));
    server.open(&file);
    let found = server.loaded(&file);
    assert_eq!(start(with(&found, "undeclared-name")), (1, 21));
    server.finish();
}

#[test]
fn a_waiver_and_the_manifests_levels_are_honoured() {
    let fixture = Fixture::new("lsp-levels");
    let manifest = "[lint]\ndeny = ['forbid-reg']\nallow = ['undriven-signal']\n";
    fixture.file("astli.toml", manifest);
    let text = "module m;\n  (* astli_allow = \"forbid-reg, unused-signal\" *)\n  reg r;\n\
                \x20 reg s;\n  logic t;\nendmodule\n";
    let file = fixture.file("m.sv", text);
    let mut server = Server::start(&fixture, &["lsp"], json!({}));
    server.open(&file);
    let found = server.loaded(&file);
    // `r` waives both rules, the tree's and the design's; `s` waives
    // neither, and the manifest makes `forbid-reg` an error.
    let reg: Vec<_> = (found.iter())
        .filter(|it| it["code"] == "forbid-reg")
        .map(|it| (start(it), &it["severity"]))
        .collect();
    assert_eq!(reg, [((3, 2), &json!(1))]);
    let unused: Vec<_> = (found.iter())
        .filter(|it| it["code"] == "unused-signal")
        .map(start)
        .collect();
    assert_eq!(unused, [(3, 6), (4, 8)]);
    assert!(!codes(&found).contains(&"undriven-signal"), "{found:#?}");
    server.finish();
}

#[test]
fn a_dependency_the_store_lacks_is_published_at_the_manifest_and_holds_the_design_back() {
    let fixture = Fixture::new("lsp-unfetched");
    let git = "[package]\nname = 'm'\n\n\
               [dependencies]\nip = { git = 'https://example.com/ip.git', version = '1' }\n";
    let manifest = fixture.file("astli.toml", git);
    let file = fixture.file("m.sv", "module m;\n  ip u_ip ();\n  reg r;\nendmodule\n");
    let mut server = Server::start(&fixture, &["lsp"], json!({}));
    server.open(&file);
    let found = server.loaded(&file);
    // What the buffer says on its own, and nothing the missing dependency
    // would explain.
    assert!(codes(&found).contains(&"forbid-reg"), "{found:#?}");
    assert!(!codes(&found).contains(&"unknown-definition"), "{found:#?}");
    let at_manifest = server
        .latest(&manifest)
        .expect("a publish for the manifest");
    let [unread] = &at_manifest[..] else {
        panic!("{at_manifest:#?}");
    };
    assert_eq!(unread["code"], "unread-dependencies");
    assert_eq!(start(unread), (3, 0));
    let message = unread["message"].as_str().unwrap();
    assert!(message.contains("`ip`"), "{message}");
    assert!(
        message.ends_with(
            "\nnote: the package is read without its dependencies\n\
             help: `astli fetch` settles and fetches them"
        ),
        "{message}"
    );
    server.finish();
}

#[test]
fn what_the_tops_build_gets_wrong_is_published_with_the_built_rules() {
    let fixture = Fixture::new("lsp-tops");
    fixture.file("astli.toml", "[top.top]\n");
    let text = "module top;\n  logic [3:0] w;\n  logic [7:0] v;\n  assign v = 8'd3;\n\
                \x20 assign w = v;\n  assign w = 4'd1;\nendmodule\n";
    let file = fixture.file("top.sv", text);
    let mut server = Server::start(&fixture, &["lsp"], json!({}));
    server.open(&file);
    let found = server.loaded(&file);
    // Driven twice in what the top builds, an error; outside it, `check`
    // of the unit alone would call it a warning.
    let drivers = with(&found, "multiple-drivers");
    assert_eq!((start(drivers), &drivers["severity"]), ((5, 9), &json!(1)));
    // A built rule, which reads the widths only what is built has.
    assert_eq!(start(with(&found, "width-trunc")), (4, 13));
    server.finish();
}

#[test]
fn a_name_declared_twice_is_placed_in_the_buffer_as_it_is_edited() {
    let fixture = Fixture::new("lsp-declared-twice");
    fixture.file("astli.toml", "");
    let file = fixture.file("a.sv", "module twice;\nendmodule\n");
    fixture.file("b.sv", "module twice;\nendmodule\n");
    let mut server = Server::start(&fixture, &["lsp"], json!({}));
    server.open(&file);
    let found = server.loaded(&file);
    assert_eq!(start(with(&found, "declared-twice")), (0, 7));
    // Five lines above it: where the load placed it is no longer where it
    // is, until the design is loaded again.
    server.insert(&file, 2, "\n\n\n\n\n");
    let changed = server.latest(&file).expect("a publish for the change");
    let twice = changed.iter().find(|it| it["code"] == "declared-twice");
    assert!(twice.is_none_or(|it| start(it) != (0, 7)), "{changed:#?}");
    let found = server.loaded(&file);
    assert_eq!(start(with(&found, "declared-twice")), (5, 7));
    server.finish();
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
