//! The loop itself: the handshake, what it does not know, panics and exit.

use std::sync::Arc;

use astli_lsp::Ended;
use expect_test::expect;
use lsp_server::{Connection, Message, Request, RequestId};
use serde_json::{Value, json};

use crate::support::{Client, code, loaded, path, uri};

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
        let general = match offered {
            Some(encodings) => json!({ "positionEncodings": encodings }),
            None => json!({}),
        };
        let (client, result) = Client::initialize(&[], json!({ "general": general }));
        let announced = &result["capabilities"]["positionEncoding"];
        assert_eq!(announced, chosen, "offered {offered:?}");
        client.finish();
    }

    // Everything else announced is the documents' sync, code actions,
    // formatting, symbols, folds, definitions and hover.
    let general = json!({ "positionEncodings": ["utf-8"] });
    let (client, result) = Client::initialize(&[], json!({ "general": general }));
    expect![[r#"
        {
          "capabilities": {
            "codeActionProvider": {
              "codeActionKinds": [
                "quickfix",
                "source.fixAll.astli"
              ]
            },
            "definitionProvider": true,
            "documentFormattingProvider": true,
            "documentSymbolProvider": true,
            "foldingRangeProvider": true,
            "hoverProvider": true,
            "positionEncoding": "utf-8",
            "textDocumentSync": {
              "change": 2,
              "openClose": true
            },
            "workspaceSymbolProvider": true
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
    struct Nothing;
    impl astli_lsp::Host for Nothing {
        fn version(&self) -> String {
            String::new()
        }
        fn fmt_options(&self, _: &std::path::Path) -> Result<Option<astli_fmt::Options>, String> {
            Ok(None)
        }
        fn design(&self, path: &std::path::Path) -> std::path::PathBuf {
            path.to_path_buf()
        }
        fn load(
            &self,
            _: &std::path::Path,
            _: &dyn astli_text::Reader,
            _: bool,
            _: &(dyn Fn() -> bool + Sync),
        ) -> Option<astli_lsp::Snapshot> {
            None
        }
        fn forget(&self, _: &std::path::Path) {}
        fn watched(&self) -> astli_lsp::Watched {
            let (names, extensions) = (Vec::new(), Vec::new());
            astli_lsp::Watched { names, extensions }
        }
    }

    let (client, server) = Connection::memory();
    let server = std::thread::spawn(move || astli_lsp::serve(&server, Arc::new(Nothing)));
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
fn requests_the_server_does_not_know_are_answered_as_unknown() {
    let mut client = Client::start(&[], false);
    assert_eq!(
        code(client.request("textDocument/references", json!({}))),
        -32601
    );
    assert_eq!(code(client.request("astli/unknown", Value::Null)), -32601);
    // A notification it does not know is dropped, and the next request is
    // answered.
    client.notify("$/setTrace", json!({ "value": "verbose" }));
    client.notify("astli/unknown", Value::Null);
    // Cancelling a request that was answered, or never sent, changes
    // nothing.
    client.notify("$/cancelRequest", json!({ "id": 99 }));
    client.notify("$/cancelRequest", json!({ "id": "unknown" }));
    assert_eq!(code(client.request("astli/unknown", Value::Null)), -32601);
    assert!(client.logs.is_empty(), "{:?}", client.logs);
    client.finish();
}

#[test]
fn a_panic_is_told_and_the_server_lives_on() {
    let mut client = Client::start(&["a.sv"], false);
    // On the main loop, asking which design a file is in.
    client.open(&uri("panic/a.sv"), "");
    // On the pool, loading one.
    let uri = &uri("a.sv");
    client.open(uri, "panic");
    let first = client.load();
    let said = client.logged(|it| it.starts_with("loading"));
    let root = path("a.sv").parent().unwrap().display().to_string();
    assert_eq!(
        said,
        format!("loading {root} panicked: a load was asked to panic")
    );

    // The next load is made, and discovers again, since the one that
    // panicked may not have kept what it found.
    client.replace(uri, "module a;");
    let load = client.load();
    assert_eq!((first.discover, load.discover), (true, true));
    assert_eq!(load.text("a.sv"), Some("module a;"));
    client.logged(loaded);
    expect![[r#"
        [
            "`textDocument/didOpen` panicked: the host was asked to panic",
        ]
    "#]]
    .assert_debug_eq(&client.logs);
    client.finish();
}

#[test]
fn exit_ends_the_server_cleanly_only_after_shutdown() {
    let mut client = Client::start(&[], false);
    assert_eq!(
        client.request("shutdown", Value::Null).unwrap(),
        Value::Null
    );
    // After `shutdown`, every request is refused, `shutdown` too.
    assert_eq!(
        code(client.request("textDocument/hover", json!({}))),
        -32600
    );
    assert_eq!(code(client.request("shutdown", Value::Null)), -32600);
    assert_eq!(client.exit(), Ended::Exited);

    let client = Client::start(&[], false);
    assert_eq!(client.exit(), Ended::ExitedWithoutShutdown);

    // A client that goes away without `exit` ends the server too.
    let client = Client::start(&[], false);
    assert!(matches!(client.hang_up(), Ended::Failed(_)));
}
