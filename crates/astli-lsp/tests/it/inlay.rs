//! Inlay hints, asked of designs the support host lowers once each has
//! loaded.
//!
//! Every `tests/data/inlay/*.sv` is a case, a design of its own unit, the
//! definition called `top` its top. It is asked for its hints over the
//! whole file, of every kind, those the server leaves out too, under
//! UTF-16, and written as the source with each hint drawn where it is,
//! `‹label›` with a space for its padding, then each hint's tooltip.
//! Adding a case means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp inlay::
//! ```

use std::fmt::Write;
use std::path::Path;

use astli_testing::snapshots;
use lsp_server::ErrorCode;
use lsp_types::{InlayHint, Label, Position, Range, Tooltip};
use serde_json::{Value, json};

use crate::support::{self, Client, loaded};

/// Asks for the hints over `range` of `uri`.
fn ask(client: &mut Client, uri: &str, range: Range) -> Result<Vec<InlayHint>, i32> {
    let params = json!({ "textDocument": { "uri": uri }, "range": range });
    let answer = client.request("textDocument/inlayHint", params);
    answer.map_err(|it| it.code).map(|it| match it {
        Value::Null => Vec::new(),
        it => serde_json::from_value(it).expect("hints"),
    })
}

/// The whole of `text`.
fn whole(text: &str) -> Range {
    Range::new(
        Position::new(0, 0),
        Position::new(text.lines().count() as u32, 0),
    )
}

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/inlay");
    snapshots::check_with(&root, "inlay", &[], |name, text| {
        let mut client = Client::hinting(&[name], json!({}));
        let uri = uri(&root.join(name));
        client.open(&uri, &text);
        client.logged(loaded);
        let hints = ask(&mut client, &uri, whole(&text)).expect("answered");
        client.finish();
        drawn(&text, &hints)
    });
}

/// `text` with each of `hints` drawn in place, then their tooltips.
fn drawn(text: &str, hints: &[InlayHint]) -> String {
    let mut out = String::new();
    for (number, line) in text.lines().enumerate() {
        let mut counted = 0;
        let on: Vec<&InlayHint> = (hints.iter())
            .filter(|it| it.position.line == number as u32)
            .collect();
        let draw = |out: &mut String, counted: u32| {
            for hint in on.iter().filter(|it| it.position.character == counted) {
                out.push_str(&written(hint));
            }
        };
        for char in line.chars() {
            draw(&mut out, counted);
            out.push(char);
            counted += char.len_utf16() as u32;
        }
        draw(&mut out, counted);
        out.push('\n');
    }
    let tooltips: Vec<String> = (hints.iter())
        .filter_map(|hint| {
            let Tooltip::String(tooltip) = hint.tooltip.as_ref()? else {
                panic!("a plain tooltip: {:?}", hint.tooltip)
            };
            let at = hint.position;
            Some(format!("{}:{} {tooltip}", at.line + 1, at.character + 1))
        })
        .collect();
    if !tooltips.is_empty() {
        out += "\ntooltips:\n";
        for tooltip in tooltips {
            writeln!(out, "  {tooltip}").unwrap();
        }
    }
    out
}

/// `hint` as drawn: its label, and its padding a space.
fn written(hint: &InlayHint) -> String {
    let Label::String(label) = &hint.label else {
        panic!("a plain label: {:?}", hint.label)
    };
    let left = if hint.padding_left == Some(true) {
        " "
    } else {
        ""
    };
    let right = if hint.padding_right == Some(true) {
        " "
    } else {
        ""
    };
    format!("{left}‹{label}›{right}")
}

fn uri(path: &Path) -> String {
    lsp_types::Uri::from_file_path(path)
        .expect("an absolute path")
        .to_string()
}

const TEXT: &str = "module m (input logic a_i, input logic b_i);\nendmodule\n\
                    module top;\n  logic x, y;\n  m u_m (x, y);\nendmodule\n";

/// A server whose design reads `a.sv`, opened holding [`TEXT`] and
/// loaded, answering the kinds it answers.
fn opened() -> (Client, String) {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = support::uri("a.sv");
    client.open(&uri, TEXT);
    client.logged(loaded);
    (client, uri)
}

/// Each hint's line, character and label.
fn labels(hints: &[InlayHint]) -> Vec<(u32, u32, String)> {
    (hints.iter())
        .map(|it| {
            let Label::String(label) = &it.label else {
                panic!("a plain label")
            };
            (it.position.line, it.position.character, label.clone())
        })
        .collect()
}

#[test]
fn only_the_hints_inside_the_range_are_answered() {
    let (mut client, uri) = opened();
    let all = labels(&ask(&mut client, &uri, whole(TEXT)).unwrap());
    let both = vec![(4, 9, "a_i:".to_string()), (4, 12, "b_i:".to_string())];
    assert_eq!(all, both);
    // A range that ends inside the instantiation, after its first argument.
    let range = Range::new(Position::new(3, 0), Position::new(4, 10));
    assert_eq!(labels(&ask(&mut client, &uri, range).unwrap()), both[..1]);
    // One that starts after it, and one that holds only the second's place.
    let after = Range::new(Position::new(4, 13), Position::new(5, 0));
    assert_eq!(ask(&mut client, &uri, after), Ok(Vec::new()));
    let at = Range::new(Position::new(4, 12), Position::new(4, 12));
    assert_eq!(labels(&ask(&mut client, &uri, at).unwrap()), both[1..]);
    client.finish();
}

#[test]
fn a_buffer_newer_than_its_design_is_answered_as_modified() {
    let (mut client, uri) = opened();
    // The edit makes the design due 100 ms later, so the request reads the
    // snapshot of the text before it.
    let text = TEXT.replace("logic x, y;", "logic w, x, y;");
    client.replace(&uri, &text);
    let answer = ask(&mut client, &uri, whole(&text));
    assert_eq!(answer, Err(ErrorCode::ContentModified as i32));
    client.logged(loaded);
    assert_eq!(ask(&mut client, &uri, whole(&text)).unwrap().len(), 2);
    client.finish();
}

#[test]
fn a_file_not_open_or_not_loaded_has_none() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = support::uri("a.sv");
    assert_eq!(ask(&mut client, &uri, whole(TEXT)), Ok(Vec::new()));
    client.open(&uri, TEXT);
    assert_eq!(ask(&mut client, &uri, whole(TEXT)), Ok(Vec::new()));
    client.logged(loaded);
    client.finish();
}
