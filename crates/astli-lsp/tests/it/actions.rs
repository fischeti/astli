//! Code actions, the fixes and waivers of what the server published, asked
//! of designs the support host lowers once each has loaded.
//!
//! Every `tests/data/actions/*.sv` is a case, opened with the `.svh` beside
//! it, if it has one, under UTF-16, by a client that takes versioned edits.
//! For each diagnostic published for each file, the quick fixes asked for
//! it are written, then what fixing all of the file gives: each action as
//! its kind and title and the lines its edit changes, as a diff. Each
//! waiver is then applied to the buffer and the design loaded again, and
//! must take a finding of its rule away without an `invalid-waiver`. A
//! diagnostic is written as its range, each position `line:character` from
//! 1, and its code. A case's text holding `restriction` is linted with that
//! group's rules on too. Adding a case means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp actions::
//! ```

use std::fmt::Write;
use std::path::Path;

use astli_testing::snapshots;
use lsp_types::{CodeAction, Diagnostic, Position, Range, TextEdit, WorkspaceEdit};
use serde_json::{Value, json};
use similar::TextDiff;

use crate::support::{self, Client, loaded};

/// What a client that takes versioned edits says.
fn versioned() -> Value {
    json!({ "workspace": { "workspaceEdit": { "documentChanges": true } } })
}

/// Asks the actions at `range` of `uri`, naming `diagnostics`, of the
/// kinds `only` names, if it names any.
fn ask(
    client: &mut Client,
    uri: &str,
    range: Range,
    diagnostics: &[Diagnostic],
    only: Option<&[&str]>,
) -> Vec<CodeAction> {
    let mut context = json!({ "diagnostics": diagnostics });
    if let Some(only) = only {
        context["only"] = json!(only);
    }
    let params = json!({ "textDocument": { "uri": uri }, "range": range, "context": context });
    let answer = client.request("textDocument/codeAction", params);
    serde_json::from_value(answer.expect("answered")).expect("actions")
}

/// What was last published for `uri`.
fn published(client: &mut Client, uri: &str) -> Vec<Diagnostic> {
    let published = client.latest(uri).expect("published");
    (published.diagnostics.into_iter())
        .map(|it| serde_json::from_value(it).expect("a diagnostic"))
        .collect()
}

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/actions");
    snapshots::check_with(&root, "actions", &["svh"], |name, text| {
        let case = root.join(name);
        let mut opened = vec![(case.clone(), text)];
        let header = case.with_extension("svh");
        if header.exists() {
            let text = std::fs::read_to_string(&header).unwrap();
            opened.push((header, text));
        }
        let names: Vec<String> = (opened.iter())
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut client = Client::designing(&names, versioned());
        for (path, text) in &opened {
            client.open(&uri(path), text);
        }
        client.logged(loaded);
        let mut out = String::new();
        let mut waivers = Vec::new();
        let mut version = 1;
        for (path, text) in &opened {
            let (file, uri) = (path.file_name().unwrap().to_string_lossy(), uri(path));
            let found = published(&mut client, &uri);
            for diagnostic in found.clone() {
                let code = match &diagnostic.code {
                    Some(lsp_types::Code::String(code)) => code.clone(),
                    code => format!("{code:?}"),
                };
                writeln!(out, "{file} {} {code}", range(diagnostic.range)).unwrap();
                let asked = std::slice::from_ref(&diagnostic);
                let quick = ask(
                    &mut client,
                    &uri,
                    diagnostic.range,
                    asked,
                    Some(&["quickfix"]),
                );
                for (title, waived) in written(&mut out, &uri, text, quick) {
                    if title.starts_with("Waive") {
                        waivers.push((code.clone(), waived));
                    }
                }
            }
            writeln!(out, "{file} all").unwrap();
            let start = Range::default();
            let all = ask(&mut client, &uri, start, &[], Some(&["source.fixAll"]));
            written(&mut out, &uri, text, all);
            // Each waiver, written and linted again with the design, takes
            // a finding of its rule away and is not itself reported.
            let count = |found: &[Diagnostic], code: &str| {
                let codes = found.iter().filter_map(|it| match &it.code {
                    Some(lsp_types::Code::String(it)) => Some(it.as_str()),
                    _ => None,
                });
                let codes: Vec<&str> = codes.collect();
                assert!(!codes.contains(&"invalid-waiver"), "{codes:?}");
                codes.iter().filter(|it| **it == code).count()
            };
            let before: Vec<usize> = (waivers.iter())
                .map(|(code, _)| count(&found, code))
                .collect();
            for ((code, waived), before) in waivers.drain(..).zip(before) {
                version += 1;
                client.change(&uri, version, json!([{ "text": waived }]));
                client.logged(loaded);
                let after = count(&published(&mut client, &uri), &code);
                assert!(after < before, "{file}: `{code}` not waived by\n{waived}");
            }
        }
        client.finish();
        out
    });
}

