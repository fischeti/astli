//! Document symbols and folding ranges, asked of the server as a client
//! asks: every `tests/data/symbols/**/*.sv` opened under UTF-16 and held to
//! the `.symbols` and the `.folds` beside it.
//!
//! An outline line is a symbol's kind, name and detail, then its range and
//! its name's, each `line:character` from 1, the character in UTF-16 code
//! units. A fold is its lines, from 1, its kind if it has one, and the text
//! of the line it starts on. Adding a case means writing the `.sv` and
//! running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp --test symbols
//! ```

use std::path::Path;
use std::thread::JoinHandle;
use std::time::Duration;

use astli_lsp::{Ended, Host};
use astli_testing::snapshots;
use expect_test::expect;
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::{DocumentSymbol, FoldingRange, Position, Range};
use serde_json::{Value, json};

/// A client, with the server it started on a thread.
struct Client {
    connection: Connection,
    server: JoinHandle<Ended>,
    next: i32,
}

impl Client {
    /// Starts a server for a client that offers no encoding, so positions
    /// are UTF-16, and has the text document `capabilities`.
    fn start(capabilities: Value) -> Client {
        let (client, server) = Connection::memory();
        let server = std::thread::spawn(move || {
            let host = Host {
                version: "VERSION".to_string(),
                // Neither request formats.
                fmt_options: std::sync::Arc::new(|_| Ok(None)),
            };
            astli_lsp::serve(&server, &host)
        });
        let mut client = Client {
            connection: client,
            server,
            next: 0,
        };
        let params = json!({
            "processId": null,
            "rootUri": null,
            "capabilities": { "textDocument": capabilities },
        });
        client.request("initialize", params);
        client.notify("initialized", json!({}));
        client
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = RequestId::from(self.next);
        let request = Request::new(id.clone(), method.to_string(), params);
        self.connection.sender.send(request.into()).expect("sent");
        loop {
            let message = (self.connection.receiver)
                .recv_timeout(Duration::from_secs(10))
                .expect("an answer");
            if let Message::Response(response) = message {
                assert_eq!(response.id, id, "answers in the order asked");
                return response.response_result.expect("answered");
            }
        }
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = Notification::new(method.to_string(), params);
        let sent = self.connection.sender.send(notification.into());
        sent.expect("sent");
    }

    /// Opens `text` as `name`, returning its URI.
    fn open(&self, name: &str, text: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        let uri = lsp_types::Uri::from_file_path(path).expect("absolute");
        let uri = uri.to_string();
        let document =
            json!({ "uri": uri, "languageId": "systemverilog", "version": 1, "text": text });
        self.notify("textDocument/didOpen", json!({ "textDocument": document }));
        uri
    }

    fn finish(mut self) {
        assert_eq!(self.request("shutdown", Value::Null), Value::Null);
        self.notify("exit", Value::Null);
        assert_eq!(self.server.join().expect("no panic"), Ended::Exited);
    }
}

/// What a client that takes symbols nested and folds whole lines says.
fn capabilities() -> Value {
    json!({
        "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
        "foldingRange": { "lineFoldingOnly": true },
    })
}

fn document(uri: &str) -> Value {
    json!({ "textDocument": { "uri": uri } })
}

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/symbols");
    snapshots::check_with(&root, "symbols", &["folds"], |name, text| {
        let mut client = Client::start(capabilities());
        let uri = client.open(name, &text);
        let symbols = client.request("textDocument/documentSymbol", document(&uri));
        client.finish();
        let symbols: Vec<DocumentSymbol> = serde_json::from_value(symbols).expect("symbols");
        let mut out = String::new();
        outline(&symbols, 0, &mut out);
        out
    });
    snapshots::check_with(&root, "folds", &["symbols"], |name, text| {
        let mut client = Client::start(capabilities());
        let uri = client.open(name, &text);
        let folds = client.request("textDocument/foldingRange", document(&uri));
        client.finish();
        let folds: Vec<FoldingRange> = serde_json::from_value(folds).expect("folds");
        let lines: Vec<&str> = text.lines().collect();
        let mut out = String::new();
        for fold in folds {
            assert!(fold.start_character.is_none() && fold.end_character.is_none());
            let kind = fold.kind.map(|it| format!(" {}", String::from(it)));
            let opens = lines[fold.start_line as usize].trim();
            let (start, end) = (fold.start_line + 1, fold.end_line + 1);
            out += &format!("{start}-{end}{}: {opens}\n", kind.unwrap_or_default());
        }
        out
    });
}

fn outline(symbols: &[DocumentSymbol], depth: usize, out: &mut String) {
    for symbol in symbols {
        let kind = format!("{:?}", symbol.kind).to_lowercase();
        let detail = (symbol.detail.as_ref()).map(|it| format!(" ({it})"));
        out.push_str(&format!(
            "{:indent$}{kind} {}{} {} name {}\n",
            "",
            symbol.name,
            detail.unwrap_or_default(),
            range(symbol.range),
            range(symbol.selection_range),
            indent = depth * 2,
        ));
        outline(
            symbol.children.as_deref().unwrap_or_default(),
            depth + 1,
            out,
        );
    }
}

fn range(range: Range) -> String {
    let at = |it: Position| format!("{}:{}", it.line + 1, it.character + 1);
    format!("{}-{}", at(range.start), at(range.end))
}

/// A client that takes no hierarchy is given the symbols flat, each naming
/// the one it is in; one that folds within a line has a body fold up to its
/// closer, `endmodule`, which stays in view.
#[test]
fn flat_symbols_and_folds_within_a_line() {
    let mut client = Client::start(json!({}));
    let text = "module top;\n  wire /* é */ w;\nendmodule\n";
    let uri = client.open("flat.sv", text);
    let symbols = client.request("textDocument/documentSymbol", document(&uri));
    let folds = client.request("textDocument/foldingRange", document(&uri));
    client.finish();
    let mut symbols = symbols;
    for symbol in symbols.as_array_mut().expect("a list") {
        symbol["location"]["uri"] = json!("URI");
    }
    expect![[r#"
        [
          {
            "kind": 2,
            "location": {
              "range": {
                "end": {
                  "character": 9,
                  "line": 2
                },
                "start": {
                  "character": 0,
                  "line": 0
                }
              },
              "uri": "URI"
            },
            "name": "top"
          },
          {
            "containerName": "top",
            "kind": 13,
            "location": {
              "range": {
                "end": {
                  "character": 17,
                  "line": 1
                },
                "start": {
                  "character": 2,
                  "line": 1
                }
              },
              "uri": "URI"
            },
            "name": "w"
          }
        ]"#]]
    .assert_eq(&serde_json::to_string_pretty(&symbols).unwrap());
    expect![[r#"
        [
          {
            "endCharacter": 0,
            "endLine": 2,
            "startLine": 0
          }
        ]"#]]
    .assert_eq(&serde_json::to_string_pretty(&folds).unwrap());
}
