//! Diagnostics as JSON, for a program to read: a line for each under
//! `--diag json`.
//!
//! Written by hand rather than through a serializer: a record is a handful
//! of strings and numbers, and escaping a string is all JSON asks of it.

use std::fmt::Write as _;

use astli_diag::{Resolved, Sources};
use astli_text::{Severity, Span};

/// A diagnostic as one line of JSON, every key present in every record:
///
/// ```json
/// {"severity":"error","code":"undefined-macro","message":"`W is not defined",
///  "path":"top.sv","start":{"line":1,"column":8,"offset":7},"end":{...},
///  "label":"not defined here","labels":[],"notes":[],"spelled":null,
///  "expansion":[],"included_from":[]}
/// ```
///
/// The place is where the user wrote the text, as `--diag short` has it;
/// `spelled` is the text a macro call stands for, and `expansion` the calls
/// it went through, innermost first.
pub fn line(sources: &Sources, one: &Resolved) -> String {
    let diagnostic = one.diagnostic;
    let labels = list(one.labels.iter().map(|(at, message)| {
        format!(
            r#"{{"message":{},{}}}"#,
            string(message),
            place(sources, *at)
        )
    }));
    let notes = list(diagnostic.notes.iter().map(|note| string(note)));
    let spelled = match one.spelled {
        Some(at) => format!("{{{}}}", place(sources, at)),
        None => "null".to_string(),
    };
    let expansion = list(one.through.iter().map(|through| {
        format!(
            r#"{{"macro":{},{}}}"#,
            string(&through.name),
            place(sources, through.call)
        )
    }));
    let included_from =
        list((one.included_from.iter()).map(|&at| format!("{{{}}}", place(sources, at))));
    format!(
        r#"{{"severity":"{}","code":{},"message":{},{},"label":{},"labels":{labels},"notes":{notes},"spelled":{spelled},"expansion":{expansion},"included_from":{included_from}}}"#,
        diagnostic.severity,
        string(diagnostic.code.as_str()),
        string(&diagnostic.message),
        place(sources, one.at),
        diagnostic
            .label
            .as_deref()
            .map_or("null".to_string(), string),
    )
}

/// A problem with no place in a file as a line of [`line`]'s shape, with
/// every key a place would fill null or empty.
pub fn unplaced(severity: Severity, message: &str, notes: &[String]) -> String {
    let notes = list(notes.iter().map(|note| string(note)));
    format!(
        r#"{{"severity":"{severity}","code":null,"message":{},"path":null,"start":null,"end":null,"label":null,"labels":[],"notes":{notes},"spelled":null,"expansion":[],"included_from":[]}}"#,
        string(message),
    )
}

/// The `path`, `start` and `end` keys of `at`, a position being its line
/// and column, both from one and the column in characters, and its byte
/// offset.
fn place(sources: &Sources, at: Span) -> String {
    let origins = sources.origins();
    let position = |offset: u32| {
        let at = origins.line_col(at.src_id, offset);
        format!(
            r#"{{"line":{},"column":{},"offset":{offset}}}"#,
            at.line, at.col
        )
    };
    format!(
        r#""path":{},"start":{},"end":{}"#,
        string(&sources.name(at.src_id)),
        position(at.start),
        position(at.end)
    )
}

/// `items`, each already JSON, as an array.
fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(","))
}

/// `text` as a JSON string.
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
