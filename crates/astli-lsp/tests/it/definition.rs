//! Go to definition, asked of designs the support host lowers once each
//! has loaded.
//!
//! Every `tests/data/definition/**/*.sv` is a case, and the `.sv` files of
//! one directory are one design, so their names must not clash where a
//! case does not mean them to. A case is opened with the `.svh` beside it,
//! if it has one, and each place a `// ^` line marks in either is asked
//! about, under UTF-16, of a client that takes links: a `^` stands below
//! the character it marks in the nearest line above that is no such line,
//! counted in characters. Each answer is written as the range it leads
//! from, then the file and range it leads to and the text there, each
//! position `line:character` from 1, the character in UTF-16 code units.
//! Adding a case means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp definition::
//! ```

use std::fmt::Write;
use std::path::Path;

use astli_testing::snapshots;
use lsp_server::ErrorCode;
use lsp_types::{Location, LocationLink, Position, Range};
use serde_json::{Value, json};

use crate::support::{self, Client, code, loaded};

/// Asks where what stands at `position` of `uri` is declared.
fn ask(client: &mut Client, uri: &str, position: Position) -> Result<Value, i32> {
    let params = json!({ "textDocument": { "uri": uri }, "position": position });
    let answer = client.request("textDocument/definition", params);
    answer.map_err(|it| it.code)
}

/// What a client that takes links says.
fn links() -> Value {
    json!({ "textDocument": { "definition": { "linkSupport": true } } })
}

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/definition");
    snapshots::check_with(&root, "definition", &["svh"], |name, text| {
        let case = root.join(name);
        let dir = case.parent().unwrap();
        let mut names: Vec<String> = (std::fs::read_dir(dir).unwrap())
            .map(|it| it.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|it| it.ends_with(".sv") || it.ends_with(".svh"))
            .collect();
        names.sort();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut client = Client::designing(&names, links());
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
            for position in marked(text) {
                let at = written(position);
                writeln!(out, "{file} {at}").unwrap();
                let answer = ask(&mut client, &uri(path), position).expect("answered");
                let found: Vec<LocationLink> = match answer {
                    Value::Null => Vec::new(),
                    answer => serde_json::from_value(answer).expect("links"),
                };
                if found.is_empty() {
                    writeln!(out, "  nothing").unwrap();
                }
                for link in found {
                    assert_eq!(link.target_range, link.target_selection_range);
                    let from = link.origin_selection_range.expect("an origin");
                    let path = link.target_uri.to_file_path().expect("a file");
                    // Written with `/` on every platform, as the snapshot is.
                    let target = path.strip_prefix(&root).unwrap_or(&path);
                    let target = target.to_string_lossy().replace('\\', "/");
                    let to = link.target_range;
                    let said = spelled(&path, to);
                    let (from, to) = (range(from), range(to));
                    writeln!(out, "  {from} -> {target} {to} `{said}`").unwrap();
                }
            }
        }
        client.finish();
        out
    });
}

/// The position of each character a `// ^` line of `text` marks.
fn marked(text: &str) -> Vec<Position> {
    let mut found = Vec::new();
    let mut above = None;
    for (number, line) in text.lines().enumerate() {
        let marks = line.trim_start().strip_prefix("//").is_some_and(|rest| {
            rest.contains('^') && rest.trim_end().chars().all(|c| c == ' ' || c == '^')
        });
        if !marks {
            above = Some((number, line));
            continue;
        }
        let (number, code) = above.expect("a mark has a line above it");
        for (column, _) in line.chars().enumerate().filter(|(_, c)| *c == '^') {
            let before = code.chars().take(column);
            let character = before.map(char::len_utf16).sum::<usize>();
            found.push(Position::new(number as u32, character as u32));
        }
    }
    found
}

/// The text `range` covers of the file at `path`, on one line, counted in
/// UTF-16 code units.
fn spelled(path: &Path, range: Range) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    let line = text
        .lines()
        .nth(range.start.line as usize)
        .unwrap_or_default();
    let end = match range.end.line == range.start.line {
        true => range.end.character,
        false => u32::MAX,
    };
    let mut counted = 0;
    let mut said = String::new();
    for char in line.chars() {
        if (range.start.character..end).contains(&counted) {
            said.push(char);
        }
        counted += char.len_utf16() as u32;
    }
    said
}

fn written(position: Position) -> String {
    format!("{}:{}", position.line + 1, position.character + 1)
}

fn range(range: Range) -> String {
    format!("{}-{}", written(range.start), written(range.end))
}

fn uri(path: &Path) -> String {
    lsp_types::Uri::from_file_path(path)
        .expect("absolute")
        .to_string()
}

/// A server whose design reads `a.sv`, with it opened holding `text` and
/// loaded, for a client that takes no links.
fn opened(text: &str) -> (Client, String) {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = support::uri("a.sv");
    client.open(&uri, text);
    client.logged(loaded);
    (client, uri)
}

#[test]
fn a_client_that_takes_no_links_is_given_locations() {
    let (mut client, uri) = opened("module a;\n  logic x;\n  assign x = 1;\nendmodule\n");
    let answer = ask(&mut client, &uri, Position::new(2, 9)).expect("answered");
    let found: Vec<Location> = serde_json::from_value(answer).expect("locations");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].uri.to_string(), uri);
    let declared = Range::new(Position::new(1, 8), Position::new(1, 9));
    assert_eq!(found[0].range, declared);
    client.finish();
}

#[test]
fn a_buffer_newer_than_its_design_is_answered_as_modified() {
    let (mut client, uri) = opened("module a;\n  logic x;\n  assign x = 1;\nendmodule\n");
    // The edit makes the design due 100 ms later, so the request reads the
    // snapshot of the text before it.
    client.replace(
        &uri,
        "module a;\n  logic y, x;\n  assign x = 1;\nendmodule\n",
    );
    let answer = ask(&mut client, &uri, Position::new(2, 9));
    assert_eq!(answer, Err(ErrorCode::ContentModified as i32));

    // Once the design is loaded again, it is answered from the new text.
    client.logged(loaded);
    let answer = ask(&mut client, &uri, Position::new(2, 9)).expect("answered");
    let found: Vec<Location> = serde_json::from_value(answer).expect("locations");
    let declared = Range::new(Position::new(1, 11), Position::new(1, 12));
    assert_eq!(found[0].range, declared);
    client.finish();
}

#[test]
fn a_file_with_no_snapshot_answers_nothing() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = support::uri("a.sv");
    // Not open.
    assert_eq!(ask(&mut client, &uri, Position::new(0, 0)), Ok(Value::Null));
    // Open, and its design not loaded yet.
    client.open(&uri, "module a;\n  logic x;\n  assign x = 1;\nendmodule\n");
    assert_eq!(ask(&mut client, &uri, Position::new(2, 9)), Ok(Value::Null));
    // Params that name no position.
    let answer = client.request("textDocument/definition", json!({}));
    assert_eq!(code(answer), ErrorCode::InvalidParams as i32);
    client.finish();
}
