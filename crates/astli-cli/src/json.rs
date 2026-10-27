//! Diagnostics as JSON, for a program to read: a line for each under
//! `--diag json`, or under `--diag gitlab` the Code Quality report GitLab
//! turns into annotations on a merge request.
//!
//! Written by hand rather than through a serializer: a record is a handful
//! of strings and numbers, and escaping a string is all JSON asks of it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use astli_diag::{Resolved, Sources};
use astli_text::{Severity, Span};

/// A diagnostic as one line of JSON, every key present in every record:
///
/// ```json
/// {"severity":"error","code":"undefined-macro","message":"`W is not defined",
///  "path":"top.sv","start":{"line":1,"column":8,"offset":7},"end":{...},
///  "label":"not defined here","labels":[],"notes":[],"help":[],"spelled":null,
///  "expansion":[],"included_from":[],"fix":[]}
/// ```
///
/// The place is where the user wrote the text, as `--diag short` has it;
/// `spelled` is the text a macro call stands for, and `expansion` the calls
/// it went through, innermost first. `fix` holds the edits that fix it, each
/// the `text` to put where it is placed, for an editor to offer.
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
    let help = list(diagnostic.help.iter().map(|help| string(help)));
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
    let fix = list((diagnostic.fix.iter()).map(|edit| {
        format!(
            r#"{{"text":{},{}}}"#,
            string(&edit.text),
            place(sources, edit.at)
        )
    }));
    format!(
        r#"{{"severity":"{}","code":{},"message":{},{},"label":{},"labels":{labels},"notes":{notes},"help":{help},"spelled":{spelled},"expansion":{expansion},"included_from":{included_from},"fix":{fix}}}"#,
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
pub fn unplaced(severity: Severity, message: &str, notes: &[String], help: &[String]) -> String {
    let notes = list(notes.iter().map(|note| string(note)));
    let help = list(help.iter().map(|help| string(help)));
    format!(
        r#"{{"severity":"{severity}","code":null,"message":{},"path":null,"start":null,"end":null,"label":null,"labels":[],"notes":{notes},"help":{help},"spelled":null,"expansion":[],"included_from":[],"fix":[]}}"#,
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

/// What `--diag gitlab` has found so far. The report is one array, so it
/// is written once, at the end of the run, rather than as each file is.
static FOUND: Mutex<Vec<Finding>> = Mutex::new(Vec::new());

/// A diagnostic as much as a Code Quality report holds of it.
struct Finding {
    severity: Severity,
    code: String,
    message: String,
    path: String,
    line: u32,
}

/// Keeps `one` for the report [`write_gitlab`] writes.
pub fn find(sources: &Sources, one: &Resolved) {
    let diagnostic = one.diagnostic;
    let finding = Finding {
        severity: diagnostic.severity,
        code: diagnostic.code.to_string(),
        message: diagnostic.message.clone(),
        path: relative(&sources.name(one.at.src_id)),
        line: sources.origins().line_col(one.at.src_id, one.at.start).line,
    };
    FOUND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(finding);
}

/// `path` from the checkout's root, which GitLab names a file from, if it
/// is in it: the directory `CI_PROJECT_DIR` names in a job, and the one the
/// run is in elsewhere.
fn relative(path: &str) -> String {
    let root = std::env::var_os("CI_PROJECT_DIR").map(PathBuf::from);
    let within = (root.or_else(|| std::env::current_dir().ok()))
        .and_then(|dir| Some(Path::new(path).strip_prefix(dir).ok()?.to_path_buf()));
    match within {
        Some(within) => within.display().to_string(),
        None => path.strip_prefix("./").unwrap_or(path).to_string(),
    }
}

/// Writes everything [`find`] kept as a Code Quality report, an array with
/// an object for each diagnostic, and `[]` for none, which GitLab reads as
/// a clean run.
pub fn write_gitlab(to: &mut dyn io::Write) -> io::Result<()> {
    let found = FOUND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut lines = Vec::with_capacity(found.len());
    let mut seen = HashMap::new();
    for finding in found.iter() {
        // Two findings alike in a file are told apart by which comes first,
        // and not by their line, so that an edit above them keeps both.
        let earlier = seen
            .entry((&finding.path, &finding.code, &finding.message))
            .or_insert(0usize);
        *earlier += 1;
        let earlier = *earlier - 1;
        let severity = match finding.severity {
            Severity::Error => "major",
            Severity::Warning => "minor",
            Severity::Note | Severity::Help => "info",
        };
        let fingerprint = fingerprint(&[
            &finding.path,
            &finding.code,
            &finding.message,
            &earlier.to_string(),
        ]);
        lines.push(format!(
            r#"{{"description":{},"check_name":{},"fingerprint":"{fingerprint}","severity":"{severity}","location":{{"path":{},"lines":{{"begin":{}}}}}}}"#,
            string(&format!("[{}] {}", finding.code, finding.message)),
            string(&finding.code),
            string(&finding.path),
            finding.line,
        ));
    }
    match lines.is_empty() {
        true => writeln!(to, "[]"),
        false => writeln!(to, "[\n{}\n]", lines.join(",\n")),
    }
}

/// A 64-bit FNV-1a hash of `parts`, in hex. GitLab compares a merge
/// request's findings with its target's by fingerprint, so it must not
/// change between runs, or between builds of astli, as `std`'s hasher may.
fn fingerprint(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        // A zero between parts keeps `ab`, `c` apart from `a`, `bc`.
        for byte in part.bytes().chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}
