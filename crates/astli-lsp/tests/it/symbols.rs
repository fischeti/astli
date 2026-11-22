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
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp symbols::
//! ```

use std::path::Path;

use astli_testing::snapshots;
use expect_test::expect;
use lsp_types::{DocumentSymbol, FoldingRange, Position, Range};
use serde_json::{Value, json};

use crate::support::{self, Client};

/// Starts a server for a client that offers no encoding, so positions are
/// UTF-16, and has the text document `capabilities`.
fn start(capabilities: Value) -> Client {
    Client::initialize(&[], json!({ "textDocument": capabilities })).0
}

/// Opens `text` as `name`, returning its URI.
fn open(client: &Client, name: &str, text: &str) -> String {
    let uri = support::uri(name);
    client.open(&uri, text);
    uri
}

/// Asks `method` of the document at `uri`, which it answers.
fn ask(client: &mut Client, method: &str, uri: &str) -> Value {
    client.request(method, document(uri)).expect("answered")
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
        let mut client = start(capabilities());
        let uri = open(&client, name, &text);
        let symbols = ask(&mut client, "textDocument/documentSymbol", &uri);
        client.finish();
        let symbols: Vec<DocumentSymbol> = serde_json::from_value(symbols).expect("symbols");
        let mut out = String::new();
        outline(&symbols, 0, &mut out);
        out
    });
    snapshots::check_with(&root, "folds", &["symbols"], |name, text| {
        let mut client = start(capabilities());
        let uri = open(&client, name, &text);
        let folds = ask(&mut client, "textDocument/foldingRange", &uri);
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
    let mut client = start(json!({}));
    let text = "module top;\n  wire /* é */ w;\nendmodule\n";
    let uri = open(&client, "flat.sv", text);
    let symbols = ask(&mut client, "textDocument/documentSymbol", &uri);
    let folds = ask(&mut client, "textDocument/foldingRange", &uri);
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
