//! The server's loop, driven as a client drives it, over an in-memory
//! connection: the handshake, the buffers, cancellation, panics and exit.

use std::path::Path;
use std::thread::JoinHandle;
use std::time::Duration;

use astli_text::{Lines, Reader};
use expect_test::expect;
use lsp_server::{Connection, Message, Notification, Request, RequestId, ResponseError};
use lsp_types::Position;
use serde_json::{Value, json};

use crate::buffers::Buffers;
use crate::position::Encoding;
use crate::{Ended, Host};

/// What the tests' server says of itself.
fn host() -> Host {
    Host {
        version: "VERSION".to_string(),
    }
}

/// What a request is answered with.
type Answer = Result<Value, ResponseError>;

/// A client, with the server it started running its loop on a thread.
struct Client {
    connection: Connection,
    server: JoinHandle<Ended>,
    next: i32,
    /// What the server has told the client's log, in order.
    logs: Vec<String>,
}

impl Client {
    /// Starts a server, and sends `initialize` offering `encodings`, or
    /// saying nothing of them with `None`. Returns the client and what the
    /// server answered.
    fn initialize(encodings: Option<&[&str]>) -> (Client, Value) {
        let (client, server) = Connection::memory();
        let server = std::thread::spawn(move || super::serve(&server, &host()));
        let mut client = Client {
            connection: client,
            server,
            next: 0,
            logs: Vec::new(),
        };
        let general = match encodings {
            Some(encodings) => json!({ "positionEncodings": encodings }),
            None => json!({}),
        };
        let params =
            json!({ "processId": null, "rootUri": null, "capabilities": { "general": general } });
        let result = client.request("initialize", params).expect("initialized");
        client.notify("initialized", json!({}));
        (client, result)
    }

    /// Starts a server and completes the handshake in `encoding`.
    fn start(encoding: Encoding) -> Client {
        let offered: &[&str] = match encoding {
            Encoding::Utf8 => &["utf-8"],
            Encoding::Utf16 => &[],
        };
        Client::initialize(Some(offered)).0
    }

    /// Sends a request, returning its id without waiting for the answer.
    fn send(&mut self, method: &str, params: Value) -> RequestId {
        self.next += 1;
        let id = RequestId::from(self.next);
        let request = Request::new(id.clone(), method.to_string(), params);
        self.connection.sender.send(request.into()).expect("sent");
        id
    }

    /// Sends a request and waits for its answer.
    fn request(&mut self, method: &str, params: Value) -> Answer {
        let id = self.send(method, params);
        self.answer(&id)
    }

    /// Waits for the answer to request `id`, keeping what the server logs
    /// on the way.
    fn answer(&mut self, id: &RequestId) -> Answer {
        loop {
            let message = (self.connection.receiver)
                .recv_timeout(Duration::from_secs(10))
                .expect("an answer");
            match message {
                Message::Response(response) => {
                    assert_eq!(&response.id, id, "answers in the order asked");
                    return response.response_result;
                }
                Message::Notification(notification) => self.logged(notification),
                Message::Request(request) => panic!("the server asked {request:?}"),
            }
        }
    }

    fn logged(&mut self, notification: Notification) {
        assert_eq!(notification.method, "window/logMessage");
        let message = notification.params["message"].as_str().expect("a message");
        self.logs.push(message.to_string());
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = Notification::new(method.to_string(), params);
        self.connection
            .sender
            .send(notification.into())
            .expect("sent");
    }

    /// The version and text the server holds for `uri`, if it is open.
    fn buffer(&mut self, uri: &str) -> Option<(i32, String)> {
        let held = self.request("test/buffer", json!(uri)).expect("answered");
        serde_json::from_value(held).expect("a buffer")
    }

    /// Sends `exit`, and returns how the server ended.
    fn exit(self) -> Ended {
        self.notify("exit", Value::Null);
        self.server.join().expect("the loop does not panic")
    }

    /// Shuts the server down and exits it, cleanly.
    fn finish(mut self) {
        assert_eq!(self.request("shutdown", Value::Null).unwrap(), Value::Null);
        assert_eq!(self.exit(), Ended::Exited);
    }
}

/// The URI of `name` in a directory that exists on every platform: a path
/// with no drive is no file on Windows.
fn uri(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    lsp_types::Uri::from_file_path(path)
        .expect("absolute")
        .to_string()
}

/// An error's code, which is what a client acts on.
fn code(result: Answer) -> i32 {
    result.expect_err("an error").code
}

