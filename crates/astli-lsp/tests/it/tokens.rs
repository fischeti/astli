//! Semantic tokens, asked of designs the support host lowers once each has
//! loaded.
//!
//! Every `tests/data/tokens/**/*.sv` is a case, and the `.sv` files of one
//! directory are one design, so their names must not clash where a case
//! does not mean them to. A case is opened with the `.svh` beside it, if it
//! has one, and the whole of each is asked for under UTF-16. Each token is
//! written decoded, as a line of its file: its position `line:character`
//! from 1, the character in UTF-16 code units, the text it covers, its type
//! and its modifiers, as the legend `initialize` announced names them.
//! Adding a case means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp tokens::
//! ```

use std::fmt::Write;
use std::path::Path;

use astli_testing::snapshots;
use lsp_server::ErrorCode;
use lsp_types::SemanticTokensLegend;
use serde_json::{Value, json};

use crate::support::{self, Client, code, loaded};

/// A token, decoded: its line and character from 0, its length, its type
/// and its modifiers, by name.
#[derive(Debug, PartialEq)]
struct Token {
    line: u32,
    character: u32,
    length: u32,
    ty: String,
    modifiers: Vec<String>,
}

/// The legend a server announces, the same for every client.
fn legend() -> SemanticTokensLegend {
    let (client, result) = Client::initialize(&[], json!({}));
    client.finish();
    let legend = &result["capabilities"]["semanticTokensProvider"]["legend"];
    serde_json::from_value(legend.clone()).expect("a legend")
}

/// The tokens of the answer `data`, each placed absolutely.
fn decoded(legend: &SemanticTokensLegend, answer: &Value) -> Vec<Token> {
    let data: Vec<u32> = serde_json::from_value(answer["data"].clone()).expect("data");
    let (mut line, mut character) = (0, 0);
    let mut found = Vec::new();
    for token in data.chunks(5) {
        let [delta_line, delta_start, length, ty, modifiers] = token else {
            panic!("a token of five numbers: {data:?}");
        };
        line += delta_line;
        character = match delta_line {
            0 => character + delta_start,
            _ => *delta_start,
        };
        let modifiers = (legend.token_modifiers.iter().enumerate())
            .filter(|(bit, _)| modifiers & (1 << bit) != 0)
            .map(|(_, it)| it.clone())
            .collect();
        found.push(Token {
            line,
            character,
            length: *length,
            ty: legend.token_types[*ty as usize].clone(),
            modifiers,
        });
    }
    found
}

/// Asks for the tokens of the whole of `uri`.
fn full(client: &mut Client, uri: &str) -> Result<Value, i32> {
    let params = json!({ "textDocument": { "uri": uri } });
    let answer = client.request("textDocument/semanticTokens/full", params);
    answer.map_err(|it| it.code)
}

#[test]
fn snapshots() {
    let legend = legend();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/tokens");
    snapshots::check_with(&root, "tokens", &["svh"], |name, text| {
        let case = root.join(name);
        let dir = case.parent().unwrap();
        let mut names: Vec<String> = (std::fs::read_dir(dir).unwrap())
            .map(|it| it.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|it| it.ends_with(".sv") || it.ends_with(".svh"))
            .collect();
        names.sort();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut client = Client::designing(&names, json!({}));
        let mut opened = vec![(case.clone(), text)];
        let header = case.with_extension("svh");
        if header.exists() {
            let text = std::fs::read_to_string(&header).unwrap();
            opened.push((header, text));
        }
        for (path, text) in &opened {
            client.open(&uri(path), text);
        }
        client.logged(loaded);
        let mut out = String::new();
        for (path, text) in &opened {
            let file = path.file_name().unwrap().to_string_lossy();
            writeln!(out, "{file}").unwrap();
            let answer = full(&mut client, &uri(path)).expect("answered");
            for token in decoded(&legend, &answer) {
                let line = text.lines().nth(token.line as usize).unwrap_or_default();
                let said = spelled(line, token.character, token.length);
                let (at, ty) = (token.line + 1, &token.ty);
                let modifiers = token.modifiers.join(" ");
                let written = format!("  {at}:{} `{said}` {ty} {modifiers}", token.character + 1);
                writeln!(out, "{}", written.trim_end()).unwrap();
            }
        }
        client.finish();
        out
    });
}

