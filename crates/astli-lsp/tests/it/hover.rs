//! Hover, asked of designs the support host lowers once each has loaded.
//! What it says of each kind of name is the crate's snapshot test over
//! `tests/data/hover/`; these ask it as a client does, of a name, a macro
//! call, a header and its `` `include ``, and where there is nothing to
//! ask.

use lsp_server::ErrorCode;
use lsp_types::{Contents, Hover, MarkupKind, Position, Range};
use serde_json::{Value, json};

use crate::support::{self, Client, code, loaded};

/// Asks what stands at `position` of `uri`.
fn ask(client: &mut Client, uri: &str, position: Position) -> Result<Value, i32> {
    let params = json!({ "textDocument": { "uri": uri }, "position": position });
    let answer = client.request("textDocument/hover", params);
    answer.map_err(|it| it.code)
}

/// The markdown and the range hover answers with.
fn said(answer: Result<Value, i32>) -> (String, Range) {
    let hover: Hover = serde_json::from_value(answer.expect("answered")).expect("a hover");
    let Contents::MarkupContent(content) = hover.contents else {
        panic!("markup: {:?}", hover.contents)
    };
    assert_eq!(content.kind, MarkupKind::Markdown);
    (content.value, hover.range.expect("a range"))
}

fn range(line: u32, start: u32, end: u32) -> Range {
    Range::new(Position::new(line, start), Position::new(line, end))
}

const TEXT: &str = "`include \"b.svh\"\n`define W 8\nmodule a;\n  logic x;\n  \
                    localparam int P = `W;\n  assign x = shared;\nendmodule\n";
const HEADER: &str = "// Shared by the unit.\nlogic shared;\n";

/// A server whose design reads `a.sv`, which includes `b.svh`, with both
/// opened, holding [`TEXT`] and [`HEADER`], and loaded.
fn opened() -> (Client, String) {
    let mut client = Client::designing(&["a.sv", "b.svh"], json!({}));
    let uri = support::uri("a.sv");
    client.open(&uri, TEXT);
    client.open(&support::uri("b.svh"), HEADER);
    client.logged(loaded);
    (client, uri)
}

#[test]
fn a_name_says_what_it_is_over_its_range() {
    let (mut client, uri) = opened();
    let (value, at) = said(ask(&mut client, &uri, Position::new(5, 10)));
    assert_eq!(
        value,
        "variable `x`: `logic`\n\n```systemverilog\nlogic x\n```\n"
    );
    assert_eq!(at, range(5, 9, 10));
    client.finish();
}

#[test]
fn a_macro_call_says_what_it_expands_to() {
    let (mut client, uri) = opened();
    let (value, at) = said(ask(&mut client, &uri, Position::new(4, 22)));
    let expands = "macro `W`\n\n```systemverilog\n`define W 8\n```\n\n\
                   expands here to\n\n```systemverilog\n8\n```\n";
    assert_eq!(value, expands);
    assert_eq!(at, range(4, 21, 23));
    client.finish();
}

#[test]
fn a_header_is_read_through_the_unit_that_includes_it() {
    let (mut client, uri) = opened();
    // A name the header declares, used in the unit.
    let (value, _) = said(ask(&mut client, &uri, Position::new(5, 15)));
    assert!(
        value.contains("// Shared by the unit.\nlogic shared\n"),
        "{value}"
    );
    // The `include, which says the header it read.
    let (value, at) = said(ask(&mut client, &uri, Position::new(0, 3)));
    assert_eq!(value, "includes `b.svh`\n");
    assert_eq!(at, range(0, 0, 16));
    // The header's own declaration, asked of in the header.
    let header = support::uri("b.svh");
    let (value, at) = said(ask(&mut client, &header, Position::new(1, 7)));
    assert!(value.starts_with("variable `shared`: `logic`"), "{value}");
    assert_eq!(at, range(1, 6, 12));
    client.finish();
}

#[test]
fn a_buffer_newer_than_its_design_is_answered_as_modified() {
    let (mut client, uri) = opened();
    // The edit makes the design due 100 ms later, so the request reads the
    // snapshot of the text before it.
    client.replace(&uri, &TEXT.replace("logic x;", "logic y, x;"));
    let answer = ask(&mut client, &uri, Position::new(5, 10));
    assert_eq!(answer, Err(ErrorCode::ContentModified as i32));
    client.logged(loaded);
    let (value, _) = said(ask(&mut client, &uri, Position::new(5, 10)));
    assert!(value.contains("logic x"), "{value}");
    client.finish();
}

#[test]
fn where_there_is_nothing_to_say_it_answers_nothing() {
    let mut client = Client::designing(&["a.sv", "b.svh"], json!({}));
    let uri = support::uri("a.sv");
    // Not open.
    assert_eq!(ask(&mut client, &uri, Position::new(0, 0)), Ok(Value::Null));
    // Open, and its design not loaded yet.
    client.open(&uri, TEXT);
    assert_eq!(
        ask(&mut client, &uri, Position::new(5, 10)),
        Ok(Value::Null)
    );
    // Loaded, and a keyword.
    client.logged(loaded);
    assert_eq!(ask(&mut client, &uri, Position::new(2, 2)), Ok(Value::Null));
    // Params that name no position.
    let answer = client.request("textDocument/hover", json!({}));
    assert_eq!(code(answer), ErrorCode::InvalidParams as i32);
    client.finish();
}