#[test]
fn initialize_negotiates_utf8_where_the_client_offers_it() {
    let rows: &[(Option<&[&str]>, &str)] = &[
        (None, "utf-16"),
        (Some(&[]), "utf-16"),
        (Some(&["utf-16"]), "utf-16"),
        (Some(&["utf-32"]), "utf-16"),
        (Some(&["utf-8"]), "utf-8"),
        (Some(&["utf-16", "utf-8"]), "utf-8"),
    ];
    for &(offered, chosen) in rows {
        let (client, result) = Client::initialize(offered);
        let announced = &result["capabilities"]["positionEncoding"];
        assert_eq!(announced, chosen, "offered {offered:?}");
        client.finish();
    }

    // Everything else announced is the documents' sync.
    let (client, result) = Client::initialize(Some(&["utf-8"]));
    expect![[r#"
        {
          "capabilities": {
            "positionEncoding": "utf-8",
            "textDocumentSync": {
              "change": 2,
              "openClose": true
            }
          },
          "serverInfo": {
            "name": "astli",
            "version": "VERSION"
          }
        }"#]]
    .assert_eq(&serde_json::to_string_pretty(&result).unwrap());
    client.finish();
}

#[test]
fn a_request_before_initialize_is_refused() {
    let (client, server) = Connection::memory();
    let server = std::thread::spawn(move || super::serve(&server, &host()));
    let request = Request::new(RequestId::from(1), "shutdown".to_string(), Value::Null);
    client.sender.send(request.into()).unwrap();
    let Ok(Message::Response(response)) = client.receiver.recv() else {
        panic!("an answer")
    };
    assert_eq!(response.response_result.unwrap_err().code, -32002);
    drop(client);
    assert!(
        matches!(server.join().unwrap(), Ended::Failed(_)),
        "no initialize, no clean end"
    );
}

#[test]
fn changes_edit_the_buffer_in_the_negotiated_encoding() {
    use Encoding::{Utf8, Utf16};
    // Each row opens `text`, sends one change of each edit, a range of
    // `[line, character]` pairs and what replaces it, and finds `edited`.
    type Edit = ([u32; 4], &'static str);
    let rows: &[(Encoding, &str, &[Edit], &str)] = &[
        (Utf16, "wire é;\n", &[([0, 5, 0, 6], "q")], "wire q;\n"),
        (Utf8, "wire é;\n", &[([0, 5, 0, 7], "q")], "wire q;\n"),
        // U+1D11E is two UTF-16 code units, and four bytes.
        (Utf16, "// 𝄞 x\n", &[([0, 6, 0, 7], "y")], "// 𝄞 y\n"),
        (Utf16, "// 𝄞 x\n", &[([0, 3, 0, 5], "")], "//  x\n"),
        (Utf8, "// 𝄞 x\n", &[([0, 3, 0, 7], "")], "//  x\n"),
        (Utf8, "// 𝄞 x\n", &[([0, 8, 0, 9], "y")], "// 𝄞 y\n"),
        // A position inside a character is that character's start.
        (Utf16, "𝄞", &[([0, 1, 0, 1], "x")], "x𝄞"),
        (Utf8, "é", &[([0, 1, 0, 1], "x")], "xé"),
        (Utf16, "a\nb\nc\n", &[([0, 1, 2, 0], "")], "ac\n"),
        // Past the end of a line is its end, before a `\r\n` too, and past
        // the last line the end of the text.
        (Utf16, "ab\ncd\n", &[([0, 10, 0, 10], "!")], "ab!\ncd\n"),
        (
            Utf16,
            "ab\r\ncd\r\n",
            &[([0, 5, 0, 5], "!")],
            "ab!\r\ncd\r\n",
        ),
        (Utf16, "ab\ncd\n", &[([9, 0, 9, 4], "x")], "ab\ncd\nx"),
        (Utf16, "ab\ncd", &[([1, 9, 1, 9], "\n")], "ab\ncd\n"),
        // A range whose end is before its start is empty.
        (Utf16, "abc", &[([0, 2, 0, 1], "x")], "abxc"),
        // Each change reads the text the one before it left.
        (
            Utf16,
            "ab",
            &[([0, 0, 0, 0], "x"), ([0, 1, 0, 1], "y")],
            "xyab",
        ),
        (
            Utf16,
            "é",
            &[([0, 0, 0, 1], "𝄞"), ([0, 2, 0, 2], "!")],
            "𝄞!",
        ),
        (
            Utf16,
            "",
            &[([0, 0, 0, 0], "a\n"), ([1, 0, 1, 0], "b")],
            "a\nb",
        ),
    ];
    for (row, &(encoding, text, edits, edited)) in rows.iter().enumerate() {
        let mut client = Client::start(encoding);
        let uri = uri(&format!("row{row}.sv"));
        let document =
            json!({ "uri": uri, "languageId": "systemverilog", "version": 1, "text": text });
        client.notify("textDocument/didOpen", json!({ "textDocument": document }));
        let changes: Vec<Value> = (edits.iter())
            .map(|([l0, c0, l1, c1], text)| {
                let range = json!({ "start": { "line": l0, "character": c0 }, "end": { "line": l1, "character": c1 } });
                json!({ "range": range, "text": text })
            })
            .collect();
        let document = json!({ "uri": uri, "version": 7 });
        let params = json!({ "textDocument": document, "contentChanges": changes });
        client.notify("textDocument/didChange", params);
        let held = client.buffer(&uri);
        assert_eq!(held, Some((7, edited.to_string())), "row {row}: {text:?}");
        assert!(client.logs.is_empty(), "row {row}: {:?}", client.logs);
        client.finish();
    }
}

#[test]
fn documents_are_held_from_open_to_close() {
    let mut client = Client::start(Encoding::Utf16);
    let open = |client: &Client, uri: &str, text: &str| {
        let document =
            json!({ "uri": uri, "languageId": "systemverilog", "version": 1, "text": text });
        client.notify("textDocument/didOpen", json!({ "textDocument": document }));
    };

    // A space in a path is escaped in its URI.
    let uri = &uri("a b.sv");
    assert!(uri.ends_with("a%20b.sv"));
    open(&client, uri, "module a;\n");
    assert_eq!(client.buffer(uri), Some((1, "module a;\n".to_string())));
    let whole = json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": "module b;\n" }] });
    client.notify("textDocument/didChange", whole);
    assert_eq!(client.buffer(uri), Some((2, "module b;\n".to_string())));
    client.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert_eq!(client.buffer(uri), None);

    // A buffer no file is behind reads as nothing.
    open(&client, "untitled:Untitled-1", "module c;\n");
    assert_eq!(client.buffer("untitled:Untitled-1"), None);

    // A change to a file that is not open is told, and changes nothing.
    let change = json!({ "textDocument": { "uri": uri, "version": 3 }, "contentChanges": [{ "text": "x" }] });
    client.notify("textDocument/didChange", change);
    assert_eq!(client.buffer(uri), None);
    assert_eq!(client.logs, [format!("{uri} changed, and is not open")]);

    // So are params that are not a notification's.
    client.notify("textDocument/didOpen", json!({ "textDocument": 1 }));
    assert_eq!(client.buffer(uri), None);
    assert!(client.logs[1].starts_with("`textDocument/didOpen`: invalid type"));
    client.finish();
}

#[test]
fn buffers_are_read_before_the_disk() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("missing.sv");
    let mut buffers = Buffers::default();
    assert!(buffers.exists(&manifest) && !buffers.exists(&missing));
    let on_disk = buffers.read(&manifest).unwrap();

    buffers.open(&manifest, "edited".to_string(), 1);
    buffers.open(&missing, "unsaved".to_string(), 1);
    assert_eq!(buffers.read(&manifest).unwrap(), "edited");
    assert_eq!(buffers.read(&missing).unwrap(), "unsaved");
    // Spelled another way, the path is the same file.
    let spelled = manifest.parent().unwrap().join("src/../missing.sv");
    assert_eq!(buffers.read(&spelled).unwrap(), "unsaved");

    buffers.close(&manifest);
    buffers.close(&missing);
    assert_eq!(buffers.read(&manifest).unwrap(), on_disk);
    assert!(!buffers.exists(&missing));
}

#[test]
fn positions_convert_to_offsets_and_back() {
    use Encoding::{Utf8, Utf16};
    let text = "a\n// 𝄞é x\r\n\nend";
    let lines = Lines::new(text);
    // Each row is an offset and its position, which convert both ways.
    let rows: &[(Encoding, u32, [u32; 2])] = &[
        (Utf16, 0, [0, 0]),
        (Utf16, 1, [0, 1]),
        (Utf16, 2, [1, 0]),
        (Utf16, 5, [1, 3]),
        (Utf16, 9, [1, 5]),
        (Utf16, 11, [1, 6]),
        (Utf16, 13, [1, 8]),
        (Utf16, 15, [2, 0]),
        (Utf16, 16, [3, 0]),
        (Utf16, 19, [3, 3]),
        (Utf8, 5, [1, 3]),
        (Utf8, 9, [1, 7]),
        (Utf8, 11, [1, 9]),
        (Utf8, 13, [1, 11]),
        (Utf8, 19, [3, 3]),
    ];
    for &(encoding, offset, [line, character]) in rows {
        let position = Position { line, character };
        let row = format!("{encoding:?} {offset}");
        assert_eq!(encoding.position(text, &lines, offset), position, "{row}");
        assert_eq!(encoding.offset(text, &lines, position), offset, "{row}");
    }

    // A position past its line, or inside a character, comes back clamped.
    let clamped: &[(Encoding, [u32; 2], u32)] = &[
        (Utf16, [0, 9], 1),
        (Utf16, [1, 4], 5),
        (Utf16, [1, 99], 13),
        (Utf8, [1, 5], 5),
        (Utf8, [1, 99], 13),
        (Utf16, [2, 1], 15),
        (Utf16, [3, 9], 19),
        (Utf16, [4, 0], 19),
        (Utf8, [u32::MAX, 0], 19),
    ];
    for &(encoding, [line, character], offset) in clamped {
        let position = Position { line, character };
        let row = format!("{encoding:?} {position:?}");
        assert_eq!(encoding.offset(text, &lines, position), offset, "{row}");
    }

    // So does an offset past the end, or inside a character.
    assert_eq!(
        Utf16.position(text, &lines, 99),
        Position {
            line: 3,
            character: 3
        }
    );
    assert_eq!(
        Utf16.position(text, &lines, 7),
        Position {
            line: 1,
            character: 3
        }
    );
    assert_eq!(
        Utf8.position(text, &lines, 10),
        Position {
            line: 1,
            character: 7
        }
    );

    let range = Utf16.range(text, &lines, 5..11);
    assert_eq!(Utf16.bytes(text, &lines, range), 5..11);
    assert_eq!(
        Utf8.range("", &Lines::new(""), 0..0).end,
        Position::default()
    );
}

#[test]
fn requests_the_server_does_not_know_are_answered_as_unknown() {
    let mut client = Client::start(Encoding::Utf16);
    assert_eq!(
        code(client.request("textDocument/hover", json!({}))),
        -32601
    );
    assert_eq!(code(client.request("astli/unknown", Value::Null)), -32601);
    // A notification it does not know is dropped, and the next request is
    // answered.
    client.notify("$/setTrace", json!({ "value": "verbose" }));
    client.notify("astli/unknown", Value::Null);
    assert_eq!(client.buffer(&uri("a.sv")), None);
    // Params a request cannot read are invalid.
    assert_eq!(code(client.request("test/wait", json!("x"))), -32602);
    assert!(client.logs.is_empty());
    client.finish();
}

#[test]
fn a_cancelled_request_is_answered_as_cancelled() {
    let mut client = Client::start(Encoding::Utf16);
    // Cancelling a request that was answered, or never sent, changes
    // nothing.
    client.notify("$/cancelRequest", json!({ "id": 99 }));
    client.notify("$/cancelRequest", json!({ "id": "unknown" }));
    let waiting = client.send("test/wait", Value::Null);
    client.notify("$/cancelRequest", json!({ "id": client.next }));
    assert_eq!(code(client.answer(&waiting)), -32800);
    assert!(client.logs.is_empty());
    client.finish();
}

#[test]
fn a_panic_is_answered_as_an_error_and_the_server_lives_on() {
    let mut client = Client::start(Encoding::Utf16);
    let uri = &uri("a.sv");
    let document = json!({ "uri": uri, "languageId": "systemverilog", "version": 1, "text": "a" });
    client.notify("textDocument/didOpen", json!({ "textDocument": document }));

    let error = client.request("test/panic", Value::Null).unwrap_err();
    assert_eq!(error.code, -32603);
    assert_eq!(
        error.message,
        "`test/panic` panicked: a request asked to panic"
    );
    let error = client.request("test/panicOnPool", Value::Null).unwrap_err();
    assert_eq!(error.code, -32603);
    client.notify("test/panic", Value::Null);

    // The buffers are as they were.
    assert_eq!(client.buffer(uri), Some((1, "a".to_string())));
    expect![[r#"
        [
            "`test/panic` panicked: a request asked to panic",
            "`test/panicOnPool` panicked: work on the pool asked to panic",
            "`test/panic` panicked: a notification asked to panic",
        ]
    "#]]
    .assert_debug_eq(&client.logs);
    client.finish();
}

#[test]
fn exit_ends_the_server_cleanly_only_after_shutdown() {
    let mut client = Client::start(Encoding::Utf16);
    assert_eq!(
        client.request("shutdown", Value::Null).unwrap(),
        Value::Null
    );
    // After `shutdown`, every request is refused, `shutdown` too.
    assert_eq!(
        code(client.request("test/buffer", json!(uri("a.sv")))),
        -32600
    );
    assert_eq!(code(client.request("shutdown", Value::Null)), -32600);
    assert_eq!(client.exit(), Ended::Exited);

    let client = Client::start(Encoding::Utf16);
    assert_eq!(client.exit(), Ended::ExitedWithoutShutdown);

    // A client that goes away without `exit` ends the server too.
    let Client {
        connection, server, ..
    } = Client::start(Encoding::Utf16);
    drop(connection);
    assert!(matches!(server.join().unwrap(), Ended::Failed(_)));
}
