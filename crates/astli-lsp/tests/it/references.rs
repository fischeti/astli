//! Find references and document highlights, asked of designs the support
//! host lowers once each has loaded.
//!
//! Every `tests/data/references/**/*.sv` is a case, read as definition's
//! are (`definition.rs`): the `.sv` files of one directory are one design,
//! a case is opened with the `.svh` beside it, and each place a `// ^`
//! line marks in either is asked about. Each answer is written as the file
//! and range of each reference and the text there, with `(declaration)`
//! after those the client leaves out when it asks without
//! `includeDeclaration`. The highlights of the same place are held to be
//! the references in the asked file, declarations among them. Adding a
//! case means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp references::
//! ```

use std::fmt::Write;
use std::path::Path;

use astli_testing::snapshots;
use lsp_types::{DocumentHighlight, Location, Position, Range};
use serde_json::json;

use crate::definition::{marked, range, spelled, uri, written};
use crate::support::{Client, loaded};

/// Asks where what stands at `position` of `uri` is named.
fn ask(
    client: &mut Client,
    uri: &str,
    position: Position,
    declarations: bool,
) -> Result<Vec<Location>, i32> {
    let params = json!({
        "textDocument": { "uri": uri },
        "position": position,
        "context": { "includeDeclaration": declarations },
    });
    let answer = client.request("textDocument/references", params);
    answer
        .map(|it| serde_json::from_value::<Option<Vec<Location>>>(it).expect("locations"))
        .map(Option::unwrap_or_default)
        .map_err(|it| it.code)
}

/// The ranges highlighted with what stands at `position` of `uri`.
fn highlighted(client: &mut Client, uri: &str, position: Position) -> Vec<Range> {
    let params = json!({ "textDocument": { "uri": uri }, "position": position });
    let answer = client.request("textDocument/documentHighlight", params);
    let found: Option<Vec<DocumentHighlight>> =
        serde_json::from_value(answer.expect("answered")).expect("highlights");
    (found.unwrap_or_default().into_iter())
        .map(|it| {
            assert_eq!(it.kind, None, "the sites tell no read from a write");
            it.range
        })
        .collect()
}

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/references");
    snapshots::check_with(&root, "references", &["svh"], |name, text| {
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
            let asked = uri(path);
            for position in marked(text) {
                writeln!(out, "{file} {}", written(position)).unwrap();
                let all = ask(&mut client, &asked, position, true).expect("answered");
                let uses = ask(&mut client, &asked, position, false).expect("answered");
                if all.is_empty() {
                    writeln!(out, "  nothing").unwrap();
                }
                for location in &all {
                    let path = location.uri.to_file_path().expect("a file");
                    // Written with `/` on every platform, as the snapshot is.
                    let target = path.strip_prefix(&root).unwrap_or(&path);
                    let target = target.to_string_lossy().replace('\\', "/");
                    let said = spelled(&path, location.range);
                    let at = range(location.range);
                    let declared = match uses.contains(location) {
                        true => "",
                        false => " (declaration)",
                    };
                    writeln!(out, "  {target} {at} `{said}`{declared}").unwrap();
                }
                assert!(
                    uses.iter().all(|it| all.contains(it)),
                    "{uses:?} of {all:?}"
                );
                let here: Vec<Range> = (all.iter())
                    .filter(|it| it.uri.as_str() == asked)
                    .map(|it| it.range)
                    .collect();
                assert_eq!(highlighted(&mut client, &asked, position), here);
            }
        }
        client.finish();
        out
    });
}
