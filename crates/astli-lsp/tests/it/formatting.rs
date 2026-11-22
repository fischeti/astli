//! `textDocument/formatting`: the edits it answers with, applied, and what
//! it tells the user when it gives none.

use astli_fmt::Options;
use astli_parse::SyntaxTree;
use serde_json::{Value, json};

use crate::support::{Client, Layout, uri};

/// A client of a server formatting every file with `options`.
fn with(options: Options, utf8: bool) -> Client {
    Client::laid_out(Box::new(move |_| Ok(Some(options))), utf8)
}

/// The edits the server formats `uri` with.
fn format(client: &mut Client, uri: &str) -> Vec<Value> {
    // Options the server ignores, since `astli.toml` is the configuration.
    let options = json!({ "tabSize": 8, "insertSpaces": false });
    let params = json!({ "textDocument": { "uri": uri }, "options": options });
    let edits = client.request("textDocument/formatting", params);
    serde_json::from_value(edits.expect("no error")).expect("a list of edits")
}

/// `text` with `edits` applied, each range read in `text` in UTF-8 if
/// `utf8` and else in UTF-16, as the protocol has them.
fn apply(text: &str, edits: &[Value], utf8: bool) -> String {
    let mut ranges: Vec<(usize, usize, &str)> = (edits.iter())
        .map(|edit| {
            let range = &edit["range"];
            let start = offset(text, &range["start"], utf8);
            let end = offset(text, &range["end"], utf8);
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
fn offset(text: &str, position: &Value, utf8: bool) -> usize {
    let line = position["line"].as_u64().expect("a line") as usize;
    let character = position["character"].as_u64().expect("a character") as usize;
    let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
    let rest = &text[start..];
    let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
    start + within(line, character, utf8)
}

/// The byte offset of `character` units into `line`.
fn within(line: &str, character: usize, utf8: bool) -> usize {
    let mut counted = 0;
    for (at, char) in line.char_indices() {
        if counted >= character {
            return at;
        }
        counted += if utf8 {
            char.len_utf8()
        } else {
            char.len_utf16()
        };
    }
    line.len()
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
    for utf8 in [true, false] {
        let mut client = with(Options::default(), utf8);
        let uri = uri("m.sv");
        client.open(&uri, UNFORMATTED);
        let edits = format(&mut client, &uri);
        let expected = formatted(UNFORMATTED, &Options::default());
        assert_eq!(apply(UNFORMATTED, &edits, utf8), expected);
        // Lines 3 and 5 differ; the ones between and around are kept.
        let lines: Vec<u64> = (edits.iter())
            .map(|it| it["range"]["start"]["line"].as_u64().unwrap())
            .collect();
        assert_eq!(lines, [2, 4]);

        // Formatted, the buffer needs nothing more.
        client.replace(&uri, &expected);
        assert_eq!(format(&mut client, &uri), Vec::<Value>::new());
        assert!(client.shown.is_empty(), "{:?}", client.shown);
        client.finish();
    }
}

#[test]
fn a_line_of_characters_past_ascii_is_counted_in_the_negotiated_encoding() {
    // The last line has no newline, so the edit adding it ends inside the
    // line, past characters of two, three and four bytes.
    let text = "module m;\n  logic a;  // ä € 😀\nendmodule // ä € 😀  ";
    for utf8 in [true, false] {
        let mut client = with(Options::default(), utf8);
        let uri = uri("wide.sv");
        client.open(&uri, text);
        let edits = format(&mut client, &uri);
        assert_eq!(
            apply(text, &edits, utf8),
            formatted(text, &Options::default())
        );
        let end = edits.last().expect("an edit")["range"]["end"].clone();
        let character = if utf8 { 26 } else { 21 };
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
    let mut client = with(indented, true);
    let uri = uri("m.sv");
    client.open(&uri, UNFORMATTED);
    let edits = format(&mut client, &uri);
    let expected = formatted(UNFORMATTED, &indented);
    assert!(expected.contains("\n    logic a;"), "{expected}");
    assert_eq!(apply(UNFORMATTED, &edits, true), expected);
    client.finish();
}

#[test]
fn a_file_given_no_edit_says_why_where_the_user_would_want_to_know() {
    let layout: Layout = Box::new(|path| match path.file_name() {
        Some(name) if name == "broken.sv" => {
            Err("astli.toml: [fmt]: unknown field `columns`".into())
        }
        Some(name) if name == "excluded.sv" => Ok(None),
        _ => Ok(Some(Options::default())),
    });
    let mut client = Client::laid_out(layout, true);

    // An unreadable manifest is shown; an excluded file is left quietly.
    for name in ["broken.sv", "excluded.sv"] {
        let uri = uri(name);
        client.open(&uri, UNFORMATTED);
        assert_eq!(format(&mut client, &uri), Vec::<Value>::new(), "{name}");
    }
    // A file that is not open has no buffer to format.
    assert_eq!(format(&mut client, &uri("closed.sv")), Vec::<Value>::new());
    assert_eq!(client.shown, ["astli.toml: [fmt]: unknown field `columns`"]);
    client.finish();
}
