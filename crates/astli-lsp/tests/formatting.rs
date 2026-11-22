//! `textDocument/formatting`, driven as a client drives it over an
//! in-memory connection: the edits it answers with, applied, and what it
//! tells the user when it gives none.

use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use astli_fmt::Options;
use astli_lsp::{Ended, FmtOptions, Host};
use astli_parse::SyntaxTree;
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

/// A client, with the server it started running its loop on a thread.
struct Client {
    connection: Connection,
    server: JoinHandle<Ended>,
    next: i32,
    /// Whether positions count UTF-16 code units rather than bytes.
    utf16: bool,
    /// What the server has shown the user, in order.
    shown: Vec<String>,
}

impl Client {
    /// Starts a server whose host answers the layout with `fmt_options`,
    /// offering UTF-8 positions unless `utf16`.
    fn start(fmt_options: FmtOptions, utf16: bool) -> Client {
        let (client, server) = Connection::memory();
        let host = Host {
            version: "VERSION".to_string(),
            fmt_options,
        };
        let server = std::thread::spawn(move || astli_lsp::serve(&server, &host));
        let mut client = Client {
            connection: client,
            server,
            next: 0,
            utf16,
            shown: Vec::new(),
        };
        let encodings: &[&str] = if utf16 { &[] } else { &["utf-8"] };
        let general = json!({ "positionEncodings": encodings });
        let params = json!({ "processId": null, "capabilities": { "general": general } });
        let result = client.request("initialize", params);
        assert_eq!(result["capabilities"]["documentFormattingProvider"], true);
        client.notify("initialized", json!({}));
        client
    }

    /// Starts a server formatting every file with `options`.
    fn with(options: Options, utf16: bool) -> Client {
        Client::start(Arc::new(move |_| Ok(Some(options))), utf16)
    }

    /// Sends a request and waits for its answer, which must not be an error.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = RequestId::from(self.next);
        let request = Request::new(id.clone(), method.to_string(), params);
        self.connection.sender.send(request.into()).expect("sent");
        loop {
            let message = (self.connection.receiver)
                .recv_timeout(Duration::from_secs(10))
                .expect("an answer");
            match message {
                Message::Response(response) => {
                    assert_eq!(response.id, id, "answers in the order asked");
                    return response.response_result.expect("no error");
                }
                Message::Notification(it) if it.method == "window/showMessage" => {
                    let message = it.params["message"].as_str().expect("a message");
                    self.shown.push(message.to_string());
                }
                Message::Notification(_) => {}
                Message::Request(request) => panic!("the server asked {request:?}"),
            }
        }
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = Notification::new(method.to_string(), params);
        self.connection
            .sender
            .send(notification.into())
            .expect("sent");
    }

    fn open(&self, uri: &str, text: &str) {
        let document =
            json!({ "uri": uri, "languageId": "systemverilog", "version": 1, "text": text });
        self.notify("textDocument/didOpen", json!({ "textDocument": document }));
    }

    /// The edits the server formats `uri` with.
    fn format(&mut self, uri: &str) -> Vec<Value> {
        // Options the server ignores, since `astli.toml` is the configuration.
        let options = json!({ "tabSize": 8, "insertSpaces": false });
        let params = json!({ "textDocument": { "uri": uri }, "options": options });
        let edits = self.request("textDocument/formatting", params);
        serde_json::from_value(edits).expect("a list of edits")
    }

    /// `text` with `edits` applied, each range read in `text`, as the
    /// protocol has them.
    fn apply(&self, text: &str, edits: &[Value]) -> String {
        let mut ranges: Vec<(usize, usize, &str)> = (edits.iter())
            .map(|edit| {
                let range = &edit["range"];
                let start = self.offset(text, &range["start"]);
                let end = self.offset(text, &range["end"]);
                (start, end, edit["newText"].as_str().expect("text"))
            })
            .collect();
        ranges.sort_by_key(|&(start, ..)| start);
        let mut out = text.to_string();
        for &(start, end, new) in ranges.iter().rev() {
            out.replace_range(start..end, new);
        }
        out
    }

    /// The byte offset in `text` of a protocol position.
    fn offset(&self, text: &str, position: &Value) -> usize {
        let line = position["line"].as_u64().expect("a line") as usize;
        let character = position["character"].as_u64().expect("a character") as usize;
        let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        let rest = &text[start..];
        start + self.within(&rest[..rest.find('\n').unwrap_or(rest.len())], character)
    }

    /// The byte offset of `character` units into `line`.
    fn within(&self, line: &str, character: usize) -> usize {
        let mut counted = 0;
        for (at, char) in line.char_indices() {
            if counted >= character {
                return at;
            }
            counted += if self.utf16 {
                char.len_utf16()
            } else {
                char.len_utf8()
            };
        }
        line.len()
    }

    fn finish(mut self) {
        assert_eq!(self.request("shutdown", Value::Null), Value::Null);
        self.notify("exit", Value::Null);
        assert_eq!(self.server.join().expect("no panic"), Ended::Exited);
    }
}