/// Writes each of `actions`, fixes of `text` at `uri`, to `out`, or that
/// there are none, and hands back each one's title and the text it gives.
fn written(
    out: &mut String,
    uri: &str,
    text: &str,
    actions: Vec<CodeAction>,
) -> Vec<(String, String)> {
    let mut given = Vec::new();
    if actions.is_empty() {
        writeln!(out, "  nothing").unwrap();
    }
    for action in actions {
        let kind = action.kind.expect("a kind").to_string();
        writeln!(out, "  {kind} \"{}\"", action.title).unwrap();
        let (version, edits) = versioned_edits(action.edit.expect("an edit"), uri);
        assert_eq!(version, Some(1), "an edit names the version it is for");
        let fixed = applied(text, &edits);
        let diff = TextDiff::from_lines(text, &fixed);
        for line in diff.unified_diff().context_radius(0).to_string().lines() {
            writeln!(out, "    {line}").unwrap();
        }
        given.push((action.title, fixed));
    }
    given
}

/// The version and the edits of `edit`, which changes `uri` alone.
fn versioned_edits(edit: WorkspaceEdit, uri: &str) -> (Option<i32>, Vec<TextEdit>) {
    let edit = serde_json::to_value(edit).unwrap();
    let changes = edit["documentChanges"]
        .as_array()
        .expect("document changes");
    assert_eq!(changes.len(), 1);
    let document = &changes[0]["textDocument"];
    assert_eq!(document["uri"], uri);
    let version = document["version"].as_i64().map(|it| it as i32);
    let edits = serde_json::from_value(changes[0]["edits"].clone()).expect("edits");
    (version, edits)
}

/// `text` with `edits`, which do not overlap, applied.
fn applied(text: &str, edits: &[TextEdit]) -> String {
    let mut edits: Vec<&TextEdit> = edits.iter().collect();
    edits.sort_by_key(|it| std::cmp::Reverse(it.range.start));
    let mut text = text.to_string();
    for edit in edits {
        let (start, end) = (
            offset(&text, edit.range.start),
            offset(&text, edit.range.end),
        );
        text.replace_range(start..end, &edit.new_text);
    }
    text
}

/// The byte `position` names in `text`, its character counted in UTF-16
/// code units.
fn offset(text: &str, position: Position) -> usize {
    let start: usize = (text.split_inclusive('\n'))
        .take(position.line as usize)
        .map(str::len)
        .sum();
    let mut counted = 0;
    for (at, char) in text[start..].char_indices() {
        if counted >= position.character || char == '\n' {
            return start + at;
        }
        counted += char.len_utf16() as u32;
    }
    text.len()
}

fn range(range: Range) -> String {
    let at = |it: Position| format!("{}:{}", it.line + 1, it.character + 1);
    format!("{}-{}", at(range.start), at(range.end))
}

fn uri(path: &Path) -> String {
    lsp_types::Uri::from_file_path(path)
        .expect("absolute")
        .to_string()
}

const TEXT: &str = "module a;\n  reg q;\n  reg r;\nendmodule\n";