/// The `length` UTF-16 code units of `line` from `start`.
fn spelled(line: &str, start: u32, length: u32) -> String {
    let mut counted = 0;
    let mut said = String::new();
    for char in line.chars() {
        if (start..start + length).contains(&counted) {
            said.push(char);
        }
        counted += char.len_utf16() as u32;
    }
    said
}

fn uri(path: &Path) -> String {
    lsp_types::Uri::from_file_path(path)
        .expect("absolute")
        .to_string()
}

const TEXT: &str =
    "module a;\n  logic x;\n  assign x = 1;\n  logic y;\n  assign y = x;\nendmodule\n";

/// A server whose design reads `a.sv`, with it opened holding [`TEXT`] and
/// loaded, for a client with `capabilities`.
fn opened(capabilities: Value) -> (Client, String) {
    let mut client = Client::designing(&["a.sv"], capabilities);
    let uri = support::uri("a.sv");
    client.open(&uri, TEXT);
    client.logged(loaded);
    (client, uri)
}

#[test]
fn a_range_is_given_the_tokens_that_overlap_it() {
    let legend = legend();
    let (mut client, uri) = opened(json!({}));
    // From the first `x` used to the start of the last line's `y`, which
    // it does not overlap.
    let range = json!({
        "start": { "line": 2, "character": 9 },
        "end": { "line": 4, "character": 9 },
    });
    let params = json!({ "textDocument": { "uri": uri }, "range": range });
    let answer = client.request("textDocument/semanticTokens/range", params);
    let found = decoded(&legend, &answer.expect("answered"));
    let at: Vec<(u32, u32)> = found.iter().map(|it| (it.line, it.character)).collect();
    assert_eq!(at, [(2, 9), (3, 8)], "{found:?}");
    client.finish();
}

#[test]
fn a_token_is_placed_in_the_negotiated_encoding() {
    // `𝄞` is four bytes and two UTF-16 code units, `é` two bytes and one.
    let text = "module a;\n  logic x;\n  /* \u{1D11E}\u{e9} */ assign x = 1;\nendmodule\n";
    for (offered, character) in [(json!(["utf-8"]), 22), (json!([]), 19)] {
        let capabilities = json!({ "general": { "positionEncodings": offered } });
        let mut client = Client::designing(&["a.sv"], capabilities);
        let uri = support::uri("a.sv");
        client.open(&uri, text);
        client.logged(loaded);
        let answer = full(&mut client, &uri).expect("answered");
        let data: Vec<u32> = serde_json::from_value(answer["data"].clone()).unwrap();
        // The module, the declaration, then the use a line below it.
        assert_eq!(data[10..13], [1, character, 1], "{data:?}");
        client.finish();
    }
}

#[test]
fn a_buffer_newer_than_its_design_is_answered_as_modified_and_asked_again() {
    let capabilities = json!({ "workspace": { "semanticTokens": { "refreshSupport": true } } });
    let (mut client, uri) = opened(capabilities);
    let refresh = "workspace/semanticTokens/refresh";
    let asked = |client: &Client| client.asked.iter().filter(|it| it.0 == refresh).count();
    let _ = client.request("astli/unknown", Value::Null);
    assert_eq!(asked(&client), 1);
    // The edit makes the design due 100 ms later, so the request reads the
    // snapshot of the text before it.
    client.replace(&uri, &TEXT.replace("logic x;", "logic w, x;"));
    assert_eq!(
        full(&mut client, &uri),
        Err(ErrorCode::ContentModified as i32)
    );
    // Once the design is loaded again, the client is told to ask again.
    client.logged(loaded);
    let _ = client.request("astli/unknown", Value::Null);
    assert_eq!(asked(&client), 2);
    let answer = full(&mut client, &uri).expect("answered");
    let data = answer["data"].as_array().expect("data");
    // The module, `w` declared, and `x` and `y` declared and used.
    assert_eq!(data.len(), 7 * 5, "{data:?}");
    client.finish();
}

#[test]
fn a_file_with_no_snapshot_answers_nothing() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = support::uri("a.sv");
    // Not open.
    assert_eq!(full(&mut client, &uri), Ok(Value::Null));
    // Open, and its design not loaded yet.
    client.open(&uri, TEXT);
    assert_eq!(full(&mut client, &uri), Ok(Value::Null));
    // Params that name no document.
    let answer = client.request("textDocument/semanticTokens/full", json!({}));
    assert_eq!(code(answer), ErrorCode::InvalidParams as i32);
    client.finish();
}