/// The URI of `name` in a directory that exists on every platform.
fn uri(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    lsp_types::Uri::from_file_path(path)
        .expect("a file URI")
        .to_string()
}

/// What `astli fmt` writes for `text` under `options`.
fn formatted(text: &str, options: &Options) -> String {
    let tree = SyntaxTree::parse("t.sv", text.to_string());
    astli_fmt::format(&tree, options).expect("formatted")
}

const UNFORMATTED: &str = "\
module m;
  logic a;
     logic   b;
  logic c;
assign a=b;
endmodule
";

#[test]
fn edits_replace_only_the_lines_that_differ() {
    for utf16 in [false, true] {
        let mut client = Client::with(Options::default(), utf16);
        let uri = uri("m.sv");
        client.open(&uri, UNFORMATTED);
        let edits = client.format(&uri);
        let expected = formatted(UNFORMATTED, &Options::default());
        assert_eq!(client.apply(UNFORMATTED, &edits), expected);
        // Lines 3 and 5 differ; the ones between and around are kept.
        let lines: Vec<u64> = (edits.iter())
            .map(|it| it["range"]["start"]["line"].as_u64().unwrap())
            .collect();
        assert_eq!(lines, [2, 4]);

        // Formatted, the buffer needs nothing more.
        let document = json!({ "uri": uri, "version": 2 });
        let change = json!({ "text": expected });
        let params = json!({ "textDocument": document, "contentChanges": [change] });
        client.notify("textDocument/didChange", params);
        assert_eq!(client.format(&uri), Vec::<Value>::new());
        assert!(client.shown.is_empty(), "{:?}", client.shown);
        client.finish();
    }
}

#[test]
fn a_line_of_characters_past_ascii_is_counted_in_the_negotiated_encoding() {
    // The last line has no newline, so the edit adding it ends inside the
    // line, past characters of two, three and four bytes.
    let text = "module m;\n  logic a;  // ä € 😀\nendmodule // ä € 😀  ";
    for utf16 in [false, true] {
        let mut client = Client::with(Options::default(), utf16);
        let uri = uri("wide.sv");
        client.open(&uri, text);
        let edits = client.format(&uri);
        assert_eq!(
            client.apply(text, &edits),
            formatted(text, &Options::default())
        );
        let end = edits.last().expect("an edit")["range"]["end"].clone();
        let character = if utf16 { 21 } else { 26 };
        assert_eq!(end, json!({ "line": 2, "character": character }));
        client.finish();
    }
}

#[test]
fn the_hosts_layout_is_the_one_formatted_with() {
    let indented = Options {
        indent: 4,
        ..Options::default()
    };
    let mut client = Client::with(indented, false);
    let uri = uri("m.sv");
    client.open(&uri, UNFORMATTED);
    let edits = client.format(&uri);
    let expected = formatted(UNFORMATTED, &indented);
    assert!(expected.contains("\n    logic a;"), "{expected}");
    assert_eq!(client.apply(UNFORMATTED, &edits), expected);
    client.finish();
}

#[test]
fn a_file_given_no_edit_says_why_where_the_user_would_want_to_know() {
    let fmt_options: FmtOptions = Arc::new(|path| match path.file_name() {
        Some(name) if name == "broken.sv" => {
            Err("astli.toml: [fmt]: unknown field `columns`".into())
        }
        Some(name) if name == "excluded.sv" => Ok(None),
        _ => Ok(Some(Options::default())),
    });
    let mut client = Client::start(fmt_options, false);

    // An unreadable manifest is shown; an excluded file is left quietly.
    for name in ["broken.sv", "excluded.sv"] {
        let uri = uri(name);
        client.open(&uri, UNFORMATTED);
        assert_eq!(client.format(&uri), Vec::<Value>::new(), "{name}");
    }
    // A file that is not open has no buffer to format.
    assert_eq!(client.format(&uri("closed.sv")), Vec::<Value>::new());
    assert_eq!(client.shown, ["astli.toml: [fmt]: unknown field `columns`"]);
    client.finish();
}