/// A server whose design reads `a.sv` and `b.svh`, for a client that says
/// `capabilities`, with `a.sv` opened holding `text` and loaded, and what
/// was published for it.
fn opened(text: &str, capabilities: Value) -> (Client, String, Vec<Diagnostic>) {
    let mut client = Client::designing(&["a.sv", "b.svh"], capabilities);
    let uri = support::uri("a.sv");
    client.open(&uri, text);
    client.logged(loaded);
    let found = published(&mut client, &uri);
    (client, uri, found)
}

#[test]
fn without_only_both_kinds_are_offered_as_plain_changes() {
    let (mut client, uri, found) = opened(TEXT, json!({}));
    let actions = ask(&mut client, &uri, found[0].range, &found[..1], None);
    let kinds: Vec<String> = (actions.iter())
        .map(|it| it.kind.clone().unwrap().to_string())
        .collect();
    // The finding's fix and its waiver, then the whole file fixed.
    assert_eq!(kinds, ["quickfix", "quickfix", "source.fixAll.astli"]);
    // A client that takes no versioned edits is given them by URI.
    let edit = actions[2].edit.as_ref().unwrap();
    let changes = edit.changes.as_ref().expect("changes");
    let edits = &changes[&uri.parse().unwrap()];
    assert_eq!(
        applied(TEXT, edits),
        "module a;\n  logic q;\n  logic r;\nendmodule\n"
    );
    // `only` names a kind neither is under.
    let refactors = ask(
        &mut client,
        &uri,
        found[0].range,
        &found,
        Some(&["refactor"]),
    );
    assert!(refactors.is_empty(), "{refactors:?}");
    // `quickfix` alone, for every diagnostic: two fixes, and a waiver of
    // each finding.
    let quick = ask(
        &mut client,
        &uri,
        found[0].range,
        &found,
        Some(&["quickfix"]),
    );
    let fixes = quick.iter().filter(|it| it.is_preferred == Some(true));
    assert_eq!(fixes.count(), 2, "{quick:?}");
    assert_eq!(quick.len(), 2 + found.len(), "{quick:?}");
    client.finish();
}

#[test]
fn a_diagnostic_of_an_older_version_is_offered_nothing() {
    let (mut client, uri, old) = opened(TEXT, versioned());
    // A line above moves each finding down one.
    client.replace(&uri, &format!("// moved\n{TEXT}"));
    let new = published(&mut client, &uri);
    let quick = Some(&["quickfix"][..]);
    let stale = ask(&mut client, &uri, old[0].range, &old[..1], quick);
    assert!(stale.is_empty(), "{stale:?}");
    // Its fix and its waiver.
    let current = ask(&mut client, &uri, new[0].range, &new[..1], quick);
    assert_eq!(current.len(), 2, "{current:?}");
    for action in current {
        let (version, _) = versioned_edits(action.edit.unwrap(), &uri);
        assert_eq!(version, Some(2));
    }
    client.finish();
}

#[test]
fn a_fix_is_offered_only_in_its_own_file() {
    let text = "`include \"b.svh\"\nmodule a;\nendmodule\n";
    let (mut client, uri, _) = opened(text, versioned());
    let header = support::uri("b.svh");
    client.open(&header, "reg shared;\n");
    // The header did not exist when the design was first loaded, so it is
    // linted once the load it makes reads it.
    let found = client.published(&header, |it| it.codes().contains(&"forbid-reg"));
    let found =
        (found.diagnostics.into_iter()).map(|it| serde_json::from_value::<Diagnostic>(it).unwrap());
    let reg: Vec<Diagnostic> = found
        .filter(|it| it.code == Some(lsp_types::Code::String("forbid-reg".to_string())))
        .collect();
    assert_eq!(reg.len(), 1);
    // Asked of the unit that includes the header, the header's finding has
    // nothing to offer; asked of the header, it is fixed or waived there.
    let asked = ask(&mut client, &uri, reg[0].range, &reg, None);
    assert!(asked.is_empty(), "{asked:?}");
    let asked = ask(
        &mut client,
        &header,
        reg[0].range,
        &reg,
        Some(&["quickfix"]),
    );
    assert_eq!(asked.len(), 2, "{asked:?}");
    client.finish();
}
