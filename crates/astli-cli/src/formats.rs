//! The formats `--diag` writes a diagnostic in besides pretty: a line for
//! each under `short`, a GitHub Actions workflow command under `github`, a
//! line of JSON under `json`, and under `gitlab` the Code Quality report
//! GitLab turns into annotations on a merge request.
//!
//! What each writes is what the command promises, so they live here rather
//! than in `astli-diag`, whose pretty rendering is for a person to read and
//! may change. Each names a place as [`Resolved::place`] has it.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use astli_diag::{Place, Resolved};
use astli_text::{Origins, Severity, Span};

use crate::json::{hash, string};

/// Writes a resolved diagnostic as one line, `file:line:col: severity[code]:
/// message`, the shape editors and CI log scanners look for.
///
/// The place is where the user wrote the text, as the pretty format has it. The source,
/// the macro chain, and the notes are left out: a line holds one location.
pub fn short(out: &mut dyn io::Write, resolved: &Resolved) -> io::Result<()> {
    let diagnostic = resolved.diagnostic;
    let place = &resolved.place;
    writeln!(
        out,
        "{}:{}: {}[{}]: {}",
        place.path, place.start, diagnostic.severity, diagnostic.code, diagnostic.message
    )
}

/// Writes a resolved diagnostic as a GitHub Actions workflow command, which
/// the runner turns into an annotation on the line of the pull request's diff.
///
/// The command's message is the line [`short`] writes, since the log
/// shows the message alone, and the place in it is what a reader of the log
/// looks for.
pub fn github(out: &mut dyn io::Write, resolved: &Resolved) -> io::Result<()> {
    let diagnostic = resolved.diagnostic;
    let Place {
        path: file,
        start,
        end,
    } = &resolved.place;
    let command = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note | Severity::Help => "notice",
    };
    let message = format!(
        "{file}:{start}: {}[{}]: {}",
        diagnostic.severity, diagnostic.code, diagnostic.message
    );
    writeln!(
        out,
        "::{command} title={},file={},line={},col={},endLine={},endColumn={}::{}",
        property(&diagnostic.code.to_string()),
        property(file),
        start.line,
        start.col,
        end.line,
        end.col,
        data(&message)
    )
}

/// Escapes a workflow command's message, which ends at the line.
fn data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a workflow command's property, which also ends at a comma, and its
/// list at a colon.
fn property(text: &str) -> String {
    data(text).replace(':', "%3A").replace(',', "%2C")
}

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
pub fn line(origins: &Origins, one: &Resolved) -> String {
    let diagnostic = one.diagnostic;
    let labels = list(one.labels.iter().map(|(at, message)| {
        format!(
            r#"{{"message":{},{}}}"#,
            string(message),
            elsewhere(origins, *at)
        )
    }));
    let notes = list(diagnostic.notes.iter().map(|note| string(note)));
    let help = list(diagnostic.help.iter().map(|help| string(help)));
    let spelled = match one.spelled {
        Some(at) => format!("{{{}}}", elsewhere(origins, at)),
        None => "null".to_string(),
    };
    let expansion = list(one.through.iter().map(|through| {
        format!(
            r#"{{"macro":{},{}}}"#,
            string(&through.name),
            elsewhere(origins, through.call)
        )
    }));
    let included_from =
        list((one.included_from.iter()).map(|&at| format!("{{{}}}", elsewhere(origins, at))));
    let fix = list((diagnostic.fix.iter()).map(|edit| {
        format!(
            r#"{{"text":{},{}}}"#,
            string(&edit.text),
            elsewhere(origins, edit.at)
        )
    }));
    format!(
        r#"{{"severity":"{}","code":{},"message":{},{},"label":{},"labels":{labels},"notes":{notes},"help":{help},"spelled":{spelled},"expansion":{expansion},"included_from":{included_from},"fix":{fix}}}"#,
        diagnostic.severity,
        string(diagnostic.code.as_str()),
        string(&diagnostic.message),
        place(&one.place, one.at),
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

/// The `path`, `start` and `end` keys of `at`, which is at `place`, a
/// position being its line and column, both from one and the column in
/// characters, and its byte offset.
fn place(place: &Place, at: Span) -> String {
    let position = |at: astli_text::LineCol, offset: u32| {
        format!(
            r#"{{"line":{},"column":{},"offset":{offset}}}"#,
            at.line, at.col
        )
    };
    format!(
        r#""path":{},"start":{},"end":{}"#,
        string(&place.path),
        position(place.start, at.start),
        position(place.end, at.end)
    )
}

/// The [`place`] keys of `at`, a span other than the one reported, such as
/// a label's or a macro call's.
fn elsewhere(origins: &Origins, at: Span) -> String {
    place(&Place::of(origins, at), at)
}

/// `items`, each already JSON, as an array.
fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(","))
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
pub fn find(one: &Resolved) {
    let diagnostic = one.diagnostic;
    let finding = Finding {
        severity: diagnostic.severity,
        code: diagnostic.code.to_string(),
        message: diagnostic.message.clone(),
        path: relative(&one.place.path),
        line: one.place.start.line,
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

/// A fingerprint of `parts`, as [`hash`] makes one. GitLab compares a
/// merge request's findings with its target's by fingerprint.
fn fingerprint(parts: &[&str]) -> String {
    hash(parts.iter().map(|part| part.as_bytes()))
}
